// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Tests of the design grid, point and start markers, and the text cursor blink cycle.

use super::*;

/// The cached dot field, moved to a pan offset, puts dots exactly where drawing each grid
/// intersection through the view transform would, and leaves out the same ones.
#[test]
fn cached_grid_dots_match_dots_placed_through_the_view_transform() {
    let (width, height) = (640.0, 480.0);
    let spacing = 8.0;
    for (zoom, pan, skip_every) in [
        (1.0, (0.0, 0.0), 0_i64),
        (1.7, (213.4, -97.25), 0),
        (9.3, (-1234.5, 3210.75), 4),
        (12.0, (55.0, 480.0), 4),
    ] {
        // The canvas flips the y axis, as the editor's view transform does.
        let affine = Affine::new([zoom, 0.0, 0.0, -zoom, pan.0, pan.1]);
        let on_canvas =
            |point: &Point| (0.0..=width).contains(&point.x) && (0.0..=height).contains(&point.y);
        let rounded = |point: Point| {
            (
                grid_index((point.x * 100.0).round()),
                grid_index((point.y * 100.0).round()),
            )
        };

        let inverse = affine.inverse();
        let (a, b) = (inverse * Point::ZERO, inverse * Point::new(width, height));
        let index = |value: f64| grid_index((value / spacing).floor());
        let mut expected = std::collections::BTreeSet::new();
        for ix in index(a.x.min(b.x)) - 1..=index(a.x.max(b.x)) + 1 {
            for iy in index(a.y.min(b.y)) - 1..=index(a.y.max(b.y)) + 1 {
                if skip_every > 0 && ix % skip_every == 0 && iy % skip_every == 0 {
                    continue;
                }
                let at = affine * Point::new(ix as f64 * spacing, iy as f64 * spacing);
                if on_canvas(&at) {
                    expected.insert(rounded(at));
                }
            }
        }

        let period = spacing * zoom;
        let offset = kurbo::Vec2::new(
            grid_dots_offset(pan.0, period, skip_every),
            grid_dots_offset(pan.1, period, skip_every),
        );
        let cached: std::collections::BTreeSet<_> =
            grid_dot_centers(period, period, width, height, skip_every)
                .into_iter()
                .map(|at| at + offset)
                .filter(on_canvas)
                .map(rounded)
                .collect();
        assert!(!expected.is_empty(), "zoom {zoom} draws dots");
        assert_eq!(
            cached, expected,
            "zoom {zoom}, pan {pan:?}, skip {skip_every}"
        );
    }
}

#[test]
fn design_grid_dot_is_round_instead_of_a_square_tile() {
    let mut dot = kurbo::BezPath::new();
    push_grid_dot(&mut dot, Point::new(10.0, 10.0), 4.0);
    assert_eq!(dot.bounding_box(), Rect::new(8.0, 8.0, 12.0, 12.0));
    assert_ne!(dot.winding(Point::new(10.0, 10.0)), 0);
    assert_eq!(
        dot.winding(Point::new(8.2, 8.2)),
        0,
        "a rounded dot does not fill its bounding-box corner"
    );
}

#[test]
fn metric_rules_are_centered_on_their_design_coordinates() {
    let horizontal = horizontal_rule_rect(10.0, 30.0, 12.25, 1.0);
    assert_eq!(horizontal.center().y, 12.25);
    assert_eq!(horizontal.height(), 1.0);

    let vertical = vertical_rule_rect(18.75, 40.0, 5.0, 1.0);
    assert_eq!(vertical.center().x, 18.75);
    assert_eq!(vertical.width(), 1.0);
}

#[test]
fn point_window_repaints_the_grid_intersection_under_its_center() {
    let marks = point_grid_marks(
        Affine::IDENTITY,
        Point::new(8.0, 8.0),
        4.5,
        false,
        8.0,
        2.0,
        false,
    );
    assert_ne!(marks.winding(Point::new(8.0, 8.0)), 0);
    assert_eq!(marks.bounding_box(), Rect::new(7.0, 7.0, 9.0, 9.0));
}

#[test]
fn point_window_preserves_an_off_center_grid_dot() {
    let marks = point_grid_marks(
        Affine::IDENTITY,
        Point::new(10.0, 8.0),
        4.5,
        false,
        8.0,
        2.0,
        false,
    );
    assert_ne!(marks.winding(Point::new(8.0, 8.0)), 0);
    assert_eq!(marks.winding(Point::new(10.0, 8.0)), 0);
}

#[test]
fn point_window_clips_line_grid_to_the_marker() {
    let marks = point_grid_marks(
        Affine::IDENTITY,
        Point::new(8.0, 8.0),
        4.5,
        false,
        8.0,
        2.0,
        true,
    );
    assert_eq!(marks.bounding_box(), Rect::new(3.5, 3.5, 12.5, 12.5));
}

#[test]
fn grid_dots_keep_their_colored_clearance_inside_point_markers() {
    for zoom in [0.1, 0.8, 1.6, 3.2, 8.0, 12.0, 16.0, 28.0, 64.0] {
        let scale = point_marker_scale(zoom);
        let (coarse, fine) = point_grid_dot_sizes(zoom);
        let ring = (POINT_RING_WIDTH * scale).max(DesignStroke::Hairline.px());
        for (radius, expected_gap) in [(POINT_CORNER_RADIUS, 1.5), (POINT_CURVE_RADIUS, 2.5)] {
            let gap = radius * scale - ring / 2.0 - coarse / 2.0;
            assert!((gap / scale - expected_gap).abs() < 1e-9, "zoom {zoom}");
        }
        assert!((fine / coarse - 0.6).abs() < 1e-9);
    }
}

#[test]
fn design_grid_levels_fade_in_at_the_gpui_thresholds() {
    assert_eq!(grid_alphas(0.8), (0.0, 0.0));
    assert_eq!(grid_alphas(1.6), (1.0, 0.0));
    assert_eq!(grid_alphas(8.0), (1.0, 0.0));
    assert_eq!(grid_alphas(16.0), (1.0, 1.0));
}

#[test]
fn text_cursor_blinks_on_a_one_second_cycle() {
    assert!(cursor_visible_at(0));
    assert!(cursor_visible_at(CURSOR_BLINK_HALF_CYCLE_NS));
    assert!(!cursor_visible_at(CURSOR_BLINK_HALF_CYCLE_NS + 1));
    assert!(!cursor_visible_at(CURSOR_BLINK_CYCLE_NS - 1));
    assert!(cursor_visible_at(CURSOR_BLINK_CYCLE_NS));
}

#[test]
fn start_markers_use_closed_contours_and_first_on_curve_direction() {
    let mut editor = widget();
    let affine = editor.session.viewport.affine();
    let initial = editor.start_markers();
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].0, editor.session.point_id_at(0, 0).unwrap());
    assert_eq!(initial[0].1, affine * Point::new(0.0, 0.0));
    assert_eq!(initial[0].2, affine * Point::new(400.0, 0.0));
    let first = editor.session.point_id_at(0, 0).unwrap();
    assert!(
        editor
            .session
            .stage_canonical_string_edit("test point kind", |draft| {
                draft
                    .set_point_type(first, runebender::font::LayerPointType::OffCurve)
                    .map_err(|error| error.to_string())
            })
            .unwrap()
    );
    assert_eq!(
        editor.start_markers()[0].0,
        editor.session.point_id_at(0, 1).unwrap(),
        "skip leading handles"
    );
    let first = editor.session.point_id_at(0, 0).unwrap();
    assert!(
        editor
            .session
            .stage_canonical_string_edit("test open path", |draft| {
                draft
                    .set_point_type(first, runebender::font::LayerPointType::Move)
                    .map_err(|error| error.to_string())
            })
            .unwrap()
    );
    assert!(
        editor.start_markers().is_empty(),
        "open paths have no marker"
    );
    let points = (0..)
        .map_while(|index| editor.session.point_id_at(0, index))
        .collect::<Vec<_>>();
    assert!(
        editor
            .session
            .stage_canonical_string_edit("test empty path", |draft| {
                draft
                    .delete_points(&points)
                    .map_err(|error| error.to_string())
            })
            .unwrap()
    );
    assert!(
        editor.start_markers().is_empty(),
        "empty contours are harmless"
    );
}

#[test]
fn start_marker_replaces_the_node_and_softens_only_smooth_starts() {
    let center = Point::new(20.0, 20.0);
    let toward = Point::new(30.0, 20.0);
    let sharp = direction_marker_shape(center, toward, POINT_CORNER_RADIUS, false).unwrap();
    let smooth = direction_marker_shape(center, toward, POINT_CURVE_RADIUS, true).unwrap();

    assert_eq!(
        sharp.elements().len(),
        4,
        "a corner start is a crisp triangle"
    );
    assert_eq!(
        smooth
            .elements()
            .iter()
            .filter(|element| matches!(element, kurbo::PathEl::QuadTo(_, _)))
            .count(),
        3,
        "a smooth start rounds each wedge corner"
    );
    assert!(
        direction_marker_shape(center, center, POINT_CURVE_RADIUS, true).is_none(),
        "a zero-length direction falls back to the ordinary node"
    );
}

#[test]
fn coincident_metric_heights_have_one_intersection() {
    let metrics = crate::application::editor::session::Metrics {
        upm: 750.0,
        ascender: 750.0,
        descender: -250.0,
        x_height: 500.0,
        cap_height: 500.0,
    };
    assert_eq!(
        text_sort_metric_ys(&metrics),
        vec![-250.0, 0.0, 500.0, 750.0]
    );
}

mod wedge_check {
    #[test]
    fn a_wedge_fills_half_a_circle() {
        use kurbo::Shape as _;
        let center = kurbo::Point::new(0.0, 0.0);
        let radius = 10.0;
        let step = std::f64::consts::PI;
        let start = -std::f64::consts::FRAC_PI_2;
        let arc = kurbo::Arc::new(center, (radius, radius), start, step, 0.0);
        let mut wedge = kurbo::BezPath::new();
        wedge.move_to(center);
        wedge.line_to(center + kurbo::Vec2::new(start.cos(), start.sin()) * radius);
        wedge.extend(arc.append_iter(0.1));
        wedge.close_path();
        let half = std::f64::consts::PI * radius * radius / 2.0;
        assert!(
            (wedge.area().abs() - half).abs() < half * 0.02,
            "{}",
            wedge.area()
        );
    }
}
