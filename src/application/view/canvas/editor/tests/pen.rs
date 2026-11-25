// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Tests of the Pen tool: point insertion, handle creation and cancelled gestures.

use super::*;

#[test]
fn a_pen_click_on_a_line_adds_a_corner_point_to_it() {
    let mut editor = widget();
    editor.tool = Tool::Pen;
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let at = harness.edit_root_widget(|root| root.widget.glyph_affine() * Point::new(200.0, 0.0));
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| {
        let session = &root.widget.session;
        let points = session.points();
        assert_eq!(
            points.len(),
            5,
            "one point joined the square, no new contour"
        );
        assert!(!session.pen_is_active());
        let added = points
            .iter()
            .find(|point| point.point == Point::new(200.0, 0.0))
            .expect("the point sits on the bottom edge");
        assert!(added.on_curve && !added.smooth);
        assert_eq!(session.selection, HashSet::from([added.id]));
    });
}

#[test]
fn a_pen_hover_on_a_line_previews_the_new_point() {
    let mut editor = widget();
    editor.tool = Tool::Pen;
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let (on_edge, away) = harness.edit_root_widget(|root| {
        let affine = root.widget.glyph_affine();
        (
            affine * Point::new(200.0, 0.0),
            affine * Point::new(200.0, 350.0),
        )
    });
    harness.mouse_move(on_edge);
    harness.edit_root_widget(|root| {
        assert_eq!(
            root.widget.insert_preview,
            Some((Point::new(200.0, 0.0), false))
        );
    });
    harness.mouse_move(away);
    harness.edit_root_widget(|root| assert_eq!(root.widget.insert_preview, None));

    // While a new contour is being drawn, the pen only draws.
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.mouse_move(on_edge);
    harness.edit_root_widget(|root| {
        assert!(root.widget.session.pen_is_active());
        assert_eq!(root.widget.insert_preview, None);
    });
}

#[test]
fn a_pen_click_on_a_curve_adds_a_smooth_point_with_two_handles() {
    let mut font = norad::Font::new();
    let mut glyph = norad::Glyph::new("A");
    glyph.width = 500.0;
    let mut contour = norad::Contour::default();
    for (x, y, typ) in [
        (0.0, 0.0, norad::PointType::Line),
        (0.0, 200.0, norad::PointType::OffCurve),
        (100.0, 300.0, norad::PointType::OffCurve),
        (300.0, 300.0, norad::PointType::Curve),
        (300.0, 0.0, norad::PointType::Line),
    ] {
        contour
            .points
            .push(norad::ContourPoint::new(x, y, typ, false, None, None));
    }
    glyph.contours.push(contour);
    font.default_layer_mut().insert_glyph(glyph);
    let mut session = Session::new(&font, "A").expect("the glyph is there");
    let curve = kurbo::CubicBez::new((0.0, 0.0), (0.0, 200.0), (100.0, 300.0), (300.0, 300.0));
    let before = kurbo::ParamCurve::eval(&curve, 0.5);
    assert!(session.insert_point_at(before, 4.0));
    let points = session.points();
    assert_eq!(
        points.len(),
        8,
        "a smooth point and a new handle on each side"
    );
    let index = points
        .iter()
        .position(|point| session.selection.contains(&point.id))
        .expect("the new point is selected");
    let added = &points[index];
    assert!(added.on_curve && added.smooth);
    assert!(
        added.point.distance(before) <= 1.0,
        "the curve keeps its shape"
    );
    assert!(!points[index - 1].on_curve && !points[index + 1].on_curve);
    assert!(
        !session.insert_point_at(Point::new(150.0, 100.0), 4.0),
        "empty space is a miss"
    );
}

#[test]
fn option_click_gives_lines_and_single_control_curves_two_handles() {
    use norad::PointType::{Curve, Line, OffCurve, QCurve};
    let contour = [
        (0.0, 0.0, Line),
        (0.0, 300.0, Line),
        (150.0, 450.0, OffCurve),
        (300.0, 300.0, QCurve),
        (300.0, 0.0, Line),
    ];
    let handles = |session: &Session| {
        let mut found: Vec<_> = session
            .points()
            .iter()
            .filter(|point| session.selection.contains(&point.id))
            .map(|point| (point.point.x, point.point.y, point.on_curve))
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        found
    };

    // A straight segment gets handles at its thirds.
    let mut session = session_with(&contour);
    assert!(session.add_segment_handles_at(Point::new(0.0, 150.0), 4.0));
    assert_eq!(
        handles(&session),
        [(0.0, 100.0, false), (0.0, 200.0, false)]
    );
    assert_eq!(session.points().len(), 7);

    // A one-control curve keeps its shape: its midpoint stays at (150, 375).
    let mut session = session_with(&contour);
    assert!(session.add_segment_handles_at(Point::new(150.0, 375.0), 4.0));
    assert_eq!(
        handles(&session),
        [(100.0, 400.0, false), (200.0, 400.0, false)]
    );
    let cubic = kurbo::CubicBez::new((0.0, 300.0), (100.0, 400.0), (200.0, 400.0), (300.0, 300.0));
    assert_eq!(
        kurbo::ParamCurve::eval(&cubic, 0.5),
        Point::new(150.0, 375.0)
    );
    assert_eq!(session.points().len(), 6);

    // A segment that already has two handles is left alone.
    let mut session = session_with(&[
        (0.0, 300.0, Line),
        (100.0, 400.0, OffCurve),
        (200.0, 400.0, OffCurve),
        (300.0, 300.0, Curve),
        (300.0, 0.0, Line),
    ]);
    assert!(!session.add_segment_handles_at(Point::new(150.0, 375.0), 4.0));
}

#[test]
fn pointer_cancel_restores_pen_preview_and_preserves_canonical_redo() {
    let path = std::env::temp_dir().join(format!(
        "runebender-editor-pen-cancel-{}-{}.ufo",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the system clock is after the Unix epoch")
            .as_nanos(),
    ));
    let mut font = norad::Font::new();
    font.default_layer_mut()
        .insert_glyph(projected_glyph(&session()));
    font.save(&path).expect("the fixture saves");
    let mut workspace = Workspace::open(&path).expect("the fixture opens");
    workspace.open_glyph(0);

    let mut pen_session = (*workspace.session).clone();
    pen_session.pen_corner(100.0, 100.0);
    assert_eq!(
        workspace.sync_session_from(&mut pen_session),
        SessionSyncOutcome::Changed
    );
    workspace.finish_open_glyph_refresh();
    let selected = workspace.session.point_id_at(0, 0).unwrap();
    Arc::make_mut(&mut workspace.session)
        .selection
        .insert(selected);
    workspace.apply_op(|session| session.nudge(20.0, 0.0));
    workspace.undo_open_glyph(false);

    workspace.modified = false;
    let baseline = projected_glyph(&workspace.session);
    let selection = workspace.session.selection.clone();
    let revision = workspace.font.project.document_revision();
    let undo = workspace.metadata_undo.len();
    let redo = workspace.metadata_redo.len();
    let address = workspace.font.active_layer_address("A").unwrap();
    assert!(workspace.font.project.can_replay_document_layer_history(
        &address,
        runebender::font::history::HistoryDirection::Redo,
    ));

    let mut editor = widget();
    editor.session = (*workspace.session).clone();
    editor.tool = Tool::Pen;
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let (from, to) = harness.edit_root_widget(|root| {
        let affine = root.widget.glyph_affine();
        (
            affine * Point::new(220.0, 180.0),
            affine * Point::new(280.0, 240.0),
        )
    });
    harness.mouse_move(from);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(to);
    harness.process_pointer_event(PointerEvent::Cancel(PRIMARY_MOUSE));

    let mut events = Vec::new();
    while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
        events.push(event);
    }
    assert!(
        events
            .iter()
            .all(|event| matches!(event, EditorEvent::Selection)),
        "a cancelled pen preview never emits Edited"
    );
    for event in events {
        harness.edit_root_widget(|root| {
            dispatch_editor_event(&mut workspace, &mut root.widget.session, event, |_, _| {});
        });
    }

    harness.edit_root_widget(|root| {
        assert_eq!(projected_glyph(&root.widget.session), baseline);
        assert_eq!(root.widget.session.selection, selection);
        assert!(root.widget.session.pen_is_active());
        assert!(root.widget.session.pending_canonical.is_none());
    });
    assert!(!workspace.modified);
    assert_eq!(workspace.font.project.document_revision(), revision);
    assert_eq!(workspace.metadata_undo.len(), undo);
    assert_eq!(workspace.metadata_redo.len(), redo);
    assert!(workspace.font.project.can_replay_document_layer_history(
        &address,
        runebender::font::history::HistoryDirection::Redo,
    ));

    let next =
        harness.edit_root_widget(|root| root.widget.glyph_affine() * Point::new(320.0, 180.0));
    harness.mouse_move(next);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    let mut edited = false;
    while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
        harness.edit_root_widget(|root| {
            dispatch_editor_event(
                &mut workspace,
                &mut root.widget.session,
                event,
                |app, event| {
                    if let EditorEvent::Edited = event {
                        edited = true;
                        app.finish_open_glyph_refresh();
                    }
                },
            );
        });
    }
    assert!(edited, "the retained pen contour accepts the next point");
    assert_eq!(
        projected_glyph(&workspace.session)
            .contours
            .last()
            .unwrap()
            .points
            .len(),
        2
    );

    std::fs::remove_dir_all(path).expect("the fixture is removed");
}

#[test]
fn pointer_cancel_never_applies_shape_or_knife_gestures() {
    for tool in [Tool::Rect, Tool::Ellipse, Tool::Knife] {
        let mut editor = widget();
        editor.tool = tool;
        let selected = editor.session.point_id_at(0, 0).unwrap();
        editor.session.selection.insert(selected);
        let baseline = projected_glyph(&editor.session);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        harness.mouse_move(Point::new(250.0, 150.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_move(Point::new(350.0, 250.0));
        harness.process_pointer_event(PointerEvent::Cancel(PRIMARY_MOUSE));

        harness.edit_root_widget(|root| {
            assert_eq!(projected_glyph(&root.widget.session), baseline);
            assert_eq!(root.widget.session.selection, HashSet::from([selected]));
            assert!(root.widget.session.pending_canonical.is_none());
        });
        while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
            assert!(
                matches!(event, EditorEvent::Selection),
                "a cancelled {tool:?} gesture never emits Edited"
            );
        }
    }
}

#[test]
fn quadratic_handle_bends_a_closing_line_without_touching_the_other_segments() {
    use norad::PointType::Line;
    use runebender::font::LayerPointType;
    let original = session_with(&[(0.0, 0.0, Line), (300.0, 0.0, Line), (301.0, 301.0, Line)]);
    let mut font = norad::Font::new();
    font.default_layer_mut()
        .insert_glyph(projected_glyph(&original));
    let path = std::env::temp_dir().join(format!(
        "runebender-quadratic-handle-{}.ufo",
        std::process::id()
    ));
    font.save(&path).unwrap();
    let mut workspace = Workspace::open(&path).unwrap();
    workspace.open_glyph(0);
    let mut session = (*workspace.session).clone();
    assert!(session.add_quadratic_handle_at(Point::new(150.5, 150.5), 2.0));
    assert_eq!(
        workspace.sync_session_from(&mut session),
        SessionSyncOutcome::Changed
    );
    let points = session.points();
    let handle = points
        .iter()
        .find(|point| session.selection.contains(&point.id))
        .unwrap();
    assert!(!handle.on_curve);
    assert_eq!(handle.point, Point::new(150.5, 150.5));
    assert!(session.nudge(20.0, 0.0));
    let types: Vec<_> = session
        .current_layer()
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|point| point.point_type())
        .collect();
    assert_eq!(
        types,
        [
            LayerPointType::QCurve,
            LayerPointType::Line,
            LayerPointType::Line,
            LayerPointType::OffCurve
        ]
    );
    assert_eq!(
        workspace.sync_session_from(&mut session),
        SessionSyncOutcome::Changed
    );
    workspace.font.project.save().unwrap();
    let saved = norad::Font::load(&path).unwrap();
    let nodes = &saved.default_layer().get_glyph("A").unwrap().contours[0].points;
    assert!(
        nodes
            .iter()
            .any(|node| node.typ == norad::PointType::QCurve)
    );
    assert!(
        nodes
            .iter()
            .any(|node| node.typ == norad::PointType::OffCurve && node.x > 150.5)
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn quadratic_handle_chain_accepts_a_pen_split_at_an_implied_segment() {
    use norad::PointType::{Line, OffCurve, QCurve};
    let mut session = session_with(&[
        (0.0, 0.0, Line),
        (0.0, 100.0, OffCurve),
        (100.0, 100.0, OffCurve),
        (100.0, 0.0, QCurve),
    ]);
    let midpoint = Point::new(12.5, 75.0);
    assert!(!session.add_quadratic_handle_at(midpoint, 2.0));
    assert!(session.insert_point_at(midpoint, 2.0));
    let segments =
        runebender::outline::segment_ops::ordinary_layer_segments(session.current_layer().unwrap());
    assert_eq!(
        segments
            .iter()
            .filter(|s| matches!(s.seg, kurbo::PathSeg::Quad(_)))
            .count(),
        3
    );
    assert!(session.points().iter().any(|p| p.on_curve
        && session.selection.contains(&p.id)
        && p.point.distance(midpoint) <= 1.0));
}

#[test]
fn quadratic_handle_ring_can_be_split_without_an_explicit_start_point() {
    use norad::PointType::OffCurve;
    let mut session = session_with(&[
        (0.0, 0.0, OffCurve),
        (200.0, 0.0, OffCurve),
        (200.0, 200.0, OffCurve),
        (0.0, 200.0, OffCurve),
    ]);
    assert!(session.insert_point_at(Point::new(25.0, 25.0), 1.0));
    let segments =
        runebender::outline::segment_ops::ordinary_layer_segments(session.current_layer().unwrap());
    assert_eq!(segments.len(), 5);
    assert!(
        segments
            .iter()
            .all(|hit| matches!(hit.seg, kurbo::PathSeg::Quad(_)))
    );
}

#[test]
fn shift_option_click_selects_the_new_quadratic_handle() {
    use masonry::core::keyboard::Modifiers;
    let mut editor = widget();
    editor.tool = Tool::Select;
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let at = harness.edit_root_widget(|root| root.widget.glyph_affine() * Point::new(200.0, 0.0));
    harness.mouse_move(at);
    harness.process_pointer_event(PointerEvent::Down(PointerButtonEvent {
        pointer: PRIMARY_MOUSE,
        button: Some(PointerButton::Primary),
        state: PointerState {
            position: PhysicalPosition::new(at.x, at.y),
            modifiers: Modifiers::ALT | Modifiers::SHIFT,
            ..Default::default()
        },
    }));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| {
        let session = &root.widget.session;
        assert_eq!(session.selection.len(), 1);
        assert!(session.points().iter().any(|p| !p.on_curve
            && session.selection.contains(&p.id)
            && p.point == Point::new(200.0, 0.0)));
    });
}

#[test]
fn option_drag_with_the_pen_draws_one_quadratic_to_the_pressed_endpoint() {
    use masonry::core::keyboard::Modifiers;
    use norad::PointType::{Move, OffCurve, QCurve};
    let mut workspace = Workspace::untitled().unwrap();
    workspace.open_glyph(workspace.font.index_of("A").unwrap());
    let mut session = (*workspace.session).clone();
    session.pen_corner(20.0, 20.0);
    assert_eq!(
        workspace.sync_session_from(&mut session),
        SessionSyncOutcome::Changed
    );
    let mut editor = widget();
    session.viewport = editor.session.viewport.clone();
    editor.session = session;
    editor.tool = Tool::Pen;
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let affine = harness.edit_root_widget(|root| root.widget.glyph_affine());
    let endpoint = affine * Point::new(200.0, 100.0);
    harness.mouse_move(endpoint);
    harness.process_pointer_event(PointerEvent::Down(PointerButtonEvent {
        pointer: PRIMARY_MOUSE,
        button: Some(PointerButton::Primary),
        state: PointerState {
            position: PhysicalPosition::new(endpoint.x, endpoint.y),
            modifiers: Modifiers::ALT,
            ..Default::default()
        },
    }));
    harness.mouse_move(affine * Point::new(80.0, 160.0));
    harness.mouse_move(affine * Point::new(90.0, 180.0));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| {
        let glyph = projected_glyph(&root.widget.session);
        let points = &glyph.contours.last().unwrap().points;
        let expected = [
            (Move, (20.0, 20.0)),
            (OffCurve, (90.0, 180.0)),
            (QCurve, (200.0, 100.0)),
        ];
        assert_eq!(points.len(), expected.len());
        for (node, (kind, position)) in points.iter().zip(expected) {
            assert_eq!(node.typ, kind);
            assert!(Point::new(node.x, node.y).distance(position.into()) < 1e-8);
        }
    });
}
