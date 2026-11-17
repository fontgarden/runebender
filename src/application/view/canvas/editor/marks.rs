// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The design grid, point markers, start markers and metric crosses that the canvas paints.

use super::*;

/// Append one round design-grid dot in screen coordinates to `marks`.
///
/// The visible grid holds thousands of dots, so each one is appended to the shared path
/// rather than built as a path of its own.
pub(super) fn push_grid_dot(marks: &mut kurbo::BezPath, at: Point, diameter: f64) {
    marks.extend(Circle::new(at, diameter / 2.0).path_elements(0.1));
}

/// What a cached field of grid dots was built for: the dot period and diameter in pixels,
/// the canvas size, and how often a dot is left out for the coarser level.
pub(super) type GridDotsKey = (u64, u64, u64, u64, u64, i64);

/// A field of grid dots that covers a `width` by `height` canvas at any pan offset.
///
/// Dots sit every `period_x` by `period_y` pixels from the origin. With `skip_every` above
/// zero, a dot whose column and row are both multiples of it is left out, because the coarser
/// level draws that one. Draw the field with [`grid_dots_offset`] for each axis.
pub(super) fn grid_dots_field(
    period_x: f64,
    period_y: f64,
    diameter: f64,
    width: f64,
    height: f64,
    skip_every: i64,
) -> kurbo::BezPath {
    let centers = grid_dot_centers(period_x, period_y, width, height, skip_every);
    // Six elements per dot: a move, four curves, and a close.
    let mut marks = kurbo::BezPath::with_capacity(centers.len() * 6);
    for at in centers {
        push_grid_dot(&mut marks, at, diameter);
    }
    marks
}

/// The dot centers of a [`grid_dots_field`], before it is moved to the pan offset.
pub(super) fn grid_dot_centers(
    period_x: f64,
    period_y: f64,
    width: f64,
    height: f64,
    skip_every: i64,
) -> Vec<Point> {
    let repeat = skip_every.max(1) as f64;
    let count = |length: f64, period: f64| -> i64 {
        let cells = ((length + 2.0 * period * repeat) / period).ceil();
        // A zoom that draws the grid keeps this far below the limit; the cast cannot overflow.
        grid_index(cells.clamp(0.0, 100_000.0))
    };
    let (columns, rows) = (count(width, period_x), count(height, period_y));
    let mut centers =
        Vec::with_capacity(usize::try_from((columns + 1) * (rows + 1)).unwrap_or_default());
    for column in 0..=columns {
        for row in 0..=rows {
            if skip_every > 0 && column % skip_every == 0 && row % skip_every == 0 {
                continue;
            }
            centers.push(Point::new(column as f64 * period_x, row as f64 * period_y));
        }
    }
    centers
}

/// Where to draw a [`grid_dots_field`] on one axis so its dots land on the grid.
///
/// `origin` is the screen position of grid index zero. The result is at most one repeat of
/// the skip pattern before the canvas edge, and a whole number of repeats from `origin`, so
/// the left-out dots stay on the coarser level's positions.
pub(super) fn grid_dots_offset(origin: f64, period: f64, skip_every: i64) -> f64 {
    let repeat = period * skip_every.max(1) as f64;
    origin.rem_euclid(repeat) - repeat
}

/// A horizontal rule whose visual centre stays on the supplied coordinate.
pub(super) fn horizontal_rule_rect(x0: f64, x1: f64, y: f64, width: f64) -> Rect {
    let half = width / 2.0;
    Rect::new(x0.min(x1), y - half, x0.max(x1), y + half)
}

/// A vertical rule whose visual centre stays on the supplied coordinate.
pub(super) fn vertical_rule_rect(x: f64, y0: f64, y1: f64, width: f64) -> Rect {
    let half = width / 2.0;
    Rect::new(x - half, y0.min(y1), x + half, y0.max(y1))
}

/// Hermite ease from zero to one.
fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The design grid's coarse and fine opacity at this zoom.
pub(super) fn grid_alphas(zoom: f64) -> (f64, f64) {
    (
        smoothstep((zoom - 0.8) / 0.8),
        smoothstep((zoom - 8.0) / 8.0),
    )
}

/// Build the part of one design-grid level visible through a point marker.
pub(super) fn point_grid_marks(
    affine: Affine,
    center: Point,
    radius: f64,
    square: bool,
    spacing: f64,
    dot_size: f64,
    grid_lines: bool,
) -> kurbo::BezPath {
    let inv = affine.inverse();
    let a = (inv * Point::new(center.x - radius, center.y)).x;
    let b = (inv * Point::new(center.x + radius, center.y)).x;
    let (lo_x, hi_x) = (a.min(b), a.max(b));
    let a = (inv * Point::new(center.x, center.y - radius)).y;
    let b = (inv * Point::new(center.x, center.y + radius)).y;
    let (lo_y, hi_y) = (a.min(b), a.max(b));
    let xs = grid_index((lo_x / spacing).ceil())..=grid_index((hi_x / spacing).floor());
    let ys = grid_index((lo_y / spacing).ceil())..=grid_index((hi_y / spacing).floor());
    let mut marks = kurbo::BezPath::new();

    if grid_lines {
        let half_at = |distance: f64| {
            if square {
                radius
            } else {
                (radius * radius - distance * distance).max(0.0).sqrt()
            }
        };
        for x_index in xs.clone() {
            let x = (affine * Point::new(x_index as f64 * spacing, 0.0)).x;
            let half = half_at(x - center.x);
            if half > 0.2 {
                marks.move_to(Point::new(x, center.y - half));
                marks.line_to(Point::new(x, center.y + half));
            }
        }
        for y_index in ys {
            let y = (affine * Point::new(0.0, y_index as f64 * spacing)).y;
            let half = half_at(y - center.y);
            if half > 0.2 {
                marks.move_to(Point::new(center.x - half, y));
                marks.line_to(Point::new(center.x + half, y));
            }
        }
    } else {
        for x_index in xs {
            for y_index in ys.clone() {
                let at = affine * Point::new(x_index as f64 * spacing, y_index as f64 * spacing);
                let dx = at.x - center.x;
                let dy = at.y - center.y;
                let inside = if square {
                    dx.abs() <= radius && dy.abs() <= radius
                } else {
                    dx * dx + dy * dy <= radius * radius
                };
                if inside {
                    push_grid_dot(&mut marks, at, dot_size);
                }
            }
        }
    }
    marks
}

pub(super) fn point_marker_shape(center: Point, radius: f64, square: bool) -> kurbo::BezPath {
    if square {
        kurbo::Shape::to_path(
            &Rect::new(
                center.x - radius,
                center.y - radius,
                center.x + radius,
                center.y + radius,
            ),
            0.1,
        )
    } else {
        kurbo::Shape::to_path(&Circle::new(center, radius), 0.15)
    }
}

/// Replace a closed contour's first node with the GPUI direction wedge.
pub(super) fn direction_marker_shape(
    center: Point,
    toward: Point,
    radius: f64,
    smooth: bool,
) -> Option<kurbo::BezPath> {
    let direction = toward - center;
    let length = direction.hypot();
    if length < f64::EPSILON {
        return None;
    }
    let forward = direction / length;
    let side = kurbo::Vec2::new(-forward.y, forward.x);
    let radius = radius * START_MARKER_SCALE;
    let tip = center + forward * radius * START_MARKER_TIP;
    let left =
        center - forward * radius * START_MARKER_BACK + side * radius * START_MARKER_HALF_WIDTH;
    let right =
        center - forward * radius * START_MARKER_BACK - side * radius * START_MARKER_HALF_WIDTH;

    let mut path = kurbo::BezPath::new();
    if smooth {
        let toward = |a: Point, b: Point| {
            Point::new(
                a.x + (b.x - a.x) * START_MARKER_SMOOTH_CUT,
                a.y + (b.y - a.y) * START_MARKER_SMOOTH_CUT,
            )
        };
        path.move_to(toward(tip, right));
        path.quad_to(tip, toward(tip, left));
        path.line_to(toward(left, tip));
        path.quad_to(left, toward(left, right));
        path.line_to(toward(right, left));
        path.quad_to(right, toward(right, tip));
    } else {
        path.move_to(tip);
        path.line_to(left);
        path.line_to(right);
    }
    path.close_path();
    Some(path)
}

/// Metric heights shared by every text sort, deduplicated so equal font
/// metrics do not paint darker than their neighbours.
pub(super) fn text_sort_metric_ys(
    metrics: &crate::application::editor::session::Metrics,
) -> Vec<f64> {
    let mut ys = vec![
        metrics.descender,
        0.0,
        metrics.ascender,
        metrics.upm.max(metrics.ascender),
        metrics.x_height,
        metrics.cap_height,
    ];
    ys.retain(|y| y.is_finite());
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    ys
}

/// Draw dark marks inward from each intersection of a sort edge and metric line.
/// Paint these after the quiet metric rules so neighbouring sorts cannot cover them.
pub(super) fn paint_metric_crosses(
    painter: &mut Painter<'_>,
    xs: [f64; 2],
    y_bounds: [f64; 2],
    ys: impl Clone + Iterator<Item = f64>,
    mark: f64,
    color: xilem::Color,
) {
    if mark < 3.0 {
        return;
    }
    let mark = mark * 0.5;
    let left = xs[0].min(xs[1]);
    let right = xs[0].max(xs[1]);
    let top = y_bounds[0].min(y_bounds[1]);
    let bottom = y_bounds[0].max(y_bounds[1]);
    let inward = mark.min((right - left) / 2.0);
    let rule = Stroke::new(DesignStroke::Hairline.px());
    for (x, inner_x) in [(left, left + inward), (right, right - inward)] {
        for y in ys.clone() {
            painter
                .stroke(Line::new((x, y), (inner_x, y)), &rule, color)
                .draw();
            painter
                .stroke(
                    Line::new((x, (y - mark).max(top)), (x, (y + mark).min(bottom))),
                    &rule,
                    color,
                )
                .draw();
        }
    }
}

/// A grid line index from an already rounded coordinate. The visible
/// canvas is a few thousand units at most, so the cast cannot truncate.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the visible canvas spans a few thousand units"
)]
pub(super) fn grid_index(v: f64) -> i64 {
    v as i64
}
