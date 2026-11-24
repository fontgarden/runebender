// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Moving selected points, ported from the web editor's select-tool
//! translate (`translate_and_snap_in_path_with_handles` and friends in
//! runebender-web's `core/src/editor.rs`).
//!
//! Three rules make a drag feel right, and all three live here:
//!
//! 1. A selected on-curve point carries its adjacent off-curve
//!    handles, so a curve keeps its shape instead of collapsing.
//! 2. A dragged handle keeps its smooth neighbour's tangent: the
//!    opposite handle is mirrored through the smooth point (its length
//!    is preserved), and when the opposite side is an on-curve point
//!    the dragged handle is projected onto that tangent line instead.
//! 3. Every moved point lands on the grid explicitly supplied by its edit context.
//!
//! Alt/Option editing passes `independent`, which turns rule 1 off:
//! the selected points move alone.

use std::collections::{HashMap, HashSet};

/// Representation-neutral point input for canonical and compatibility drags.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PointState {
    pub(crate) position: kurbo::Point,
    pub(crate) off_curve: bool,
    pub(crate) smooth: bool,
}

/// The spacing of the editor's main dot grid, in font units, with a dot at the origin.
///
/// Snap to Grid moves points onto these dots; a finer grid of a quarter this spacing appears
/// only at high zoom.
pub const DOT_GRID_SPACING: f64 = 8.0;

/// The default design grid: whole font units.
pub const DEFAULT_GRID_SPACING: f64 = 1.0;

/// Snap one coordinate to the nearest multiple of `spacing`, or leave it when `spacing` is not
/// positive.
pub fn snap_to_spacing(value: f64, spacing: f64) -> f64 {
    if spacing.is_nan() || spacing <= 0.0 {
        return value;
    }
    (value / spacing).round() * spacing
}

fn snap_pt(p: kurbo::Point, spacing: f64) -> kurbo::Point {
    kurbo::Point::new(snap_to_spacing(p.x, spacing), snap_to_spacing(p.y, spacing))
}

/// The index `d` places from `index` around a contour of `len`
/// points.
///
/// A closed contour wraps. An open one stops at either end and
/// returns `None` past it.
pub(crate) fn step_index(index: usize, len: usize, closed: bool, d: isize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let offset = d.unsigned_abs();
    if d >= 0 {
        let j = index.checked_add(offset)?;
        if closed {
            Some(j % len)
        } else {
            (j < len).then_some(j)
        }
    } else if closed {
        Some((index + len - offset % len) % len)
    } else {
        index.checked_sub(offset)
    }
}

/// The opposite handle mirrored through `anchor`, keeping its own
/// length.
///
/// This is `mirrored_smooth_handle` in the web editor.
fn mirrored_smooth_handle(
    moved: kurbo::Point,
    anchor: kurbo::Point,
    opposite: kurbo::Point,
    spacing: f64,
) -> Option<kurbo::Point> {
    let v = moved - anchor;
    let len = v.hypot();
    if len < 1e-9 {
        return None;
    }
    let opposite_len = (opposite - anchor).hypot();
    if opposite_len < 1e-9 {
        return None;
    }
    Some(snap_pt(anchor - (v / len) * opposite_len, spacing))
}

/// The dragged handle projected onto the tangent through an
/// on-curve neighbour.
///
/// Only axis-aligned tangents snap: snapping a diagonal would pull
/// the handle off the line the projection just put it on. This is
/// `projected_smooth_handle` in the web editor.
fn projected_smooth_handle(
    moved: kurbo::Point,
    anchor: kurbo::Point,
    line_point: kurbo::Point,
    spacing: f64,
) -> Option<kurbo::Point> {
    let tangent = anchor - line_point;
    let tangent_len = tangent.hypot();
    if tangent_len < 1e-9 {
        return None;
    }
    let unit = tangent / tangent_len;
    let d = moved - anchor;
    let distance = (d.x * unit.x + d.y * unit.y).abs();
    let projected = anchor + unit * distance;
    if tangent.x.abs() < 1e-9 || tangent.y.abs() < 1e-9 {
        Some(snap_pt(projected, spacing))
    } else {
        Some(projected)
    }
}

/// Point indices whose current positions a persistent drag must preserve.
pub(crate) fn affected_indices(
    points: &[PointState],
    selected: &HashSet<usize>,
    closed: bool,
    independent: bool,
) -> Vec<usize> {
    let mut affected = Vec::new();
    let push = |affected: &mut Vec<usize>, index| {
        if !affected.contains(&index) {
            affected.push(index);
        }
    };
    for index in 0..points.len() {
        if !selected.contains(&index) {
            continue;
        }
        push(&mut affected, index);
        if !independent && !points[index].off_curve {
            for direction in [-1_isize, 1] {
                if let Some(neighbor) = step_index(index, points.len(), closed, direction)
                    && points[neighbor].off_curve
                {
                    push(&mut affected, neighbor);
                }
            }
        }
        if !points[index].off_curve {
            continue;
        }
        for direction in [-1_isize, 1] {
            let Some(anchor) = step_index(index, points.len(), closed, direction) else {
                continue;
            };
            if points[anchor].off_curve || !points[anchor].smooth {
                continue;
            }
            let Some(opposite) = step_index(anchor, points.len(), closed, direction) else {
                continue;
            };
            if !selected.contains(&opposite) && points[opposite].off_curve {
                push(&mut affected, opposite);
            }
        }
    }
    affected
}

fn moved_indices(
    points: &[PointState],
    selected: &HashSet<usize>,
    closed: bool,
    independent: bool,
) -> Vec<usize> {
    let mut moved = Vec::new();
    for &index in &affected_indices(points, selected, closed, independent) {
        if selected.contains(&index)
            || (!independent
                && points[index].off_curve
                && [-1_isize, 1].into_iter().any(|direction| {
                    step_index(index, points.len(), closed, direction).is_some_and(|neighbor| {
                        selected.contains(&neighbor) && !points[neighbor].off_curve
                    })
                }))
        {
            moved.push(index);
        }
    }
    moved
}

/// Calculate snapped point positions for one contour during a selection drag.
pub(crate) fn translated_positions(
    points: &[PointState],
    selected: &HashSet<usize>,
    originals: &HashMap<usize, kurbo::Point>,
    delta: (f64, f64),
    closed: bool,
    independent: bool,
    spacing: f64,
) -> Vec<(usize, kurbo::Point)> {
    let mut result = points.to_vec();
    let moved = moved_indices(points, selected, closed, independent);
    for &index in &moved {
        let base = originals
            .get(&index)
            .copied()
            .unwrap_or(points[index].position);
        result[index].position = snap_pt(base + kurbo::Vec2::new(delta.0, delta.1), spacing);
    }
    let mut smooth_updates = Vec::new();
    for &index in &moved {
        if !selected.contains(&index) || !result[index].off_curve {
            continue;
        }
        for direction in [-1_isize, 1] {
            let Some(anchor_index) = step_index(index, result.len(), closed, direction) else {
                continue;
            };
            if result[anchor_index].off_curve || !result[anchor_index].smooth {
                continue;
            }
            let Some(opposite) = step_index(anchor_index, result.len(), closed, direction) else {
                continue;
            };
            if selected.contains(&opposite) {
                continue;
            }
            let update = if result[opposite].off_curve {
                let opposite_position = if moved.contains(&opposite) {
                    result[opposite].position
                } else {
                    originals
                        .get(&opposite)
                        .copied()
                        .unwrap_or(result[opposite].position)
                };
                mirrored_smooth_handle(
                    result[index].position,
                    result[anchor_index].position,
                    opposite_position,
                    spacing,
                )
                .map(|position| (opposite, position))
            } else {
                projected_smooth_handle(
                    result[index].position,
                    result[anchor_index].position,
                    result[opposite].position,
                    spacing,
                )
                .map(|position| (index, position))
            };
            if let Some(update) = update {
                smooth_updates.push(update);
            }
        }
    }
    for (index, position) in smooth_updates {
        result[index].position = position;
    }
    result
        .iter()
        .zip(points)
        .enumerate()
        .filter(|(_, (after, before))| after.position != before.position)
        .map(|(index, (after, _))| (index, after.position))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed contour: corner, handle, handle, smooth on-curve,
    /// handle, handle. Enough shape to test tangents.
    fn curve_points() -> Vec<PointState> {
        let p = |x: f64, y: f64, off_curve: bool, smooth: bool| PointState {
            position: kurbo::Point::new(x, y),
            off_curve,
            smooth,
        };
        vec![
            p(0.0, 0.0, false, false),
            p(20.0, 0.0, true, false),
            p(100.0, 20.0, true, false),
            p(100.0, 100.0, false, true),
            p(100.0, 180.0, true, false),
            p(20.0, 200.0, true, false),
            p(0.0, 200.0, false, false),
            p(-20.0, 100.0, true, false),
            p(-20.0, 50.0, true, false),
        ]
    }

    fn at(points: &[PointState], index: usize) -> (f64, f64) {
        (points[index].position.x, points[index].position.y)
    }

    fn selection(indices: &[usize]) -> HashSet<usize> {
        indices.iter().copied().collect()
    }

    /// The positions a persistent drag captures before its first event.
    fn drag_origins(
        points: &[PointState],
        selected: &HashSet<usize>,
        independent: bool,
    ) -> HashMap<usize, kurbo::Point> {
        affected_indices(points, selected, true, independent)
            .into_iter()
            .map(|index| (index, points[index].position))
            .collect()
    }

    /// Apply one drag event to a closed contour on the default design grid.
    fn translate(
        points: &mut [PointState],
        selected: &HashSet<usize>,
        originals: &HashMap<usize, kurbo::Point>,
        delta: (f64, f64),
        independent: bool,
    ) -> bool {
        let moves = translated_positions(
            points,
            selected,
            originals,
            delta,
            true,
            independent,
            DEFAULT_GRID_SPACING,
        );
        for &(index, position) in &moves {
            points[index].position = position;
        }
        !moves.is_empty()
    }

    fn positions(points: &[PointState]) -> Vec<kurbo::Point> {
        points.iter().map(|point| point.position).collect()
    }

    #[test]
    fn on_curve_point_carries_its_handles() {
        let mut points = curve_points();
        assert!(translate(
            &mut points,
            &selection(&[3]),
            &HashMap::new(),
            (10.0, 0.0),
            false
        ));
        assert_eq!(at(&points, 3), (110.0, 100.0));
        // Both adjacent handles moved with it.
        assert_eq!(at(&points, 2), (110.0, 20.0));
        assert_eq!(at(&points, 4), (110.0, 180.0));
        // A point two steps away stayed put.
        assert_eq!(at(&points, 1), (20.0, 0.0));
    }

    #[test]
    fn independent_leaves_handles_behind() {
        let mut points = curve_points();
        translate(
            &mut points,
            &selection(&[3]),
            &HashMap::new(),
            (10.0, 0.0),
            true,
        );
        assert_eq!(at(&points, 3), (110.0, 100.0));
        assert_eq!(at(&points, 2), (100.0, 20.0));
        assert_eq!(at(&points, 4), (100.0, 180.0));
    }

    #[test]
    fn dragging_a_handle_mirrors_the_smooth_opposite() {
        let mut points = curve_points();
        // Point 4 is the handle after the smooth on-curve 3; its
        // opposite is point 2, 80 units away on the other side.
        translate(
            &mut points,
            &selection(&[4]),
            &HashMap::new(),
            (0.0, 20.0),
            false,
        );
        assert_eq!(at(&points, 4), (100.0, 200.0));
        // The opposite handle stays collinear through (100, 100) and
        // keeps its own length.
        assert_eq!(at(&points, 2), (100.0, 20.0));
    }

    #[test]
    fn positions_land_on_the_design_grid() {
        let mut points = curve_points();
        translate(
            &mut points,
            &selection(&[0]),
            &HashMap::new(),
            (3.4, 2.6),
            true,
        );
        assert_eq!(at(&points, 0), (3.0, 3.0));
    }

    #[test]
    fn originals_anchor_a_long_drag() {
        let mut points = curve_points();
        let selected = selection(&[0]);
        let originals = [(0, kurbo::Point::ZERO)].into_iter().collect();
        // Two drag events from the same start: the second wins outright
        // instead of stacking on the first.
        translate(&mut points, &selected, &originals, (10.0, 0.0), true);
        translate(&mut points, &selected, &originals, (20.0, 0.0), true);
        assert_eq!(at(&points, 0), (20.0, 0.0));
    }

    #[test]
    fn captured_origins_anchor_carried_handles_across_snapping_thresholds() {
        let mut original = curve_points();
        original[2].position.x = 101.0;
        original[4].position.x = 101.0;
        let selected = selection(&[3]);
        let originals = drag_origins(&original, &selected, false);
        assert_eq!(originals.len(), 3);
        let mut one_event = original.clone();
        translate(&mut one_event, &selected, &originals, (2.0, 0.0), false);
        let mut two_events = original;
        translate(&mut two_events, &selected, &originals, (1.0, 0.0), false);
        translate(&mut two_events, &selected, &originals, (2.0, 0.0), false);
        assert_eq!(positions(&two_events), positions(&one_event));
        assert_eq!(at(&two_events, 2), (103.0, 20.0));
        assert_eq!(at(&two_events, 3), (102.0, 100.0));
        assert_eq!(at(&two_events, 4), (103.0, 180.0));
    }

    #[test]
    fn a_persistent_drag_needs_every_carried_origin() {
        // The canonical edit rejects a drag whose origins omit any of these indices.
        let mut affected = affected_indices(&curve_points(), &selection(&[3]), true, false);
        affected.sort_unstable();
        assert_eq!(affected, [2, 3, 4]);
    }

    #[test]
    fn captured_origins_anchor_smooth_opposites_across_drag_events() {
        let mut original = curve_points();
        original[2].position = kurbo::Point::new(101.0, 19.0);
        original[4].position = kurbo::Point::new(101.0, 181.0);
        let selected = selection(&[4]);
        let originals = drag_origins(&original, &selected, false);
        assert_eq!(originals.len(), 2);
        let mut one_event = original.clone();
        translate(&mut one_event, &selected, &originals, (3.0, 1.0), false);
        let mut two_events = original;
        translate(&mut two_events, &selected, &originals, (1.0, 0.0), false);
        translate(&mut two_events, &selected, &originals, (3.0, 1.0), false);
        assert_eq!(positions(&two_events), positions(&one_event));
    }

    #[test]
    fn unrelated_selected_handle_does_not_carry_a_smooth_opposite() {
        let original = curve_points();
        let primary = selection(&[4]);
        let multiple = selection(&[1, 4]);
        let mut primary_result = original.clone();
        let primary_origins = drag_origins(&primary_result, &primary, false);
        translate(
            &mut primary_result,
            &primary,
            &primary_origins,
            (20.0, 0.0),
            false,
        );
        let mut multiple_result = original;
        let multiple_origins = drag_origins(&multiple_result, &multiple, false);
        translate(
            &mut multiple_result,
            &multiple,
            &multiple_origins,
            (20.0, 0.0),
            false,
        );
        assert_eq!(at(&multiple_result, 2), at(&primary_result, 2));
    }
}
