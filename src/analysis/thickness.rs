// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Local stroke thickness over the whole ink of a glyph: the field behind
//! the thickness overlay.
//!
//! Thickness is defined as in Hildebrand and Rüegsegger (1997): at a point
//! `p` inside the shape it is the diameter of the largest disc that contains
//! `p` and lies wholly inside the shape.
//! The definition needs no stroke direction, so it holds at junctions,
//! terminals and corners as well as along a stem, and the field has no gaps.
//!
//! The computation follows the standard pipeline on a grid:
//!
//! 1. Rasterize the outline into an inside mask by nonzero winding.
//! 2. Take the exact Euclidean distance transform of the mask with the
//!    separable lower-envelope algorithm of Felzenszwalb and Huttenlocher
//!    (*Distance Transforms of Sampled Functions*, Theory of Computing 2012).
//!    Each inside cell now holds the radius of the largest disc centered on it.
//! 3. Keep the distance ridge: the cells whose disc is not contained in a
//!    neighbor's disc.
//!    Refine each ridge radius against the true outline, so the result is not
//!    quantized to the grid.
//!    Prune the branches that run into convex corners, where the maximal disc
//!    touches the outline over a narrow angle: the usual separation-angle
//!    pruning of a medial axis.
//! 4. Cover: visit ridge discs from largest to smallest and write each
//!    diameter into every cell it covers that has no larger value yet.
//!    This is the disc-covering step of the `ImageJ` *Local Thickness* plugin
//!    (Dougherty and Kunzelmann 2007).
//!    Ink the pruned branches used to cover, the corners of square terminals,
//!    takes the value of the nearest covered ink.
//!
//! Colors are normalized per glyph around the area-weighted median of the
//! field, so one color means one thickness everywhere in the glyph.
//!
//! The outline is read as drawn: an edge inside an overlap of two contours
//! counts as boundary, so overlapping contours should be merged first.
//!
//! Pure design-space geometry with no render deps, so it unit-tests on
//! native `cargo test`.

use kurbo::{
    Affine, BezPath, Line, ParamCurve as _, ParamCurveDeriv as _, ParamCurveNearest as _, PathSeg,
    Point, Rect, Shape as _,
};

/// The thickness field of a glyph, sampled on a grid over its bounding box.
///
/// Row 0 is the top of the glyph and column 0 its left, the image order, so
/// [`ThicknessField::pixel_to_design`] places it on the canvas directly.
#[derive(Clone, Debug, PartialEq)]
pub struct ThicknessField {
    /// Design-space position of the grid's top-left corner.
    pub origin: Point,
    /// Design units per cell.
    pub cell: f64,
    /// Cells per row.
    pub width: usize,
    /// Rows.
    pub height: usize,
    /// Local thickness per cell in design units, row-major; 0 outside the ink.
    pub thickness: Vec<f32>,
    /// Coverage per cell in 0..=1: 1 well inside the ink, fading over the
    /// last cell to the edge, 0 outside.
    pub coverage: Vec<f32>,
    /// Area-weighted median thickness, or 0 when there is no ink.
    pub reference: f64,
}

impl ThicknessField {
    /// How far either side of the reference the color ramp reaches, as a
    /// fraction of the reference. Narrow, so small changes in weight show.
    pub const RAMP_WINDOW: f64 = 0.2;

    /// Normalized position of `thickness` on the color ramp.
    ///
    /// The reference maps to 0.5; `RAMP_WINDOW` below it or thinner maps to
    /// 0, the same above it or thicker to 1.
    pub fn ramp(&self, thickness: f64) -> f64 {
        if self.reference <= 1e-9 {
            return 0.5;
        }
        let relative = thickness / self.reference - 1.0;
        (0.5 + relative / (2.0 * Self::RAMP_WINDOW)).clamp(0.0, 1.0)
    }

    /// Maps pixel coordinates (x right, y down, one unit per cell) to design space.
    pub fn pixel_to_design(&self) -> Affine {
        Affine::new([
            self.cell,
            0.0,
            0.0,
            -self.cell,
            self.origin.x,
            self.origin.y,
        ])
    }

    /// Thickness at a design-space point, or `None` outside the ink.
    pub fn at(&self, point: Point) -> Option<f64> {
        let (i, j) = self.cell_of(point)?;
        let value = self.thickness[j * self.width + i];
        (value > 0.0).then_some(f64::from(value))
    }

    fn cell_of(&self, point: Point) -> Option<(usize, usize)> {
        let x = (point.x - self.origin.x) / self.cell;
        let y = (self.origin.y - point.y) / self.cell;
        if x < 0.0 || y < 0.0 {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "non-negative and bounded by the grid size, which is small"
        )]
        let (i, j) = (x.floor() as usize, y.floor() as usize);
        (i < self.width && j < self.height).then_some((i, j))
    }
}

/// Compute the thickness field of `outline` on a grid about `cells` wide
/// along the outline's longer side.
///
/// Returns `None` for an empty outline.
pub fn thickness_field(outline: &BezPath, cells: usize) -> Option<ThicknessField> {
    let bounds = outline.bounding_box();
    if bounds.is_zero_area() || !bounds.is_finite() {
        return None;
    }
    let cells = cells.clamp(8, 2048) as f64;
    let cell = bounds.size().max_side() / cells;
    // Cell centers sit off integer coordinates, so a scanline never passes
    // exactly through a node, where a crossing count is ambiguous.
    let pad = cell * 1.5 + cell * 0.207;
    let origin = Point::new(bounds.x0 - pad, bounds.y1 + pad);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "positive and bounded by the clamped cell count plus padding"
    )]
    let width = ((bounds.width() + 2.0 * pad) / cell).ceil() as usize;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "positive and bounded by the clamped cell count plus padding"
    )]
    let height = ((bounds.height() + 2.0 * pad) / cell).ceil() as usize;
    let grid = Grid {
        origin,
        cell,
        width,
        height,
    };

    let inside = rasterize(outline, &grid);
    if !inside.iter().any(|&v| v) {
        return None;
    }
    let squared = distance_transform(&inside, width, height);
    let segments: Vec<PathSeg> = outline.segments().collect();
    let ridge = ridge(&squared, width, height, &grid, &segments);
    let mut thickness = cover(&ridge, &inside, &grid);
    fill_uncovered(&mut thickness, &inside, &grid);

    let mut coverage = vec![0.0_f32; width * height];
    for (k, value) in coverage.iter_mut().enumerate() {
        if inside[k] {
            // Distance to the nearest outside center, in cells: 1 on the
            // edge row, so the edge fades over one cell.
            *value = narrow((squared[k].sqrt() - 0.5).clamp(0.0, 1.0));
        }
    }

    let mut values: Vec<f32> = thickness.iter().copied().filter(|&t| t > 0.0).collect();
    values.sort_by(f32::total_cmp);
    let reference = if values.is_empty() {
        0.0
    } else {
        f64::from(values[values.len() / 2])
    };

    Some(ThicknessField {
        origin,
        cell,
        width,
        height,
        thickness,
        coverage,
        reference,
    })
}

/// A value stored in the field's `f32` grids.
#[expect(
    clippy::cast_possible_truncation,
    reason = "design-unit values fit an f32 comfortably; the grid stores single precision"
)]
fn narrow(value: f64) -> f32 {
    value as f32
}

struct Grid {
    origin: Point,
    cell: f64,
    width: usize,
    height: usize,
}

impl Grid {
    fn center(&self, i: usize, j: usize) -> Point {
        Point::new(
            self.origin.x + (i as f64 + 0.5) * self.cell,
            self.origin.y - (j as f64 + 0.5) * self.cell,
        )
    }

    /// The cells within `radius` cells of `(i, j)` in each direction, clipped
    /// to the grid: the row range and, for a row, the column range.
    fn rows_near(&self, j: usize, radius: usize) -> std::ops::RangeInclusive<usize> {
        j.saturating_sub(radius)..=(j + radius).min(self.height - 1)
    }

    fn columns_near(&self, i: usize, radius: usize) -> std::ops::RangeInclusive<usize> {
        i.saturating_sub(radius)..=(i + radius).min(self.width - 1)
    }
}

/// Inside mask by nonzero winding, one scanline per row of cell centers.
fn rasterize(outline: &BezPath, grid: &Grid) -> Vec<bool> {
    let mut inside = vec![false; grid.width * grid.height];
    let x0 = grid.origin.x - grid.cell;
    let x1 = grid.origin.x + (grid.width as f64 + 1.0) * grid.cell;
    let segments: Vec<PathSeg> = outline.segments().collect();
    let mut crossings: Vec<(f64, i32)> = Vec::new();
    for j in 0..grid.height {
        let y = grid.center(0, j).y;
        let scan = Line::new((x0, y), (x1, y));
        crossings.clear();
        for seg in &segments {
            for hit in seg.intersect_line(scan) {
                let dy = match seg {
                    PathSeg::Line(l) => l.p1.y - l.p0.y,
                    PathSeg::Quad(q) => q.deriv().eval(hit.segment_t).y,
                    PathSeg::Cubic(c) => c.deriv().eval(hit.segment_t).y,
                };
                if dy.abs() > 1e-12 {
                    crossings.push((scan.eval(hit.line_t).x, if dy > 0.0 { 1 } else { -1 }));
                }
            }
        }
        crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut winding = 0;
        let mut next = 0;
        for i in 0..grid.width {
            let x = grid.center(i, j).x;
            while next < crossings.len() && crossings[next].0 <= x {
                winding += crossings[next].1;
                next += 1;
            }
            inside[j * grid.width + i] = winding != 0;
        }
    }
    inside
}

/// Exact squared Euclidean distance from every cell to the nearest outside
/// cell, in cell units, by the separable lower-envelope algorithm.
fn distance_transform(inside: &[bool], width: usize, height: usize) -> Vec<f64> {
    const INF: f64 = 1e20;
    let mut d: Vec<f64> = inside.iter().map(|&v| if v { INF } else { 0.0 }).collect();
    let n = width.max(height);
    let mut f = vec![0.0; n];
    let mut out = vec![0.0; n];
    let mut v = vec![0_usize; n];
    let mut z = vec![0.0; n + 1];
    // Columns.
    for i in 0..width {
        for j in 0..height {
            f[j] = d[j * width + i];
        }
        transform_1d(&f[..height], &mut out[..height], &mut v, &mut z);
        for j in 0..height {
            d[j * width + i] = out[j];
        }
    }
    // Rows.
    for j in 0..height {
        let row = &mut d[j * width..(j + 1) * width];
        f[..width].copy_from_slice(row);
        transform_1d(&f[..width], &mut out[..width], &mut v, &mut z);
        row.copy_from_slice(&out[..width]);
    }
    d
}

/// One-dimensional squared-distance transform of a sampled function `f`:
/// `out[q] = min_p ((q - p)² + f[p])`, as the lower envelope of parabolas.
fn transform_1d(f: &[f64], out: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    let sq = |x: usize| (x * x) as f64;
    for q in 1..n {
        loop {
            let p = v[k];
            // Intersection of the parabolas rooted at p and q.
            let s = ((f[q] + sq(q)) - (f[p] + sq(p))) / (2.0 * (q as f64 - p as f64));
            if s <= z[k] {
                if k == 0 {
                    // The new parabola dominates everything so far.
                    v[0] = q;
                    z[0] = f64::NEG_INFINITY;
                    z[1] = f64::INFINITY;
                    break;
                }
                k -= 1;
            } else {
                k += 1;
                v[k] = q;
                z[k] = s;
                z[k + 1] = f64::INFINITY;
                break;
            }
        }
    }
    k = 0;
    for (q, value) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let diff = q as f64 - p as f64;
        *value = diff * diff + f[p];
    }
}

/// A disc center on the distance ridge, with its radius in design units.
struct Disc {
    i: usize,
    j: usize,
    radius: f64,
}

/// The distance ridge: inside cells whose disc no 8-neighbor's disc
/// contains, each with its radius refined against the true outline.
fn ridge(
    squared: &[f64],
    width: usize,
    height: usize,
    grid: &Grid,
    segments: &[PathSeg],
) -> Vec<Disc> {
    let mut discs = Vec::new();
    for j in 0..height {
        for i in 0..width {
            let r = squared[j * width + i].sqrt();
            if r <= 0.0 {
                continue;
            }
            let mut contained = false;
            'search: for nj in grid.rows_near(j, 1) {
                for ni in grid.columns_near(i, 1) {
                    if ni == i && nj == j {
                        continue;
                    }
                    let rn = squared[nj * width + ni].sqrt();
                    let step = if ni == i || nj == j {
                        1.0
                    } else {
                        std::f64::consts::SQRT_2
                    };
                    // Strict on ties, so a flat ridge keeps one center, not none.
                    let later = (nj, ni) > (j, i);
                    if rn > r + step - 1e-9 || ((rn - (r + step)).abs() < 1e-9 && later) {
                        contained = true;
                        break 'search;
                    }
                }
            }
            if contained {
                continue;
            }
            let (radius, separation) =
                maximal_disc(grid.center(i, j), r * grid.cell, grid.cell, segments);
            if separation < MIN_SEPARATION_RADIANS {
                continue;
            }
            discs.push(Disc { i, j, radius });
        }
    }
    discs
}

/// Smallest angle, seen from a disc's center, between the outline points it
/// touches, for the disc to count as spanning a stroke.
///
/// A disc on a branch into a convex corner touches the two edges of the
/// corner, at most 90° apart for a square corner. A disc across a stroke
/// touches both sides, near 180° apart.
const MIN_SEPARATION_RADIANS: f64 = 100.0_f64.to_radians();

/// Radius of the maximal inscribed disc near `start`, a ridge cell center
/// whose grid radius is about `estimate`, and the largest angle between the
/// outline points the disc touches.
///
/// The distance to the outline is locally maximal on the medial axis, so a
/// pattern search climbing that function from the cell center lands on the
/// axis to within a fraction of a cell, and the distance there is the radius.
fn maximal_disc(start: Point, estimate: f64, cell: f64, segments: &[PathSeg]) -> (f64, f64) {
    const LEVELS: u32 = 4;
    const MOVES_PER_LEVEL: usize = 8;
    const DIRECTIONS: [(f64, f64); 4] = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)];
    // The search walks at most this far from the cell, and the distance can
    // grow by at most that much, so a window this wide sees every segment
    // that can be nearest at any step.
    let walk = cell * MOVES_PER_LEVEL as f64;
    let reach = estimate + 2.0 * walk + cell;
    let distance = |p: Point| boundary_distance(p, reach, segments).unwrap_or(0.0);
    let mut center = start;
    let mut best = distance(center);
    let mut step = cell * 0.5;
    for _ in 0..LEVELS {
        let mut moves = 0;
        loop {
            let improved = DIRECTIONS
                .iter()
                .map(|&(dx, dy)| center + kurbo::Vec2::new(dx * step, dy * step))
                .map(|p| (p, distance(p)))
                .filter(|&(_, d)| d > best + 1e-9)
                .max_by(|a, b| a.1.total_cmp(&b.1));
            match improved {
                Some((p, d)) if moves < MOVES_PER_LEVEL => {
                    center = p;
                    best = d;
                    moves += 1;
                }
                _ => break,
            }
        }
        step *= 0.5;
    }
    (best, separation_angle(center, best, reach, cell, segments))
}

/// Largest angle, seen from `center`, between two outline points at the
/// disc's `radius`, within a tolerance of a fraction of a cell.
fn separation_angle(
    center: Point,
    radius: f64,
    reach: f64,
    cell: f64,
    segments: &[PathSeg],
) -> f64 {
    let window = Rect::from_center_size(center, (2.0 * reach, 2.0 * reach));
    let overlaps =
        |b: Rect| b.x1 >= window.x0 && b.x0 <= window.x1 && b.y1 >= window.y0 && b.y0 <= window.y1;
    let tolerance = radius + cell * 0.3;
    let touches: Vec<kurbo::Vec2> = segments
        .iter()
        .filter(|seg| overlaps(seg.bounding_box()))
        .filter_map(|seg| {
            let nearest = seg.nearest(center, 1e-6);
            (nearest.distance_sq.sqrt() <= tolerance).then(|| seg.eval(nearest.t) - center)
        })
        .collect();
    let mut widest = 0.0_f64;
    for (a, u) in touches.iter().enumerate() {
        for v in &touches[a + 1..] {
            let cos = u.dot(*v) / (u.hypot() * v.hypot()).max(1e-12);
            widest = widest.max(cos.clamp(-1.0, 1.0).acos());
        }
    }
    widest
}

/// Exact distance from `point` to the outline, looking only at segments
/// whose bounding box comes within `reach`.
fn boundary_distance(point: Point, reach: f64, segments: &[PathSeg]) -> Option<f64> {
    let window = Rect::from_center_size(point, (2.0 * reach, 2.0 * reach));
    // A plain overlap test: a horizontal or vertical line has a zero-area
    // bounding box, which `Rect::intersect` cannot tell from a miss.
    let overlaps =
        |b: Rect| b.x1 >= window.x0 && b.x0 <= window.x1 && b.y1 >= window.y0 && b.y0 <= window.y1;
    segments
        .iter()
        .filter(|seg| overlaps(seg.bounding_box()))
        .map(|seg| seg.nearest(point, 1e-6).distance_sq.sqrt())
        .min_by(f64::total_cmp)
}

/// Cover the ink with the ridge discs, largest first, writing each diameter
/// into every cell it covers that has no larger value yet.
fn cover(discs: &[Disc], inside: &[bool], grid: &Grid) -> Vec<f32> {
    let mut order: Vec<&Disc> = discs.iter().collect();
    order.sort_by(|a, b| b.radius.total_cmp(&a.radius));
    let mut thickness = vec![0.0_f32; grid.width * grid.height];
    for disc in order {
        let diameter = narrow(2.0 * disc.radius);
        // Cover half a cell beyond the radius, so neighboring discs meet.
        let r_cells = disc.radius / grid.cell + 0.5;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a disc radius in cells is positive and bounded by the grid size"
        )]
        let span = r_cells.ceil() as usize;
        for j in grid.rows_near(disc.j, span) {
            let dj = j as f64 - disc.j as f64;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a chord half-width in cells is non-negative and bounded by the radius"
            )]
            let half = (r_cells * r_cells - dj * dj).max(0.0).sqrt().floor() as usize;
            let row = j * grid.width;
            for i in grid.columns_near(disc.i, half) {
                let k = row + i;
                if inside[k] && thickness[k] < diameter {
                    thickness[k] = diameter;
                }
            }
        }
    }
    thickness
}

/// Give every inside cell the pruned branches left uncovered the value of
/// the nearest covered cell, by repeated dilation.
fn fill_uncovered(thickness: &mut [f32], inside: &[bool], grid: &Grid) {
    let width = grid.width;
    let mut pending: Vec<usize> = (0..inside.len())
        .filter(|&k| inside[k] && thickness[k] <= 0.0)
        .collect();
    let mut next = Vec::new();
    let mut writes: Vec<(usize, f32)> = Vec::new();
    while !pending.is_empty() {
        writes.clear();
        next.clear();
        for &k in &pending {
            let (i, j) = (k % width, k / width);
            let mut best = 0.0_f32;
            for nj in grid.rows_near(j, 1) {
                for ni in grid.columns_near(i, 1) {
                    best = best.max(thickness[nj * width + ni]);
                }
            }
            if best > 0.0 {
                writes.push((k, best));
            } else {
                next.push(k);
            }
        }
        if writes.is_empty() {
            // Nothing covered anywhere: an outline too thin for the grid.
            break;
        }
        for &(k, value) in &writes {
            thickness[k] = value;
        }
        std::mem::swap(&mut pending, &mut next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
        let mut path = BezPath::new();
        path.move_to((x0, y0));
        path.line_to((x0, y1));
        path.line_to((x1, y1));
        path.line_to((x1, y0));
        path.close_path();
        path
    }

    /// Brute-force squared distance transform, for checking the fast one.
    fn brute_force(inside: &[bool], width: usize, height: usize) -> Vec<f64> {
        let mut out = vec![0.0; width * height];
        for j in 0..height {
            for i in 0..width {
                if !inside[j * width + i] {
                    continue;
                }
                let mut best = f64::INFINITY;
                for jj in 0..height {
                    for ii in 0..width {
                        if !inside[jj * width + ii] {
                            let (dx, dy) = (ii as f64 - i as f64, jj as f64 - j as f64);
                            best = best.min(dx * dx + dy * dy);
                        }
                    }
                }
                out[j * width + i] = best;
            }
        }
        out
    }

    #[test]
    fn the_distance_transform_matches_brute_force() {
        let (w, h) = (23, 17);
        let mut inside = vec![true; w * h];
        for (k, v) in inside.iter_mut().enumerate() {
            let (i, j) = (k % w, k / w);
            if i == 0
                || j == 0
                || i == w - 1
                || j == h - 1
                || (i == 11 && j == 8)
                || (i > 15 && j < 4)
            {
                *v = false;
            }
        }
        assert_eq!(
            distance_transform(&inside, w, h),
            brute_force(&inside, w, h)
        );
    }

    #[test]
    fn a_bar_reads_its_height_everywhere_including_its_ends() {
        let field = thickness_field(&rect(0.0, 0.0, 200.0, 20.0), 200).expect("ink");
        assert!((field.reference - 20.0).abs() < 0.5, "{}", field.reference);
        for x in [1.0, 5.0, 50.0, 100.0, 195.0, 199.0] {
            for y in [1.0, 10.0, 19.0] {
                let t = field.at(Point::new(x, y)).expect("inside");
                assert!((t - 20.0).abs() < 0.75, "({x}, {y}) read {t}");
            }
        }
        assert!(field.at(Point::new(-5.0, 10.0)).is_none());
        assert!(field.at(Point::new(100.0, 30.0)).is_none());
    }

    #[test]
    fn a_ring_reads_its_wall_and_a_junction_reads_its_inscribed_disc() {
        // A square ring with 20 unit walls.
        let mut ring = rect(0.0, 0.0, 100.0, 100.0);
        ring.move_to((20.0, 20.0));
        ring.line_to((80.0, 20.0));
        ring.line_to((80.0, 80.0));
        ring.line_to((20.0, 80.0));
        ring.close_path();
        let field = thickness_field(&ring, 200).expect("ink");
        assert!((field.reference - 20.0).abs() < 0.5, "{}", field.reference);
        let wall = field.at(Point::new(10.0, 50.0)).expect("inside");
        assert!((wall - 20.0).abs() < 0.75, "{wall}");
        // The L corner holds a larger disc, touching both outer edges and the
        // inner corner: radius 20·√2 / (1 + √2).
        let corner = field.at(Point::new(10.0, 10.0)).expect("inside");
        let expected = 2.0 * 20.0 * 2.0_f64.sqrt() / (1.0 + 2.0_f64.sqrt());
        assert!((corner - expected).abs() < 0.75, "{corner} vs {expected}");
        assert!(
            field.at(Point::new(50.0, 50.0)).is_none(),
            "counter is not ink"
        );

        // A 20 unit stem crossed by a 20 unit bar, as one contour: the crossing
        // holds a wider disc.
        let mut cross = BezPath::new();
        cross.move_to((40.0, 0.0));
        for corner in [
            (40.0, 40.0),
            (0.0, 40.0),
            (0.0, 60.0),
            (40.0, 60.0),
            (40.0, 100.0),
            (60.0, 100.0),
            (60.0, 60.0),
            (100.0, 60.0),
            (100.0, 40.0),
            (60.0, 40.0),
            (60.0, 0.0),
        ] {
            cross.line_to(corner);
        }
        cross.close_path();
        let field = thickness_field(&cross, 200).expect("ink");
        let stem = field.at(Point::new(50.0, 10.0)).expect("inside");
        assert!((stem - 20.0).abs() < 0.75, "{stem}");
        let middle = field.at(Point::new(50.0, 50.0)).expect("inside");
        assert!((middle - 20.0 * 2.0_f64.sqrt()).abs() < 1.0, "{middle}");
    }

    #[test]
    fn the_ramp_is_centered_on_the_reference() {
        let field = ThicknessField {
            origin: Point::ZERO,
            cell: 1.0,
            width: 0,
            height: 0,
            thickness: Vec::new(),
            coverage: Vec::new(),
            reference: 100.0,
        };
        assert_eq!(field.ramp(100.0), 0.5);
        assert!((field.ramp(90.0) - 0.25).abs() < 1e-9);
        assert!(field.ramp(80.0).abs() < 1e-9);
        assert_eq!(field.ramp(10.0), 0.0);
        assert!((field.ramp(120.0) - 1.0).abs() < 1e-9);
        assert_eq!(field.ramp(400.0), 1.0);
        let empty = ThicknessField {
            reference: 0.0,
            ..field
        };
        assert_eq!(empty.ramp(7.0), 0.5);
    }
}
