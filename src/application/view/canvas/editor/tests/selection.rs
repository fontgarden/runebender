// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Tests of the Select tool.
//! They cover the context menu, components, anchors, the layer picture, nudges and the lasso.

use super::*;

/// A right click opens the menu as a layer, and the editor remembers
/// which layer so a second right click does not stack another one.
#[test]
fn right_click_opens_one_menu_layer() {
    let mut harness =
        TestHarness::create_with_size(default_property_set(), widget().prepare(), (600, 400));
    harness.mouse_move(Point::new(300.0, 200.0));
    harness.mouse_button_press(Some(PointerButton::Secondary));
    let first = harness.edit_root_widget(|root| root.widget.menu);
    assert!(first.is_some(), "the menu layer was created");

    harness.mouse_button_press(Some(PointerButton::Secondary));
    let second = harness.edit_root_widget(|root| root.widget.menu);
    assert_eq!(first, second, "a second right click did not stack a layer");
}

#[test]
fn right_click_selects_the_component_under_the_menu() {
    let mut font = norad::Font::new();
    font.default_layer_mut()
        .insert_glyph(projected_glyph(&session()));
    let mut composite = norad::Glyph::new("composite");
    composite.width = 500.0;
    composite.components.push(norad::Component::new(
        norad::Name::new("A").expect("valid glyph name"),
        norad::AffineTransform::default(),
        None,
    ));
    font.default_layer_mut().insert_glyph(composite);
    let mut editor = widget();
    editor.session = Session::new(&font, "composite").expect("composite resolves");
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let at = harness.edit_root_widget(|root| root.widget.glyph_affine() * Point::new(200.0, 350.0));
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Secondary));
    let menu = harness.edit_root_widget(|root| {
        assert!(root.widget.session.selected_component.is_some());
        root.widget.menu.expect("component menu opens")
    });
    let menu_origin = harness
        .get_widget_with_id(menu)
        .ctx()
        .to_window(Point::ORIGIN);
    harness.mouse_move(menu_origin + (20.0, 16.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    let mut entered = false;
    while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
        entered |= matches!(event, EditorEvent::EnterComponent(_));
    }
    assert!(
        entered,
        "the component menu opens the component in place first"
    );
}

#[test]
fn a_double_click_on_a_component_opens_it_in_place() {
    let mut harness = TestHarness::create_with_size(
        default_property_set(),
        composite_editor().prepare(),
        (600, 400),
    );
    let at = harness.edit_root_widget(|root| root.widget.glyph_affine() * Point::new(200.0, 350.0));
    double_click(&mut harness, at);
    let mut entered = false;
    while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
        entered |= matches!(event, EditorEvent::EnterComponent(_));
    }
    assert!(entered, "the double click opens the component");
}

#[test]
fn with_components_shown_a_press_on_a_component_point_opens_that_part() {
    let mut editor = composite_editor();
    editor.view.components = true;
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let (at, expected) = harness.edit_root_widget(|root| {
        let part = &root.widget.session.parts[0];
        let point = part.points[0];
        (root.widget.sort_affine() * point.point, point.id)
    });
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    let mut opened = None;
    while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
        if let EditorEvent::EnterPart { owner, point } = event {
            opened = Some((owner, point));
        }
    }
    let (owner, point) = opened.expect("the press opens the component's part");
    assert!(matches!(owner, PartOwner::Component(_)));
    assert_eq!(point, Some(expected));
    harness.edit_root_widget(|root| {
        assert!(matches!(root.widget.drag, Drag::Enter { .. }));
    });
}

#[test]
fn anchors_select_and_move_with_points() {
    let mut harness = TestHarness::create_with_size(
        default_property_set(),
        anchored_widget().prepare(),
        (600, 400),
    );
    let (anchor_at, point_at, anchor, point) = harness.edit_root_widget(|root| {
        let affine = root.widget.glyph_affine();
        let (anchor, position) = root.widget.session.anchor_points()[0];
        let point = root.widget.session.points()[0];
        (affine * position, affine * point.point, anchor, point.id)
    });

    // A click selects the anchor alone, and a click on a point lets go of it.
    harness.mouse_move(anchor_at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| {
        assert!(root.widget.session.selected_anchors.contains(&anchor));
        assert!(root.widget.session.selection.is_empty());
    });
    harness.mouse_move(point_at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| {
        assert!(root.widget.session.selected_anchors.is_empty());
        assert!(root.widget.session.selection.contains(&point));
        // Shift-click is the same toggle as for points; select both directly.
        root.widget.session.selected_anchors.insert(anchor);
    });

    // Dragging the anchor carries the point by the same amount.
    harness.mouse_move(anchor_at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(anchor_at + kurbo::Vec2::new(40.0, 0.0));
    harness.edit_root_widget(|root| {
        let moved_anchor = root.widget.session.anchor_points()[0].1;
        let moved_point = root.widget.session.points()[0].point;
        let anchor_dx = moved_anchor.x - 200.0;
        assert!(anchor_dx > 0.0, "the anchor follows the pointer");
        assert!((moved_point.x - anchor_dx).abs() <= 1.0, "{moved_point:?}");
    });
    harness.mouse_button_release(Some(PointerButton::Primary));

    // A marquee around the anchor alone selects it alone.
    harness.edit_root_widget(|root| root.widget.session.clear_selection());
    let (from, to) = harness.edit_root_widget(|root| {
        let at = root.widget.glyph_affine() * root.widget.session.anchor_points()[0].1;
        (
            at - kurbo::Vec2::new(15.0, 15.0),
            at + kurbo::Vec2::new(15.0, 15.0),
        )
    });
    harness.mouse_move(from);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(to);
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| {
        assert!(root.widget.session.selected_anchors.contains(&anchor));
        assert!(root.widget.session.selection.is_empty());
    });
}

/// A left click still edits rather than being eaten by menu handling.
#[test]
fn left_click_is_not_swallowed() {
    let mut harness =
        TestHarness::create_with_size(default_property_set(), widget().prepare(), (600, 400));
    harness.mouse_move(Point::new(300.0, 200.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
    let menu = harness.edit_root_widget(|root| root.widget.menu);
    assert!(menu.is_none(), "a left click does not open the menu");
}

#[test]
fn a_picture_is_selected_moved_and_resized_from_a_corner() {
    let mut editor = widget();
    let placed = runebender::font::LayerImage::new(
        std::path::PathBuf::from("page.png"),
        None,
        Affine::new([2.0, 0.0, 0.0, 2.0, 100.0, 0.0]),
    )
    .unwrap();
    assert!(editor.session.set_image(Some(placed)));
    editor.underlay.image = Some(PlacedImage {
        data: ImageData {
            data: Blob::new(Arc::new(vec![0; 100 * 100 * 4])),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: 100,
            height: 100,
        },
        to_glyph: Affine::new([2.0, 0.0, 0.0, -2.0, 100.0, 200.0]),
    });
    let original = projected_glyph(&editor.session).contours;
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    // the picture spans x 100..300 and y 0..200 in design space; press in the middle
    let (inside, delta) = harness.edit_root_widget(|root| {
        let affine = root.widget.glyph_affine();
        (
            affine * Point::new(200.0, 100.0),
            affine * Point::new(230.0, 140.0),
        )
    });
    harness.mouse_move(inside);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(delta);
    harness.mouse_button_release(Some(PointerButton::Primary));
    let corner = harness.edit_root_widget(|root| {
        let session = &root.widget.session;
        assert!(session.image_selected);
        let [_, _, _, _, x, y] = session.layer_image().unwrap().transform().as_coeffs();
        assert!(
            (x - 130.0).abs() < 1.0 && (y - 40.0).abs() < 1.0,
            "moved to {x}, {y}"
        );
        assert_eq!(projected_glyph(session).contours, original);
        root.widget.glyph_affine() * Point::new(330.0, 240.0)
    });
    // the top right corner, pulled out to twice the size about the lower left
    let pulled =
        harness.edit_root_widget(|root| root.widget.glyph_affine() * Point::new(530.0, 440.0));
    harness.mouse_move(corner);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(pulled);
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| {
        let [scale, _, _, _, x, y] = root
            .widget
            .session
            .layer_image()
            .unwrap()
            .transform()
            .as_coeffs();
        assert!((scale - 4.0).abs() < 0.05, "scaled to {scale}");
        assert!(
            (x - 130.0).abs() < 1.0 && (y - 40.0).abs() < 1.0,
            "anchor kept at {x}, {y}"
        );
    });
}

#[test]
fn a_locked_picture_can_be_selected_but_not_changed() {
    // a locked picture as it comes from a source, beside the glyph's own ink
    let mut font = norad::Font::new();
    let mut glyph = norad::Glyph::new("A");
    glyph.width = 500.0;
    glyph.image = Some(
        norad::Image::new(
            std::path::PathBuf::from("page.png"),
            None,
            norad::AffineTransform {
                x_scale: 2.0,
                xy_scale: 0.0,
                yx_scale: 0.0,
                y_scale: 2.0,
                x_offset: 450.0,
                y_offset: 0.0,
            },
        )
        .unwrap(),
    );
    glyph.lib.insert(
        "com.runebender.imageLocked".into(),
        plist::Value::Boolean(true),
    );
    font.default_layer_mut().insert_glyph(glyph);
    let mut editor = widget();
    editor.session = Session::new(&font, "A").unwrap();
    let placed = editor.session.layer_image().unwrap();
    assert!(editor.session.image_locked());
    editor.underlay.image = Some(PlacedImage {
        data: ImageData {
            data: Blob::new(Arc::new(vec![0; 100 * 100 * 4])),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: 100,
            height: 100,
        },
        to_glyph: Affine::new([2.0, 0.0, 0.0, -2.0, 450.0, 200.0]),
    });
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let (inside, beside, outside) = harness.edit_root_widget(|root| {
        let affine = root.widget.glyph_affine();
        (
            affine * Point::new(550.0, 100.0),
            affine * Point::new(610.0, 160.0),
            affine * Point::new(800.0, 100.0),
        )
    });
    // beside the glyph's own ink, which a click would select first: a click selects it
    harness.mouse_move(inside);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| assert!(root.widget.session.image_selected));
    // a drag over it does not move it, and Delete does not remove it
    harness.mouse_move(inside);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(beside);
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.mouse_move(inside);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Delete)));
    harness.edit_root_widget(|root| {
        let session = &mut root.widget.session;
        assert_eq!(
            session.layer_image().unwrap().transform(),
            placed.transform()
        );
        assert!(!session.remove_image() && !session.set_image_transform(Affine::IDENTITY));
    });
    // a click beside it deselects it
    harness.mouse_move(outside);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| assert!(!root.widget.session.image_selected));
}

#[test]
fn every_arrow_nudges_a_point_the_same_distance() {
    // A nudge smaller than the grid used to round back to the start in two directions and
    // jump double in the others.
    let step = crate::application::platform::config::nudge().step;
    for (key, delta) in [
        (NamedKey::ArrowRight, (step, 0.0)),
        (NamedKey::ArrowLeft, (-step, 0.0)),
        (NamedKey::ArrowUp, (0.0, step)),
        (NamedKey::ArrowDown, (0.0, -step)),
    ] {
        let mut editor = widget();
        let (id, before) = {
            let point = editor.session.points()[0];
            (point.id, point.point)
        };
        editor.session.selection = HashSet::from([id]);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        harness.focus_on(Some(harness.root_id()));
        harness.process_text_event(TextEvent::key_down(Key::Named(key)));
        harness.edit_root_widget(|root| {
            let after = root
                .widget
                .session
                .points()
                .into_iter()
                .find(|point| point.id == id)
                .unwrap()
                .point;
            assert_eq!(after - before, kurbo::Vec2::from(delta), "{key:?}");
        });
    }
}

#[test]
fn a_double_click_switches_a_point_between_corner_and_smooth() {
    let editor = widget();
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let (id, at, smooth) = harness.edit_root_widget(|root| {
        let (id, at, _, smooth, _) = root.widget.screen_points()[0];
        (id, at, smooth)
    });
    let state = PointerState {
        position: PhysicalPosition::new(at.x, at.y),
        count: 2,
        ..PointerState::default()
    };
    harness.process_pointer_event(PointerEvent::Down(PointerButtonEvent {
        pointer: PRIMARY_MOUSE,
        button: Some(PointerButton::Primary),
        state,
    }));
    harness.edit_root_widget(|root| {
        let point = root
            .widget
            .session
            .points()
            .into_iter()
            .find(|point| point.id == id)
            .unwrap();
        assert_eq!(point.smooth, !smooth, "the double click switched it");
    });
}

#[test]
fn an_option_nudge_moves_a_smooth_point_without_its_handles() {
    use masonry::core::keyboard::{Code, KeyboardEvent, Modifiers};
    let nudge = crate::application::platform::config::nudge();
    let (step, shift_step, command_step) = (nudge.step, nudge.shift, nudge.command);
    let mut editor = widget();
    editor.session = smooth_curve_session();
    let smooth = editor.session.points()[3].id;
    editor.session.selection = HashSet::from([smooth]);
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    harness.focus_on(Some(harness.root_id()));
    let arrow_up = |modifiers| {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Named(NamedKey::ArrowUp),
            code: Code::Unidentified,
            modifiers,
            ..KeyboardEvent::default()
        })
    };
    let positions = |harness: &mut TestHarness<EditorWidget>| {
        harness.edit_root_widget(|root| {
            root.widget.session.points()[2..=4]
                .iter()
                .map(|point| point.point)
                .collect::<Vec<_>>()
        })
    };
    // Without a workspace, each nudge stages from the same committed
    // state, so every press below starts from the original outline.
    for (modifiers, rise, handles_follow) in [
        (Modifiers::ALT, step, false),
        (Modifiers::ALT | Modifiers::SHIFT, shift_step, false),
        (Modifiers::empty(), step, true),
        (Modifiers::META | Modifiers::SHIFT, command_step, true),
    ] {
        harness.process_text_event(arrow_up(modifiers));
        let handle_y = if handles_follow { 300.0 + rise } else { 300.0 };
        assert_eq!(
            positions(&mut harness),
            [
                Point::new(200.0, handle_y),
                Point::new(300.0, 300.0 + rise),
                Point::new(400.0, handle_y),
            ],
            "{modifiers:?}"
        );
    }
}

#[test]
fn an_option_drag_moves_a_smooth_point_without_its_handles() {
    let mut session = smooth_curve_session();
    let smooth = session.points()[3].id;
    session.selection.insert(smooth);
    let positions = |session: &Session| {
        session.points()[2..=4]
            .iter()
            .map(|point| point.point)
            .collect::<Vec<_>>()
    };
    session.begin_point_drag();
    assert!(session.drag_points_to((0.0, 20.0), true));
    assert_eq!(
        positions(&session),
        [
            Point::new(200.0, 300.0),
            Point::new(300.0, 320.0),
            Point::new(400.0, 300.0),
        ],
        "Option leaves both handles in place"
    );
    assert!(session.drag_points_to((0.0, 20.0), false));
    assert_eq!(
        positions(&session),
        [
            Point::new(200.0, 320.0),
            Point::new(300.0, 320.0),
            Point::new(400.0, 320.0),
        ],
        "without Option the handles follow"
    );
    assert!(session.drag_points_to((0.0, 20.0), true));
    assert_eq!(
        positions(&session)[0],
        Point::new(200.0, 300.0),
        "pressing Option mid-drag returns the handles"
    );
    session.end_point_drag();
}

#[test]
fn making_a_point_smooth_lines_up_its_handles() {
    use norad::PointType::{Curve, Line, OffCurve};
    let contour = [
        (0.0, 0.0, Line),
        (100.0, 100.0, OffCurve),
        (150.0, 100.0, OffCurve),
        (200.0, 0.0, Curve),
        (250.0, 100.0, OffCurve),
        (300.0, 100.0, OffCurve),
        (400.0, 0.0, Curve),
        (200.0, -200.0, Line),
    ];
    let id_at = |session: &Session, x: f64, y: f64| {
        session
            .points()
            .iter()
            .find(|point| point.point == Point::new(x, y))
            .map(|point| point.id)
            .expect("point exists")
    };
    let at = |session: &Session, index: usize| session.points()[index].point;

    // Handles on both sides turn onto the line that bisects their directions.
    let mut session = session_with(&contour);
    let corner = id_at(&session, 200.0, 0.0);
    assert!(session.toggle_smooth(corner));
    let (before, after) = (at(&session, 2), at(&session, 4));
    assert_eq!((before.y, after.y), (0.0, 0.0), "both handles lie on y = 0");
    assert!(before.x < 200.0 && after.x > 200.0);
    assert!((Point::new(200.0, 0.0).distance(before) - 50_f64.hypot(100.0)).abs() < 1.0);
    assert!((Point::new(200.0, 0.0).distance(after) - 50_f64.hypot(100.0)).abs() < 1.0);

    // Beside a straight segment, the handle continues that segment.
    let mut session = session_with(&contour);
    let joint = id_at(&session, 400.0, 0.0);
    assert!(session.toggle_smooth(joint));
    let handle = at(&session, 5);
    assert!(
        (handle.y - (handle.x - 400.0)).abs() <= 1.0,
        "the handle lies on the line through (200, -200) and (400, 0): {handle:?}"
    );
    assert!(handle.x > 400.0);
}

#[test]
fn tab_and_shift_tab_walk_the_selection_around_the_contour() {
    use masonry::core::keyboard::{Code, KeyboardEvent, Modifiers};
    use norad::PointType::{Curve, Line, OffCurve};
    let mut editor = widget();
    editor.session = session_with(&[
        (0.0, 0.0, Line),
        (0.0, 200.0, OffCurve),
        (100.0, 300.0, OffCurve),
        (300.0, 300.0, Curve),
        (300.0, 0.0, Line),
    ]);
    let ids: Vec<_> = editor
        .session
        .points()
        .iter()
        .map(|point| point.id)
        .collect();
    editor.session.selection = HashSet::from([ids[0]]);
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    harness.focus_on(Some(harness.root_id()));
    let tab = |modifiers| {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Named(NamedKey::Tab),
            code: Code::Tab,
            modifiers,
            ..KeyboardEvent::default()
        })
    };
    let mut press = |modifiers| {
        harness.process_text_event(tab(modifiers));
        harness.edit_root_widget(|root| root.widget.session.selection.clone())
    };
    // Forward follows the contour, handles included.
    assert_eq!(press(Modifiers::empty()), HashSet::from([ids[1]]));
    assert_eq!(press(Modifiers::empty()), HashSet::from([ids[2]]));
    // Shift-Tab goes back, and wraps past the start of a closed contour.
    assert_eq!(press(Modifiers::SHIFT), HashSet::from([ids[1]]));
    assert_eq!(press(Modifiers::SHIFT), HashSet::from([ids[0]]));
    assert_eq!(press(Modifiers::SHIFT), HashSet::from([ids[4]]));
    assert_eq!(press(Modifiers::empty()), HashSet::from([ids[0]]));
    assert_eq!(harness.focused_widget_id(), Some(harness.root_id()));
}

#[test]
fn lasso_selects_enclosed_point_without_editing_outline() {
    let mut editor = widget();
    editor.tool = Tool::Lasso;
    let original = projected_glyph(&editor.session);
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let (id, point) = harness.edit_root_widget(|root| {
        let (id, point, _, _, _) = root.widget.screen_points()[0];
        (id, point)
    });
    for (index, (x, y)) in [(-14.0, -14.0), (14.0, -14.0), (14.0, 14.0), (-14.0, 14.0)]
        .into_iter()
        .enumerate()
    {
        harness.mouse_move(point + kurbo::Vec2::new(x, y));
        if index == 0 {
            harness.mouse_button_press(Some(PointerButton::Primary));
        }
    }
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| {
        assert_eq!(root.widget.session.selection, HashSet::from([id]));
        assert_eq!(projected_glyph(&root.widget.session), original);
    });
}
