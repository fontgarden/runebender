// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Collisions between the marks and dots of the glyphs of one shaped line.
//!
//! GPOS places a mark on its own base and knows nothing of the neighbors, so a kasra
//! can touch the dot of the next letter, or a fatha the fatha of the previous one, and
//! no table reports it.
//! This module finds those contacts on the placed outlines of a line, in font units.
//!
//! The method:
//!
//! 1. Each outline is flattened to closed polylines with a tolerance of
//!    [`FLATTEN_TOLERANCE`] and split into islands: an outer contour with its counters.
//!    A contour nests in the smallest larger contour that contains its first point.
//!    A contour with the opposite direction to an outer contour that holds it is a
//!    counter; a contour inside a counter is a new island, such as a dot in the bowl of
//!    a letter.
//! 2. Each island gets a role.
//!    Every island of a mark glyph is a [`IslandRole::Mark`].
//!    In another glyph, the island with the largest area is the body, and an island that
//!    is small next to it is a [`IslandRole::Dot`]; see [`CollisionOptions`].
//! 3. Candidate pairs come from a sweep and prune on x over the island bounds, each
//!    widened by half the clearance.
//!    Pairs in one glyph, body against body, and a mark against its own base are left out
//!    by default; [`CollisionOptions`] says what counts.
//! 4. Each candidate pair is tested exactly on the polylines.
//!    Segments are grouped in runs with bounds, and only runs closer than the clearance
//!    are compared, so the cost follows the length of outline near the other island.
//!    A crossing or a touch is an [`CollisionKind::Overlap`]; one island inside the
//!    filled area of the other, by the nonzero winding number, is a
//!    [`CollisionKind::Containment`]; a gap below the clearance is
//!    [`CollisionKind::Near`].
//!
//! The result is in a fixed order, sorted by glyph and island indices.
//! [`CollisionCache`] keeps the flattened islands by a key the caller supplies, such as
//! the glyph name and layer revision, and the result of each pair by the relative offset
//! of its two glyphs, so a drag tests again only the pairs whose geometry changed.
//!
//! The module reads outlines only; it has no font, layout or view dependency.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use kurbo::{BezPath, PathEl, Point, Rect, Vec2};

/// The flattening tolerance in font units, the largest distance from a curve to its
/// polyline.
pub const FLATTEN_TOLERANCE: f64 = 0.25;

/// The default clearance as a fraction of the em.
///
/// Two percent of the em is 20 units at 1,000 units per em.
/// At a text size of 16 pixels per em that is 0.3 pixels, so a gap below it closes in
/// rendering, and at display sizes it is still a visible gap that a designer judges by eye.
pub const DEFAULT_CLEARANCE_EM: f64 = 0.02;

/// The default largest dot area, as a fraction of the em squared.
///
/// 0.04 em squared is a square of 200 by 200 units at 1,000 units per em, larger than any
/// nuqta, so a second large part of a letter is never a dot.
pub const DEFAULT_DOT_MAX_AREA_EM2: f64 = 0.04;

/// The default largest ratio of a dot area to the body area of its glyph.
pub const DEFAULT_DOT_AREA_RATIO: f64 = 0.25;

/// Segments in one bounded run of a polyline.
const RUN: usize = 8;

/// The distance below which two segments touch.
const TOUCH: f64 = 1e-9;

/// One glyph of a shaped line, placed in line coordinates.
#[derive(Clone, Copy, Debug)]
pub struct PlacedGlyph<'a, K> {
    /// The cache key of the outline, such as the glyph name and the layer revision.
    ///
    /// Two glyphs with the same key must have the same outline.
    /// [`find_collisions`] does not read it.
    pub key: K,
    /// The outline in font units, relative to the glyph origin, with components resolved.
    pub outline: &'a BezPath,
    /// The glyph origin in line coordinates: the pen position plus the offset of the shaper.
    pub origin: Point,
    /// The cluster of the glyph: a mark has the cluster of the base it belongs to.
    ///
    /// A shaper can give a mark a cluster of its own; the caller then gives the mark the
    /// cluster of its base.
    pub cluster: usize,
    /// Whether the glyph is a mark.
    pub is_mark: bool,
}

/// What counts as a collision.
#[derive(Clone, Debug, PartialEq)]
pub struct CollisionOptions {
    /// Two islands closer than this, in font units, collide; 0 reports contact only.
    pub clearance: f64,
    /// Test a mark against the islands of a base glyph in the same cluster.
    pub mark_to_own_base: bool,
    /// Test a dot against the body of its own glyph.
    pub dot_to_own_body: bool,
    /// Test a body against the body of another glyph.
    ///
    /// Joined letters overlap on purpose, so this is off by default.
    pub body_to_body: bool,
    /// The largest area of a dot, in square font units.
    pub dot_max_area: f64,
    /// The largest ratio of the area of a dot to the area of the body of its glyph.
    pub dot_area_ratio: f64,
}

impl CollisionOptions {
    /// The default options for a font with `units_per_em` units per em.
    pub fn for_units_per_em(units_per_em: f64) -> Self {
        Self {
            clearance: DEFAULT_CLEARANCE_EM * units_per_em,
            mark_to_own_base: false,
            dot_to_own_body: false,
            body_to_body: false,
            dot_max_area: DEFAULT_DOT_MAX_AREA_EM2 * units_per_em * units_per_em,
            dot_area_ratio: DEFAULT_DOT_AREA_RATIO,
        }
    }
}

impl Default for CollisionOptions {
    /// The default options at 1,000 units per em.
    fn default() -> Self {
        Self::for_units_per_em(1000.0)
    }
}

/// The part an island plays in its glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IslandRole {
    /// The main part of a letter.
    Body,
    /// A small island of a letter that is not a mark glyph, such as a nuqta.
    Dot,
    /// An island of a mark glyph.
    Mark,
}

/// How two islands meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CollisionKind {
    /// The outlines cross or touch.
    Overlap,
    /// One island lies wholly inside the filled area of the other.
    Containment,
    /// The outlines are apart, but closer than the clearance.
    Near,
}

/// One collision between two islands.
#[derive(Clone, Debug, PartialEq)]
pub struct Collision {
    /// The index of the first glyph in the input; never larger than `second`.
    pub first: usize,
    /// The island of the first glyph, in the order of its outline.
    pub first_island: usize,
    /// The role of that island.
    pub first_role: IslandRole,
    /// The index of the second glyph in the input.
    pub second: usize,
    /// The island of the second glyph.
    pub second_island: usize,
    /// The role of that island.
    pub second_role: IslandRole,
    /// How the islands meet.
    pub kind: CollisionKind,
    /// The smallest distance between the outlines; 0 for an overlap or a containment.
    pub distance: f64,
    /// In line coordinates: for [`CollisionKind::Near`], the nearest point of the first
    /// island and of the second; otherwise both are the center of `region`.
    pub points: [Point; 2],
    /// In line coordinates: the bounds of the shared area for an overlap, of the inner
    /// island for a containment, and of the two nearest points for a near pair.
    pub region: Rect,
}

/// The counts of the last run of a [`CollisionCache`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CollisionRunStats {
    /// Outlines flattened in the run; the others came from the cache.
    pub outlines_flattened: usize,
    /// Island pairs tested exactly in the run.
    pub pairs_tested: usize,
    /// Island pairs whose result came from the cache.
    pub pairs_cached: usize,
}

/// One closed polyline, with the bounds of each run of [`RUN`] segments.
#[derive(Clone, Debug)]
struct Ring {
    points: Vec<Point>,
    bounds: Rect,
    runs: Vec<Rect>,
    signed_area: f64,
}

impl Ring {
    fn new(points: Vec<Point>) -> Self {
        let count = points.len();
        let mut signed_area = 0.0;
        for index in 0..count {
            let a = points[index];
            let b = points[(index + 1) % count];
            signed_area += a.x * b.y - b.x * a.y;
        }
        let runs = (0..count)
            .step_by(RUN)
            .map(|start| {
                let end = (start + RUN).min(count);
                let mut bounds = Rect::from_points(points[start], points[start]);
                for index in start..end {
                    bounds = bounds.union_pt(points[(index + 1) % count]);
                }
                bounds
            })
            .collect::<Vec<_>>();
        let bounds = runs
            .iter()
            .copied()
            .reduce(|a, b| a.union(b))
            .unwrap_or_default();
        Self {
            points,
            bounds,
            runs,
            signed_area: signed_area * 0.5,
        }
    }

    fn segment(&self, index: usize) -> (Point, Point) {
        (
            self.points[index],
            self.points[(index + 1) % self.points.len()],
        )
    }

    /// The segments of run `run`.
    fn run_segments(&self, run: usize) -> std::ops::Range<usize> {
        let start = run * RUN;
        start..(start + RUN).min(self.points.len())
    }

    /// The winding number of the ring around `point`.
    fn winding(&self, point: Point) -> i32 {
        let mut winding = 0;
        for (run, bounds) in self.runs.iter().enumerate() {
            if bounds.y0 > point.y || bounds.y1 < point.y || bounds.x1 < point.x {
                continue;
            }
            for index in self.run_segments(run) {
                let (a, b) = self.segment(index);
                let left = (b - a).cross(point - a);
                if a.y <= point.y {
                    if b.y > point.y && left > 0.0 {
                        winding += 1;
                    }
                } else if b.y <= point.y && left < 0.0 {
                    winding -= 1;
                }
            }
        }
        winding
    }
}

/// An outer contour with its counters, and the outer contours that overlap it.
#[derive(Clone, Debug)]
struct Island {
    rings: Vec<Ring>,
    bounds: Rect,
    area: f64,
}

impl Island {
    /// Whether `point` is in the filled area, by the nonzero rule.
    fn contains(&self, point: Point) -> bool {
        contains_closed(self.bounds, point)
            && self
                .rings
                .iter()
                .map(|ring| ring.winding(point))
                .sum::<i32>()
                != 0
    }
}

/// Whether `rect` contains `point`, edges included.
fn contains_closed(rect: Rect, point: Point) -> bool {
    point.x >= rect.x0 && point.x <= rect.x1 && point.y >= rect.y0 && point.y <= rect.y1
}

/// The closed polylines of an outline; open subpaths draw no ink and are left out.
fn rings_of(outline: &BezPath) -> Vec<Ring> {
    let mut rings = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    fn finish(current: &mut Vec<Point>, rings: &mut Vec<Ring>) {
        if current.len() > 1 && current.first() == current.last() {
            current.pop();
        }
        if current.len() >= 3 {
            let ring = Ring::new(std::mem::take(current));
            if ring.signed_area.abs() > TOUCH {
                rings.push(ring);
            }
        }
        current.clear();
    }
    kurbo::flatten(
        outline.elements().iter().copied(),
        FLATTEN_TOLERANCE,
        |element| match element {
            PathEl::MoveTo(point) => {
                current.clear();
                current.push(point);
            }
            PathEl::LineTo(point) => {
                if current.last() != Some(&point) {
                    current.push(point);
                }
            }
            PathEl::ClosePath => finish(&mut current, &mut rings),
            PathEl::QuadTo(..) | PathEl::CurveTo(..) => {}
        },
    );
    rings
}

/// Split an outline into islands, in a fixed order.
fn islands_of(outline: &BezPath) -> Vec<Island> {
    let rings = rings_of(outline);
    let mut order: Vec<usize> = (0..rings.len()).collect();
    order.sort_by(|&a, &b| {
        rings[b]
            .signed_area
            .abs()
            .total_cmp(&rings[a].signed_area.abs())
            .then(a.cmp(&b))
    });
    // For each placed ring: its island and whether it is a counter.
    let mut placed: Vec<Option<(usize, bool)>> = vec![None; rings.len()];
    let mut members: Vec<Vec<(usize, bool)>> = Vec::new();
    for (position, &index) in order.iter().enumerate() {
        let start = rings[index].points[0];
        let container = order[..position].iter().rev().copied().find(|&other| {
            contains_closed(rings[other].bounds, start) && rings[other].winding(start) != 0
        });
        let same_direction =
            |other: usize| (rings[other].signed_area > 0.0) == (rings[index].signed_area > 0.0);
        let (island, counter) = match container.and_then(|other| placed[other].map(|p| (other, p)))
        {
            None => (None, false),
            Some((other, (island, false))) => (Some(island), !same_direction(other)),
            Some((other, (island, true))) => {
                if same_direction(other) {
                    (Some(island), true)
                } else {
                    (None, false)
                }
            }
        };
        let island = island.unwrap_or_else(|| {
            members.push(Vec::new());
            members.len() - 1
        });
        placed[index] = Some((island, counter));
        members[island].push((index, counter));
    }
    // Islands in the order of their first ring in the outline.
    for list in &mut members {
        list.sort_unstable();
    }
    members.sort_by_key(|list| list[0].0);
    members
        .into_iter()
        .map(|list| {
            let mut area = 0.0;
            let mut bounds: Option<Rect> = None;
            let mut island_rings = Vec::with_capacity(list.len());
            for (index, counter) in list {
                let ring = rings[index].clone();
                let size = ring.signed_area.abs();
                area += if counter { -size } else { size };
                if !counter {
                    bounds = Some(bounds.map_or(ring.bounds, |b| b.union(ring.bounds)));
                }
                island_rings.push(ring);
            }
            Island {
                rings: island_rings,
                bounds: bounds.unwrap_or_default(),
                area: f64::max(area, 0.0),
            }
        })
        .collect()
}

/// The roles of the islands of one glyph.
fn roles(islands: &[Island], is_mark: bool, options: &CollisionOptions) -> Vec<IslandRole> {
    if is_mark {
        return vec![IslandRole::Mark; islands.len()];
    }
    let largest = islands
        .iter()
        .enumerate()
        .max_by(|(a, x), (b, y)| x.area.total_cmp(&y.area).then(b.cmp(a)))
        .map(|(index, _)| index);
    let body_area = largest.map_or(0.0, |index| islands[index].area);
    islands
        .iter()
        .enumerate()
        .map(|(index, island)| {
            let small = island.area <= options.dot_max_area
                && island.area <= options.dot_area_ratio * body_area;
            if Some(index) != largest && small {
                IslandRole::Dot
            } else {
                IslandRole::Body
            }
        })
        .collect()
}

/// The distance between two boxes; 0 when they touch or overlap.
fn box_distance(a: Rect, b: Rect) -> f64 {
    let dx = (b.x0 - a.x1).max(a.x0 - b.x1).max(0.0);
    let dy = (b.y0 - a.y1).max(a.y0 - b.y1).max(0.0);
    dx.hypot(dy)
}

/// The nearest point of segment `a`–`b` to `point`.
fn nearest_on_segment(point: Point, a: Point, b: Point) -> Point {
    let direction = b - a;
    let length = direction.hypot2();
    if length == 0.0 {
        return a;
    }
    let t = ((point - a).dot(direction) / length).clamp(0.0, 1.0);
    a + direction * t
}

/// The point where two segments cross, when they cross properly.
fn crossing(p1: Point, p2: Point, q1: Point, q2: Point) -> Option<Point> {
    let d1 = (q2 - q1).cross(p1 - q1);
    let d2 = (q2 - q1).cross(p2 - q1);
    let d3 = (p2 - p1).cross(q1 - p1);
    let d4 = (p2 - p1).cross(q2 - p1);
    let opposite = |a: f64, b: f64| (a > 0.0 && b < 0.0) || (a < 0.0 && b > 0.0);
    (opposite(d1, d2) && opposite(d3, d4)).then(|| p1 + (p2 - p1) * (d1 / (d1 - d2)))
}

/// The distance between two segments that do not cross, with the nearest points.
fn segment_distance(p1: Point, p2: Point, q1: Point, q2: Point) -> (f64, Point, Point) {
    let candidates = [
        (p1, nearest_on_segment(p1, q1, q2)),
        (p2, nearest_on_segment(p2, q1, q2)),
        (nearest_on_segment(q1, p1, p2), q1),
        (nearest_on_segment(q2, p1, p2), q2),
    ];
    let mut best = (f64::INFINITY, p1, q1);
    for (a, b) in candidates {
        let distance = (b - a).hypot();
        if distance < best.0 {
            best = (distance, a, b);
        }
    }
    best
}

/// The result of one island pair, relative to the origin of the first glyph.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PairHit {
    kind: CollisionKind,
    distance: f64,
    points: [Point; 2],
    region: Rect,
}

impl PairHit {
    fn translated(self, offset: Vec2) -> Self {
        Self {
            points: [self.points[0] + offset, self.points[1] + offset],
            region: self.region + offset,
            ..self
        }
    }
}

/// Test island `a` against island `b` moved by `delta`, in the coordinates of `a`.
fn test_pair(a: &Island, b: &Island, delta: Vec2, clearance: f64) -> Option<PairHit> {
    let b_bounds = b.bounds + delta;
    if box_distance(a.bounds, b_bounds) > clearance {
        return None;
    }
    let mut crossings: Vec<Point> = Vec::new();
    let mut best = (f64::INFINITY, Point::ZERO, Point::ZERO);
    for ring_a in &a.rings {
        for ring_b in &b.rings {
            if box_distance(ring_a.bounds, ring_b.bounds + delta) > clearance {
                continue;
            }
            for (run_a, bounds_a) in ring_a.runs.iter().enumerate() {
                for (run_b, bounds_b) in ring_b.runs.iter().enumerate() {
                    let reach = clearance.min(best.0);
                    if box_distance(*bounds_a, *bounds_b + delta) > reach {
                        continue;
                    }
                    for index_a in ring_a.run_segments(run_a) {
                        let (p1, p2) = ring_a.segment(index_a);
                        let segment_bounds = Rect::from_points(p1, p2);
                        for index_b in ring_b.run_segments(run_b) {
                            let (q1, q2) = ring_b.segment(index_b);
                            let (q1, q2) = (q1 + delta, q2 + delta);
                            if box_distance(segment_bounds, Rect::from_points(q1, q2))
                                > clearance.min(best.0)
                            {
                                continue;
                            }
                            if let Some(point) = crossing(p1, p2, q1, q2) {
                                crossings.push(point);
                                best = (0.0, point, point);
                                continue;
                            }
                            let (distance, near_a, near_b) = segment_distance(p1, p2, q1, q2);
                            if distance <= TOUCH {
                                crossings.push(near_a);
                            }
                            if distance < best.0 {
                                best = (distance, near_a, near_b);
                            }
                        }
                    }
                }
            }
        }
    }
    if !crossings.is_empty() {
        let region = overlap_region(a, b, delta, &crossings);
        let center = region.center();
        return Some(PairHit {
            kind: CollisionKind::Overlap,
            distance: 0.0,
            points: [center, center],
            region,
        });
    }
    let first_a = a.rings[0].points[0];
    let first_b = b.rings[0].points[0] + delta;
    let inner = if b.contains(first_a - delta) {
        Some(a.bounds)
    } else if a.contains(first_b) {
        Some(b_bounds)
    } else {
        None
    };
    if let Some(region) = inner {
        let center = region.center();
        return Some(PairHit {
            kind: CollisionKind::Containment,
            distance: 0.0,
            points: [center, center],
            region,
        });
    }
    (best.0 < clearance).then(|| PairHit {
        kind: CollisionKind::Near,
        distance: best.0,
        points: [best.1, best.2],
        region: Rect::from_points(best.1, best.2),
    })
}

/// The bounds of the shared area of two islands: the crossings, and the vertices of each
/// island inside the other.
fn overlap_region(a: &Island, b: &Island, delta: Vec2, crossings: &[Point]) -> Rect {
    let mut region = Rect::from_points(crossings[0], crossings[0]);
    for point in crossings {
        region = region.union_pt(*point);
    }
    let shared = a.bounds.intersect(b.bounds + delta);
    for ring in &a.rings {
        for point in &ring.points {
            if contains_closed(shared, *point) && b.contains(*point - delta) {
                region = region.union_pt(*point);
            }
        }
    }
    for ring in &b.rings {
        for point in &ring.points {
            let moved = *point + delta;
            if contains_closed(shared, moved) && a.contains(moved) {
                region = region.union_pt(moved);
            }
        }
    }
    region
}

/// One island in line coordinates, for the sweep.
#[derive(Clone, Copy, Debug)]
struct Entry {
    glyph: usize,
    island: usize,
    bounds: Rect,
}

/// Whether a pair of islands is tested under `options`.
fn pair_counts(
    a: (usize, usize, IslandRole),
    b: (usize, usize, IslandRole),
    options: &CollisionOptions,
) -> bool {
    let (glyph_a, cluster_a, role_a) = a;
    let (glyph_b, cluster_b, role_b) = b;
    if glyph_a == glyph_b {
        return options.dot_to_own_body
            && matches!(
                (role_a, role_b),
                (IslandRole::Dot, IslandRole::Body) | (IslandRole::Body, IslandRole::Dot)
            );
    }
    let marks = (role_a == IslandRole::Mark, role_b == IslandRole::Mark);
    match (role_a, role_b) {
        (IslandRole::Body, IslandRole::Body) => options.body_to_body,
        _ if cluster_a == cluster_b && marks.0 != marks.1 => options.mark_to_own_base,
        _ => true,
    }
}

/// The shared detection: `islands[i]` belongs to `items[i]`, and `test` tests one pair.
fn detect<K>(
    items: &[PlacedGlyph<'_, K>],
    islands: &[Arc<[Island]>],
    options: &CollisionOptions,
    mut test: impl FnMut(usize, usize, usize, usize, Vec2) -> Option<PairHit>,
) -> Vec<Collision> {
    let clearance = options.clearance.max(0.0);
    let glyph_roles: Vec<Vec<IslandRole>> = items
        .iter()
        .zip(islands)
        .map(|(item, islands)| roles(islands, item.is_mark, options))
        .collect();
    let mut entries: Vec<Entry> = items
        .iter()
        .zip(islands)
        .enumerate()
        .flat_map(|(glyph, (item, islands))| {
            islands.iter().enumerate().map(move |(island, data)| Entry {
                glyph,
                island,
                bounds: (data.bounds + item.origin.to_vec2())
                    .inflate(clearance * 0.5, clearance * 0.5),
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        a.bounds
            .x0
            .total_cmp(&b.bounds.x0)
            .then(a.glyph.cmp(&b.glyph))
            .then(a.island.cmp(&b.island))
    });
    let mut collisions = Vec::new();
    let mut active: Vec<Entry> = Vec::new();
    for entry in entries {
        active.retain(|other| other.bounds.x1 >= entry.bounds.x0);
        for other in &active {
            if other.bounds.y1 < entry.bounds.y0 || other.bounds.y0 > entry.bounds.y1 {
                continue;
            }
            let (first, second) = if (other.glyph, other.island) < (entry.glyph, entry.island) {
                (*other, entry)
            } else {
                (entry, *other)
            };
            let describe = |e: &Entry| {
                (
                    e.glyph,
                    items[e.glyph].cluster,
                    glyph_roles[e.glyph][e.island],
                )
            };
            if !pair_counts(describe(&first), describe(&second), options) {
                continue;
            }
            let origin = items[first.glyph].origin.to_vec2();
            let delta = items[second.glyph].origin.to_vec2() - origin;
            let Some(hit) = test(
                first.glyph,
                first.island,
                second.glyph,
                second.island,
                delta,
            ) else {
                continue;
            };
            let hit = hit.translated(origin);
            collisions.push(Collision {
                first: first.glyph,
                first_island: first.island,
                first_role: glyph_roles[first.glyph][first.island],
                second: second.glyph,
                second_island: second.island,
                second_role: glyph_roles[second.glyph][second.island],
                kind: hit.kind,
                distance: hit.distance,
                points: hit.points,
                region: hit.region,
            });
        }
        active.push(entry);
    }
    collisions.sort_by_key(|c| (c.first, c.first_island, c.second, c.second_island));
    collisions
}

/// Find the collisions of one placed line, with nothing cached.
pub fn find_collisions<K>(
    items: &[PlacedGlyph<'_, K>],
    options: &CollisionOptions,
) -> Vec<Collision> {
    let islands: Vec<Arc<[Island]>> = items
        .iter()
        .map(|item| Arc::from(islands_of(item.outline)))
        .collect();
    let clearance = options.clearance.max(0.0);
    detect(items, &islands, options, |a, ia, b, ib, delta| {
        test_pair(&islands[a][ia], &islands[b][ib], delta, clearance)
    })
}

/// The key of one tested island pair: both outline keys and islands, the offset of the
/// second glyph from the first, and the clearance, as bits.
type PairKey<K> = (K, usize, K, usize, u64, u64, u64);

/// Flattened islands and pair results kept between runs, keyed by outline key.
///
/// After each run the cache keeps only what that run used, so it holds about one line.
#[derive(Debug)]
pub struct CollisionCache<K> {
    islands: HashMap<K, (Arc<[Island]>, u64)>,
    pairs: HashMap<PairKey<K>, (Option<PairHit>, u64)>,
    generation: u64,
    stats: CollisionRunStats,
}

impl<K> Default for CollisionCache<K> {
    fn default() -> Self {
        Self {
            islands: HashMap::new(),
            pairs: HashMap::new(),
            generation: 0,
            stats: CollisionRunStats::default(),
        }
    }
}

impl<K: Clone + Eq + Hash> CollisionCache<K> {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// The counts of the last run.
    pub fn stats(&self) -> CollisionRunStats {
        self.stats
    }

    /// Find the collisions of one placed line, as [`find_collisions`] does, with the
    /// islands and pair results of earlier runs.
    pub fn find_collisions(
        &mut self,
        items: &[PlacedGlyph<'_, K>],
        options: &CollisionOptions,
    ) -> Vec<Collision> {
        self.generation += 1;
        let generation = self.generation;
        let mut stats = CollisionRunStats::default();
        let islands: Vec<Arc<[Island]>> = items
            .iter()
            .map(|item| {
                let entry = self.islands.entry(item.key.clone()).or_insert_with(|| {
                    stats.outlines_flattened += 1;
                    (Arc::from(islands_of(item.outline)), generation)
                });
                entry.1 = generation;
                entry.0.clone()
            })
            .collect();
        let clearance = options.clearance.max(0.0);
        let pairs = &mut self.pairs;
        let collisions = detect(items, &islands, options, |a, ia, b, ib, delta| {
            let key = (
                items[a].key.clone(),
                ia,
                items[b].key.clone(),
                ib,
                delta.x.to_bits(),
                delta.y.to_bits(),
                clearance.to_bits(),
            );
            if let Some(cached) = pairs.get_mut(&key) {
                cached.1 = generation;
                stats.pairs_cached += 1;
                return cached.0;
            }
            stats.pairs_tested += 1;
            let hit = test_pair(&islands[a][ia], &islands[b][ib], delta, clearance);
            pairs.insert(key, (hit, generation));
            hit
        });
        self.islands.retain(|_, (_, used)| *used == generation);
        self.pairs.retain(|_, (_, used)| *used == generation);
        self.stats = stats;
        collisions
    }
}

#[cfg(test)]
mod tests {
    use kurbo::{Circle, Shape as _};

    use super::*;
    use crate::font::project::Project;
    use crate::font::variable::SourceId;
    use crate::text::layout_tables::{LayoutCategory, LayoutInputs, build_shaping_font};

    /// A closed rectangle, counterclockwise.
    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
        let mut path = BezPath::new();
        path.move_to((x0, y0));
        path.line_to((x1, y0));
        path.line_to((x1, y1));
        path.line_to((x0, y1));
        path.close_path();
        path
    }

    /// A closed rectangle, clockwise, for a counter.
    fn rect_reversed(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
        let mut path = BezPath::new();
        path.move_to((x0, y0));
        path.line_to((x0, y1));
        path.line_to((x1, y1));
        path.line_to((x1, y0));
        path.close_path();
        path
    }

    fn circle(x: f64, y: f64, radius: f64) -> BezPath {
        Circle::new((x, y), radius).to_path(0.01)
    }

    fn joined(parts: &[BezPath]) -> BezPath {
        let mut path = BezPath::new();
        for part in parts {
            path.extend(part.elements().iter().copied());
        }
        path
    }

    fn glyph(
        outline: &BezPath,
        x: f64,
        y: f64,
        cluster: usize,
        is_mark: bool,
    ) -> PlacedGlyph<'_, usize> {
        PlacedGlyph {
            key: 0,
            outline,
            origin: Point::new(x, y),
            cluster,
            is_mark,
        }
    }

    fn options(clearance: f64) -> CollisionOptions {
        CollisionOptions {
            clearance,
            ..CollisionOptions::default()
        }
    }

    #[test]
    fn the_default_clearance_is_two_percent_of_the_em() {
        assert_eq!(
            CollisionOptions::default().clearance,
            20.0,
            "1,000 units per em"
        );
        assert_eq!(
            CollisionOptions::for_units_per_em(2048.0).clearance,
            40.96,
            "2,048 units per em"
        );
    }

    #[test]
    fn an_outline_splits_into_islands_with_counters_and_roles() {
        // A body with a counter, a dot in the counter, and a dot below.
        let outline = joined(&[
            rect(0.0, 0.0, 400.0, 400.0),
            rect_reversed(100.0, 100.0, 300.0, 300.0),
            rect(180.0, 180.0, 220.0, 220.0),
            rect(180.0, -100.0, 240.0, -40.0),
        ]);
        let islands = islands_of(&outline);
        assert_eq!(islands.len(), 3, "body, inner dot, lower dot");
        assert_eq!(islands[0].rings.len(), 2, "the body keeps its counter");
        assert_eq!(islands[0].area, 120_000.0, "the counter is subtracted");
        assert!(
            !islands[0].contains(Point::new(200.0, 250.0)),
            "the counter is not ink"
        );
        assert!(
            islands[0].contains(Point::new(50.0, 50.0)),
            "the body is ink"
        );
        let options = CollisionOptions::default();
        assert_eq!(
            roles(&islands, false, &options),
            [IslandRole::Body, IslandRole::Dot, IslandRole::Dot],
            "roles of a letter"
        );
        assert_eq!(
            roles(&islands, true, &options),
            [IslandRole::Mark; 3],
            "every island of a mark is a mark"
        );
    }

    #[test]
    fn two_squares_that_cross_overlap_in_their_shared_area() {
        let a = rect(0.0, 0.0, 100.0, 100.0);
        let items = [glyph(&a, 0.0, 0.0, 0, true), glyph(&a, 60.0, 70.0, 1, true)];
        let found = find_collisions(&items, &options(20.0));
        assert_eq!(found.len(), 1, "{found:?}");
        let hit = &found[0];
        assert_eq!((hit.first, hit.second), (0, 1), "glyph order");
        assert_eq!(hit.kind, CollisionKind::Overlap, "kind");
        assert_eq!(hit.distance, 0.0, "distance");
        assert_eq!(
            hit.region,
            Rect::new(60.0, 70.0, 100.0, 100.0),
            "shared area"
        );
        assert_eq!(
            hit.points,
            [Point::new(80.0, 85.0); 2],
            "marker at the center"
        );
    }

    #[test]
    fn an_island_inside_another_is_a_containment() {
        let body = rect(0.0, 0.0, 400.0, 400.0);
        let mark = circle(0.0, 0.0, 30.0);
        let items = [
            glyph(&body, 0.0, 0.0, 0, false),
            glyph(&mark, 200.0, 200.0, 1, true),
        ];
        let found = find_collisions(&items, &options(20.0));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, CollisionKind::Containment, "kind");
        assert_eq!(found[0].distance, 0.0, "distance");
        let region = found[0].region;
        assert!(
            (region.x0 - 170.0).abs() < 0.5 && (region.y1 - 230.0).abs() < 0.5,
            "{region:?}"
        );
        assert_eq!(
            (found[0].first_role, found[0].second_role),
            (IslandRole::Body, IslandRole::Mark),
            "roles"
        );
    }

    #[test]
    fn a_mark_in_a_counter_is_measured_against_the_counter() {
        let body = joined(&[
            rect(0.0, 0.0, 400.0, 400.0),
            rect_reversed(100.0, 100.0, 300.0, 300.0),
        ]);
        let mark = rect(-20.0, -20.0, 20.0, 20.0);
        let far = [
            glyph(&body, 0.0, 0.0, 0, false),
            glyph(&mark, 200.0, 200.0, 1, true),
        ];
        assert!(
            find_collisions(&far, &options(20.0)).is_empty(),
            "60 units from the counter"
        );
        let close = [
            glyph(&body, 0.0, 0.0, 0, false),
            glyph(&mark, 130.0, 200.0, 1, true),
        ];
        let found = find_collisions(&close, &options(20.0));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, CollisionKind::Near, "kind");
        assert_eq!(found[0].distance, 10.0, "distance");
        assert_eq!(found[0].points[0].x, 100.0, "nearest point on the counter");
        assert_eq!(found[0].points[1].x, 110.0, "nearest point on the mark");
    }

    #[test]
    fn two_circles_closer_than_the_clearance_are_near() {
        let a = circle(0.0, 0.0, 50.0);
        // A gap of 10 units between the circles.
        let items = [glyph(&a, 0.0, 0.0, 0, true), glyph(&a, 110.0, 0.0, 1, true)];
        let found = find_collisions(&items, &options(20.0));
        assert_eq!(found.len(), 1, "{found:?}");
        let hit = &found[0];
        assert_eq!(hit.kind, CollisionKind::Near, "kind");
        assert!(
            (hit.distance - 10.0).abs() <= 2.0 * FLATTEN_TOLERANCE,
            "{}",
            hit.distance
        );
        assert!(
            (hit.points[0].x - 50.0).abs() < 0.5 && hit.points[0].y.abs() < 5.0,
            "{hit:?}"
        );
        assert!(
            (hit.points[1].x - 60.0).abs() < 0.5 && hit.points[1].y.abs() < 5.0,
            "{hit:?}"
        );
        let crossing = [glyph(&a, 0.0, 0.0, 0, true), glyph(&a, 90.0, 0.0, 1, true)];
        let found = find_collisions(&crossing, &options(20.0));
        assert_eq!(found[0].kind, CollisionKind::Overlap, "{found:?}");
        let region = found[0].region;
        assert!(
            (region.x0 - 40.0).abs() < 0.5 && (region.x1 - 50.0).abs() < 0.5,
            "the lens between the circles: {region:?}"
        );
    }

    #[test]
    fn the_clearance_is_an_open_bound() {
        let a = rect(0.0, 0.0, 100.0, 100.0);
        let at = |x: f64, clearance: f64| {
            let items = [glyph(&a, 0.0, 0.0, 0, true), glyph(&a, x, 0.0, 1, true)];
            find_collisions(&items, &options(clearance))
        };
        assert!(
            at(120.0, 20.0).is_empty(),
            "a gap equal to the clearance is clear"
        );
        assert!(at(120.5, 20.0).is_empty(), "a wider gap is clear");
        let inside = at(119.5, 20.0);
        assert_eq!(inside.len(), 1, "a gap just below the clearance");
        assert_eq!(inside[0].kind, CollisionKind::Near, "kind");
        assert_eq!(inside[0].distance, 19.5, "distance");
        let touch = at(100.0, 20.0);
        assert_eq!(touch[0].kind, CollisionKind::Overlap, "shared edges touch");
        assert_eq!(
            touch[0].region,
            Rect::new(100.0, 0.0, 100.0, 100.0),
            "the shared edge"
        );
        assert!(
            at(101.0, 0.0).is_empty(),
            "clearance 0 reports contact only"
        );
        assert_eq!(at(100.0, 0.0).len(), 1, "a touch at clearance 0");
    }

    /// Two letters, each with a body and a dot below and a mark above; the line is right
    /// to left, so the second letter is on the left.
    struct Line {
        letter: BezPath,
        mark: BezPath,
    }

    impl Line {
        fn new() -> Self {
            Self {
                letter: joined(&[
                    rect(0.0, 0.0, 300.0, 100.0),
                    rect(120.0, -120.0, 180.0, -60.0),
                ]),
                mark: rect(-60.0, 0.0, 60.0, 40.0),
            }
        }

        /// The letters at x 300 (cluster 0) and 0 (cluster 1), each with a mark.
        fn items(&self, mark_0: Point, mark_1: Point) -> Vec<PlacedGlyph<'_, usize>> {
            vec![
                glyph(&self.letter, 300.0, 0.0, 0, false),
                glyph(&self.mark, mark_0.x, mark_0.y, 0, true),
                glyph(&self.letter, 0.0, 0.0, 1, false),
                glyph(&self.mark, mark_1.x, mark_1.y, 1, true),
            ]
        }
    }

    #[test]
    fn a_mark_over_the_mark_of_a_neighbor_is_reported() {
        let line = Line::new();
        // Each mark is 40 units above its letter; the marks share x 260 to 280.
        let items = line.items(Point::new(320.0, 140.0), Point::new(220.0, 140.0));
        let found = find_collisions(&items, &CollisionOptions::default());
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].first, found[0].second), (1, 3), "the two marks");
        assert_eq!(found[0].kind, CollisionKind::Overlap, "kind");
        assert_eq!(
            found[0].region,
            Rect::new(260.0, 140.0, 280.0, 180.0),
            "shared area"
        );
    }

    #[test]
    fn a_mark_on_its_own_base_counts_only_when_asked() {
        let line = Line::new();
        // The first mark sits low, into its own letter.
        let items = line.items(Point::new(450.0, 80.0), Point::new(150.0, 140.0));
        assert!(
            find_collisions(&items, &CollisionOptions::default()).is_empty(),
            "off by default"
        );
        let asked = CollisionOptions {
            mark_to_own_base: true,
            ..CollisionOptions::default()
        };
        let found = find_collisions(&items, &asked);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            (found[0].first, found[0].second, found[0].kind),
            (0, 1, CollisionKind::Overlap),
            "the first letter and its mark"
        );
    }

    #[test]
    fn a_dot_touching_the_mark_of_the_next_letter_is_reported() {
        let line = Line::new();
        // The mark of the second letter is low, under the dot of the first letter, which
        // is at x 420 to 480 and y -120 to -60.
        let items = line.items(Point::new(450.0, 140.0), Point::new(450.0, -100.0));
        let found = find_collisions(&items, &CollisionOptions::default());
        assert_eq!(found.len(), 1, "{found:?}");
        let hit = &found[0];
        assert_eq!(
            (hit.first, hit.first_island, hit.second),
            (0, 1, 3),
            "dot and mark"
        );
        assert_eq!(
            (hit.first_role, hit.second_role),
            (IslandRole::Dot, IslandRole::Mark),
            "roles"
        );
        assert_eq!(hit.kind, CollisionKind::Overlap, "kind");
    }

    #[test]
    fn joined_bases_are_not_reported() {
        let line = Line::new();
        // Two letters that overlap by 20 units, as joined letters do.
        let items = [
            glyph(&line.letter, 280.0, 0.0, 0, false),
            glyph(&line.letter, 0.0, 0.0, 1, false),
        ];
        assert!(
            find_collisions(&items, &CollisionOptions::default()).is_empty(),
            "joined letters"
        );
        let asked = CollisionOptions {
            body_to_body: true,
            ..CollisionOptions::default()
        };
        let found = find_collisions(&items, &asked);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            (found[0].first_role, found[0].second_role),
            (IslandRole::Body, IslandRole::Body),
            "bodies"
        );
    }

    #[test]
    fn a_dot_against_its_own_body_counts_only_when_asked() {
        let outline = joined(&[
            rect(0.0, 0.0, 300.0, 100.0),
            rect(120.0, -30.0, 180.0, -10.0),
        ]);
        let items = [glyph(&outline, 0.0, 0.0, 0, false)];
        assert!(
            find_collisions(&items, &CollisionOptions::default()).is_empty(),
            "off by default"
        );
        let asked = CollisionOptions {
            dot_to_own_body: true,
            ..CollisionOptions::default()
        };
        let found = find_collisions(&items, &asked);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].first, found[0].second), (0, 0), "one glyph");
        assert_eq!(found[0].kind, CollisionKind::Near, "kind");
        assert_eq!(found[0].distance, 10.0, "distance");
    }

    /// One glyph of a synthetic line: outline key, origin, cluster, and whether it is a mark.
    type Placement = (usize, Point, usize, bool);

    /// A right-to-left line of `letters` letters, each a curved body, a dot above, and a
    /// dot below, with a mark above and most with a mark below.
    /// The mark below the second of every four letters sits under the lower dot of the
    /// previous letter.
    fn long_line(letters: usize) -> (BezPath, BezPath, Vec<Placement>) {
        let body = joined(&[
            kurbo::RoundedRect::new(0.0, 0.0, 320.0, 120.0, 40.0).to_path(0.01),
            circle(160.0, 260.0, 45.0),
            circle(160.0, -110.0, 35.0),
        ]);
        let mut mark = kurbo::Ellipse::new((0.0, 0.0), (80.0, 25.0), 0.3).to_path(0.01);
        // A kurbo ellipse is an open path; a font contour is closed.
        mark.close_path();
        let mut placements = Vec::new();
        for index in 0..letters {
            // Each letter is 300 units wide, so neighbors overlap by 20 units, as joined
            // letters do.
            let x = f64::from(u32::try_from(letters - 1 - index).unwrap()) * 300.0;
            placements.push((0, Point::new(x, 0.0), index, false));
            placements.push((1, Point::new(x + 220.0, 430.0), index, true));
            match index % 4 {
                // The previous letter of a displaced mark has no mark below.
                0 => {}
                1 => placements.push((1, Point::new(x + 460.0, -150.0), index, true)),
                _ => placements.push((1, Point::new(x + 160.0, -200.0), index, true)),
            }
        }
        (body, mark, placements)
    }

    fn long_items<'a>(
        body: &'a BezPath,
        mark: &'a BezPath,
        placements: &[Placement],
    ) -> Vec<PlacedGlyph<'a, usize>> {
        placements
            .iter()
            .map(|&(key, origin, cluster, is_mark)| PlacedGlyph {
                key,
                outline: if key == 0 { body } else { mark },
                origin,
                cluster,
                is_mark,
            })
            .collect()
    }

    #[test]
    fn the_output_is_deterministic_and_the_cache_agrees() {
        let (body, mark, placements) = long_line(20);
        let items = long_items(&body, &mark, &placements);
        let options = CollisionOptions::default();
        let first = find_collisions(&items, &options);
        assert_eq!(
            first.len(),
            5,
            "one dot and mark pair per fourth letter: {first:?}"
        );
        assert!(
            first
                .iter()
                .all(|c| (c.first_role, c.second_role) == (IslandRole::Dot, IslandRole::Mark)),
            "{first:?}"
        );
        assert_eq!(
            first,
            find_collisions(&items, &options),
            "same input, same output"
        );
        let mut sorted = first.clone();
        sorted.sort_by_key(|c| (c.first, c.first_island, c.second, c.second_island));
        assert_eq!(first, sorted, "sorted by glyph and island");

        let mut cache = CollisionCache::new();
        assert_eq!(cache.find_collisions(&items, &options), first, "cold cache");
        assert_eq!(
            cache.stats().outlines_flattened,
            2,
            "one flattening per key"
        );
        // Equal outline keys at equal offsets share one pair test, even in one run.
        let cold = cache.stats();
        assert!(cold.pairs_tested > 0, "pairs were tested");
        let tested = cold.pairs_tested + cold.pairs_cached;
        assert_eq!(cache.find_collisions(&items, &options), first, "warm cache");
        assert_eq!(
            cache.stats().outlines_flattened,
            0,
            "nothing flattened again"
        );
        assert_eq!(cache.stats().pairs_tested, 0, "nothing tested again");
        assert_eq!(
            cache.stats().pairs_cached,
            tested,
            "every pair from the cache"
        );

        // A drag moves one mark: only the pairs of that mark are tested again.
        let mut moved = placements.clone();
        moved[4].1.y += 7.0;
        let items = long_items(&body, &mark, &moved);
        let dragged = cache.find_collisions(&items, &options);
        assert_eq!(
            dragged,
            find_collisions(&items, &options),
            "cached equals fresh"
        );
        let stats = cache.stats();
        assert!(
            (1..=4).contains(&stats.pairs_tested),
            "only the moved mark: {stats:?}"
        );
        // The whole line moves: the relative offsets stay, so nothing is tested again.
        let shifted: Vec<Placement> = moved
            .iter()
            .map(|&(key, origin, cluster, is_mark)| {
                (key, origin + Vec2::new(37.0, 0.0), cluster, is_mark)
            })
            .collect();
        let items = long_items(&body, &mark, &shifted);
        let moved_line = cache.find_collisions(&items, &options);
        assert_eq!(cache.stats().pairs_tested, 0, "{:?}", cache.stats());
        let expected = dragged[0].region + Vec2::new(37.0, 0.0);
        assert!(
            (moved_line[0].region.x0 - expected.x0).abs() < 1e-9
                && (moved_line[0].region.y1 - expected.y1).abs() < 1e-9,
            "results move with the line"
        );
    }

    /// One placed glyph of the fixture: name, outline, origin, cluster, and mark flag.
    type FixtureGlyph = (String, Arc<BezPath>, Point, usize, bool);

    /// The placed glyphs of `text`, shaped right to left with the live layout tables of
    /// `project`, with the outlines of the project.
    /// A mark gets the cluster of the base before it in logical order.
    fn shaped_line(project: &Project, source: SourceId, text: &str) -> Vec<FixtureGlyph> {
        let inputs = LayoutInputs::from_project(project, source).unwrap();
        let font = build_shaping_font(&inputs).unwrap();
        let mut pen = 0.0;
        let mut line: Vec<FixtureGlyph> = font
            .shape(text, true)
            .unwrap()
            .iter()
            .map(|shaped| {
                let name = font.glyph_name(shaped.glyph_id).unwrap().to_owned();
                let entry = project
                    .document_source_glyph_entry(source, &name)
                    .unwrap()
                    .unwrap();
                let origin = Point::new(pen + shaped.x_offset, shaped.y_offset);
                pen += shaped.x_advance;
                let is_mark = inputs.glyphs.iter().any(|glyph| {
                    glyph.name == name && glyph.category == Some(LayoutCategory::Mark)
                });
                let cluster = usize::try_from(shaped.cluster).unwrap();
                (name, entry.outline().clone(), origin, cluster, is_mark)
            })
            .collect();
        let bases: Vec<usize> = line.iter().filter(|g| !g.4).map(|g| g.3).collect();
        for glyph in line.iter_mut().filter(|g| g.4) {
            if let Some(base) = bases.iter().copied().filter(|&b| b <= glyph.3).max() {
                glyph.3 = base;
            }
        }
        line
    }

    fn line_collisions(line: &[FixtureGlyph], options: &CollisionOptions) -> Vec<Collision> {
        let items: Vec<PlacedGlyph<'_, &str>> = line
            .iter()
            .map(|(name, outline, origin, cluster, is_mark)| PlacedGlyph {
                key: name.as_str(),
                outline,
                origin: *origin,
                cluster: *cluster,
                is_mark: *is_mark,
            })
            .collect();
        find_collisions(&items, options)
    }

    #[test]
    fn a_raised_kasra_in_the_arabic_fixture_collides_with_the_dot_of_beh() {
        let mut project = Project::load(&crate::testing::fonts::arabic_regular_ufo()).unwrap();
        let source = project.document_sources().next().unwrap().id();
        let options = CollisionOptions {
            mark_to_own_base: true,
            ..CollisionOptions::for_units_per_em(1000.0)
        };
        let line = shaped_line(&project, source, "بِ");
        let names: Vec<&str> = line.iter().map(|glyph| glyph.0.as_str()).collect();
        assert_eq!(names, ["kasra-ar", "beh-ar"], "the shaping of the README");
        assert_eq!(line[0].3, line[1].3, "the kasra is in the cluster of beh");
        assert!(line[0].4 && !line[1].4, "the kasra is a mark");
        assert!(
            line_collisions(&line, &options).is_empty(),
            "the fixture is clear"
        );

        // Raise the kasra by 200 units: it enters the dot of beh.
        let layer = project.document_source(source).unwrap().default_layer();
        let anchor = project
            .document_layer("kasra-ar", &layer)
            .unwrap()
            .anchors()
            .find(|anchor| anchor.name() == "_bottom")
            .unwrap()
            .id();
        project
            .edit_document_layer("kasra-ar", &layer, |draft| {
                draft
                    .set_anchor_position(anchor, Point::new(0.0, -200.0))
                    .map(|_| ())
            })
            .unwrap();
        let line = shaped_line(&project, source, "بِ");
        let found = line_collisions(&line, &options);
        assert_eq!(found.len(), 1, "{found:?}");
        let hit = &found[0];
        assert_eq!(
            (line[hit.first].0.as_str(), line[hit.second].0.as_str()),
            ("kasra-ar", "beh-ar"),
            "the pair"
        );
        assert_eq!(
            (hit.first_role, hit.second_role),
            (IslandRole::Mark, IslandRole::Dot),
            "roles"
        );
        assert_eq!(hit.kind, CollisionKind::Overlap, "kind");
        let dot = Rect::new(220.0, -190.0, 300.0, -110.0) + line[1].2.to_vec2();
        assert!(
            dot.contains(hit.region.center()),
            "{:?} in {dot:?}",
            hit.region
        );
        // With the default options, a mark on its own base is not a collision.
        assert!(
            line_collisions(&line, &CollisionOptions::default()).is_empty(),
            "default options"
        );
    }
}
