// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Outline effects: operations that produce a new shape from a contour.
//!
//! Expand a stroke, offset, extrude, roughen, apply a corner component,
//! and bolden by a learned per-point offset.
//! The geometry kernels operate on Kurbo paths; the canonical edits in `LayerEditDraft` call them.

use kurbo::{Affine, BezPath, PathEl};

use crate::outline::glyph_paths::round_units;

fn split_subpaths(path: &BezPath) -> Vec<BezPath> {
    let mut paths = Vec::new();
    let mut current = BezPath::new();
    for element in path.elements() {
        if matches!(element, PathEl::MoveTo(_)) && !current.elements().is_empty() {
            paths.push(current);
            current = BezPath::new();
        }
        current.push(*element);
    }
    if !current.elements().is_empty() {
        paths.push(current);
    }
    paths
}

fn rounded_closed_path(path: &BezPath) -> BezPath {
    let mut output = BezPath::new();
    for element in path.elements() {
        match element {
            PathEl::MoveTo(point) => output.move_to(point.round()),
            PathEl::LineTo(point) => output.line_to(point.round()),
            PathEl::QuadTo(control, point) => output.quad_to(control.round(), point.round()),
            PathEl::CurveTo(first, second, point) => {
                output.curve_to(first.round(), second.round(), point.round());
            }
            PathEl::ClosePath => output.close_path(),
        }
    }
    if !matches!(output.elements().last(), Some(PathEl::ClosePath)) {
        output.close_path();
    }
    output
}

/// Return the closed subpaths produced by stroking one path.
pub fn expanded_stroke_paths(path: &BezPath, width: f64) -> Vec<BezPath> {
    if !width.is_finite() || width <= 0.0 || path.is_empty() {
        return Vec::new();
    }
    let style = kurbo::Stroke::new(width);
    let stroked = kurbo::stroke(
        path.elements().iter().copied(),
        &style,
        &kurbo::StrokeOpts::default(),
        0.25,
    );
    split_subpaths(&stroked)
        .iter()
        .map(rounded_closed_path)
        .collect()
}

/// Offset a set of paths by unioning or subtracting their stroke band.
///
/// `Some` reports a successful geometry operation, including a valid empty result.
pub fn offset_paths(paths: &[BezPath], delta: f64) -> Option<Vec<BezPath>> {
    if !delta.is_finite() || delta == 0.0 || paths.is_empty() {
        return None;
    }
    let mut combined = BezPath::new();
    let mut band = BezPath::new();
    // A miter join keeps a corner a corner. The default round join replaced each one with
    // an arc of radius `delta`.
    let style = kurbo::Stroke::new(delta.abs() * 2.0).with_join(kurbo::Join::Miter);
    let opts = kurbo::StrokeOpts::default();
    for path in paths {
        band.extend(
            kurbo::stroke(path.elements().iter().copied(), &style, &opts, 0.25)
                .elements()
                .iter()
                .copied(),
        );
        combined.extend(path.elements().iter().copied());
    }
    let operation = if delta > 0.0 {
        linesweeper::BinaryOp::Union
    } else {
        linesweeper::BinaryOp::Difference
    };
    let result =
        linesweeper::binary_op(&combined, &band, linesweeper::FillRule::NonZero, operation).ok()?;
    Some(
        result
            .contours()
            .map(|contour| rounded_closed_path(&contour.path))
            .collect(),
    )
}

/// Sweep paths along an angle and return the resulting silhouette or side faces.
///
/// `Some` reports a successful geometry operation, including a valid empty result.
pub fn extruded_paths(
    paths: &[BezPath],
    offset: f64,
    angle_degrees: f64,
    keep_front: bool,
) -> Option<Vec<BezPath>> {
    if !offset.is_finite() || !angle_degrees.is_finite() || offset <= 0.0 || paths.is_empty() {
        return None;
    }
    let (sin, cos) = (-angle_degrees).to_radians().sin_cos();
    let delta = kurbo::Vec2::new(offset * cos, offset * sin);
    let mut combined = BezPath::new();
    let mut front = BezPath::new();
    for path in paths {
        front.extend(path.elements().iter().copied());
        combined.extend(path.elements().iter().copied());
        combined.extend((Affine::translate(delta) * path).elements().iter().copied());
        let mut walls = BezPath::new();
        for segment in path.segments() {
            use kurbo::ParamCurve as _;
            let (a, b) = (segment.eval(0.0), segment.eval(1.0));
            let (a2, b2) = (a + delta, b + delta);
            let area = (b.x - a.x) * (b2.y - a.y) - (b2.x - a.x) * (b.y - a.y);
            let quad = if area >= 0.0 {
                [a, b, b2, a2]
            } else {
                [a, a2, b2, b]
            };
            walls.move_to(quad[0]);
            walls.line_to(quad[1]);
            walls.line_to(quad[2]);
            walls.line_to(quad[3]);
            walls.close_path();
        }
        combined.extend(walls.elements().iter().copied());
    }
    let silhouette = linesweeper::binary_op(
        &combined,
        &BezPath::new(),
        linesweeper::FillRule::NonZero,
        linesweeper::BinaryOp::Union,
    )
    .ok()?;
    let mut merged = BezPath::new();
    for contour in silhouette.contours() {
        merged.extend(contour.path.elements().iter().copied());
    }
    if keep_front {
        return Some(
            split_subpaths(&merged)
                .iter()
                .map(rounded_closed_path)
                .collect(),
        );
    }
    let result = linesweeper::binary_op(
        &merged,
        &front,
        linesweeper::FillRule::NonZero,
        linesweeper::BinaryOp::Difference,
    )
    .ok()?;
    Some(
        result
            .contours()
            .map(|contour| rounded_closed_path(&contour.path))
            .collect(),
    )
}

/// Flatten and jitter one path using a deterministic shared random state.
pub fn roughened_path(
    path: &BezPath,
    segment_length: f64,
    horizontal: f64,
    vertical: f64,
    state: &mut u64,
) -> Option<BezPath> {
    use kurbo::ParamCurve as _;
    use kurbo::ParamCurveArclen as _;

    if !segment_length.is_finite()
        || !horizontal.is_finite()
        || !vertical.is_finite()
        || segment_length < 1.0
    {
        return None;
    }
    let mut jitter = |amount: f64| {
        *state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let unit = (*state >> 11) as f64 / (1_u64 << 53) as f64;
        (unit * 2.0 - 1.0) * amount
    };
    let mut points = Vec::new();
    for segment in path.segments() {
        let length = segment.arclen(0.5);
        let steps =
            usize::try_from(round_units((length / segment_length).ceil().max(1.0))).unwrap_or(1);
        for step in 0..steps {
            let parameter = step as f64 / steps as f64;
            let point = segment.eval(parameter);
            points.push(kurbo::Point::new(
                (point.x + jitter(horizontal)).round(),
                (point.y + jitter(vertical)).round(),
            ));
        }
    }
    if points.len() < 3 {
        return None;
    }
    let mut output = BezPath::new();
    output.move_to(points[0]);
    for point in &points[1..] {
        output.line_to(*point);
    }
    output.close_path();
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape as _;

    fn square() -> BezPath {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((100.0, 0.0));
        path.line_to((100.0, 100.0));
        path.line_to((0.0, 100.0));
        path.close_path();
        path
    }

    fn bounds(paths: &[BezPath]) -> kurbo::Rect {
        paths
            .iter()
            .map(|path| path.bounding_box())
            .reduce(|a, b| a.union(b))
            .expect("at least one path")
    }

    #[test]
    fn extrude_and_roughen_transform_a_square() {
        // Extrude right-down at 30° by 40: the box grows +40·cos30 in
        // x and −40·sin30 in y, and the front face is cut away.
        let extruded = extruded_paths(&[square()], 40.0, 30.0, false).expect("extrudes");
        let bbox = bounds(&extruded);
        assert!((bbox.x1 - (100.0 + 40.0 * (30_f64).to_radians().cos())).abs() <= 2.0);
        assert!((bbox.y0 - (-40.0 * (30_f64).to_radians().sin())).abs() <= 2.0);

        // Roughen: many short jittered segments replace the four.
        let mut state = 7_u64.wrapping_mul(6364136223846793005).wrapping_add(1);
        let rough = roughened_path(&square(), 10.0, 4.0, 4.0, &mut state).expect("roughens");
        assert!(
            rough.segments().count() >= 30,
            "flattened into short segments: {}",
            rough.segments().count()
        );
        // Different seed, different rough.
        let mut other_state = 8_u64.wrapping_mul(6364136223846793005).wrapping_add(1);
        let other = roughened_path(&square(), 10.0, 4.0, 4.0, &mut other_state).expect("roughens");
        assert_ne!(rough, other);
    }

    #[test]
    fn offset_bolder_and_lighter() {
        let bolder = offset_paths(&[square()], 10.0).expect("offsets outward");
        let bbox = bounds(&bolder);
        assert!(
            (bbox.width() - 120.0).abs() <= 2.0 && (bbox.height() - 120.0).abs() <= 2.0,
            "bolder grows: {}x{}",
            bbox.width(),
            bbox.height()
        );
        // A square with sharp corners has the full area; round corners lose 86 units of it.
        let area: f64 = bolder.iter().map(|path| path.area().abs()).sum();
        assert!(
            (area - 14_400.0).abs() <= 20.0,
            "corners stay sharp: {area}"
        );
        let lighter = offset_paths(&[square()], -10.0).expect("offsets inward");
        let bbox = bounds(&lighter);
        assert!(
            (bbox.width() - 80.0).abs() <= 2.0 && (bbox.height() - 80.0).abs() <= 2.0,
            "lighter shrinks: {}x{}",
            bbox.width(),
            bbox.height()
        );
    }

    #[test]
    fn expand_stroke_makes_outlines() {
        // An open two-point skeleton line from (0,0) to (100,0).
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((100.0, 0.0));
        let outlines = expanded_stroke_paths(&line, 40.0);
        // The skeleton became a closed outline that spans the stroke:
        // 100 long plus round caps of radius 20 each side, 40 tall.
        assert_eq!(outlines.len(), 1);
        assert!(matches!(
            outlines[0].elements().last(),
            Some(PathEl::ClosePath)
        ));
        let bbox = outlines[0].bounding_box();
        assert!((bbox.height() - 40.0).abs() <= 2.0, "stroke height ~40");
        assert!((bbox.width() - 140.0).abs() <= 2.0, "length plus caps ~140");
        // Width zero and an empty path refuse.
        assert!(expanded_stroke_paths(&line, 0.0).is_empty());
        assert!(expanded_stroke_paths(&BezPath::new(), 40.0).is_empty());
    }
}
