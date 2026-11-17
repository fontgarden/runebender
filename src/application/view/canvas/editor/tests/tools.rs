// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Tests of the label, metaball, brush, measure, hand and interpolation tools.

use super::*;

#[test]
fn a_label_loop_makes_a_sample() {
    // The harness holds no document, so each edit stages from the same base; the
    // polygon gestures are covered by the session's own tests.
    let mut editor = widget();
    editor.tool = Tool::Label;
    let original = projected_glyph(&editor.session);
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let affine = harness.edit_root_widget(|root| root.widget.glyph_affine());
    let screen = |x: f64, y: f64| affine * Point::new(x, y);

    // with no word open, the caption says what to do
    harness.mouse_move(screen(200.0, 350.0));
    harness.edit_root_widget(|root| {
        assert!(root.widget.label_hover.is_none());
        assert!(
            root.widget
                .label_caption()
                .unwrap()
                .starts_with("Click a word")
        );
    });

    // a loop around the square, with no sample selected: a new sample
    harness.mouse_move(screen(660.0, 350.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
    for step in 1..=16 {
        let turn = f64::from(step) * std::f64::consts::TAU / 16.0;
        harness.mouse_move(screen(
            200.0 + 460.0 * turn.cos(),
            350.0 + 520.0 * turn.sin(),
        ));
    }
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| {
        let session = &root.widget.session;
        assert_eq!(session.neural_item().samples.len(), 1);
        assert_eq!(session.label.sample, Some(0), "the new sample is selected");
        assert!(
            session.neural_item().samples[0].boundary.len() <= 16,
            "the loop became a polygon with few corners"
        );
        assert_eq!(
            projected_glyph(session).contours,
            original.contours,
            "labeling never edits the outline"
        );
    });
}

#[test]
fn a_click_on_another_word_opens_it_and_a_click_outside_every_word_closes_it() {
    let path = std::path::Path::new("assets/font-sources/neural-fonts/NastaliqDemo.nufo");
    let mut workspace = Workspace::open(path).expect("the demo source opens");
    workspace.open_glyph(workspace.font.index_of("ba-basic").unwrap());
    workspace.select_tool(Tool::Label);
    let item = workspace.session.neural_item();
    assert!(item.samples.len() >= 2, "ba-basic has two words or more");
    Arc::make_mut(&mut workspace.session).select_sample(Some(0));
    let before = item.samples[0].clone();
    let mut editor = widget();
    editor.session = (*workspace.session).clone();
    editor.tool = Tool::Label;
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (1200, 700));
    let dispatch = |harness: &mut TestHarness<EditorWidget>, workspace: &mut Workspace| {
        while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
            harness.edit_root_widget(|root| {
                dispatch_editor_event(workspace, &mut root.widget.session, event, |_, _| {});
            });
        }
    };
    let affine = harness.edit_root_widget(|root| root.widget.glyph_affine());
    let click = |harness: &mut TestHarness<EditorWidget>, at: Point| {
        harness.mouse_move(affine * at);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
    };

    // The middle of the second word's loop: that word opens, the first is untouched.
    let second = item.samples[1].boundary_path().bounding_box().center();
    click(&mut harness, second);
    dispatch(&mut harness, &mut workspace);
    assert_eq!(workspace.session.label.sample, Some(1));
    assert_eq!(workspace.session.neural_item().samples[0], before);

    // Far from every word: no word is open.
    let all = item
        .samples
        .iter()
        .map(|sample| sample.boundary_path().bounding_box())
        .reduce(|a, b| a.union(b))
        .unwrap();
    click(&mut harness, Point::new(all.x0 - 2000.0, all.center().y));
    dispatch(&mut harness, &mut workspace);
    assert_eq!(workspace.session.label.sample, None);
}

#[test]
fn label_clicks_through_the_workspace_keep_the_sample_and_make_only_polygons() {
    use crate::application::editor::tools::label::LabelSelection;
    let path = std::path::Path::new("assets/font-sources/neural-fonts/NastaliqDemo.nufo");
    let mut workspace = Workspace::open(path).expect("the demo source opens");
    workspace.open_glyph(workspace.font.index_of("bismillah-001").unwrap());
    workspace.select_tool(Tool::Label);
    // the panel's row click
    Arc::make_mut(&mut workspace.session).select_sample(Some(0));
    let mut editor = widget();
    editor.session = (*workspace.session).clone();
    editor.tool = Tool::Label;
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (1200, 700));
    let dispatch = |harness: &mut TestHarness<EditorWidget>, workspace: &mut Workspace| {
        while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
            harness.edit_root_widget(|root| {
                dispatch_editor_event(workspace, &mut root.widget.session, event, |app, event| {
                    match event {
                        EditorEvent::Edited => app.finish_open_glyph_refresh(),
                        EditorEvent::Selection(count) => app.selected_points = count,
                        _ => {}
                    }
                });
            });
        }
    };
    let affine = harness.edit_root_widget(|root| root.widget.glyph_affine());
    let screen = |x: f64, y: f64| affine * Point::new(x, y);

    // a click on the ب tooth keeps the word open and changes nothing
    harness.mouse_move(screen(1100.0, 450.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    dispatch(&mut harness, &mut workspace);
    assert_eq!(workspace.session.label.error, None);
    assert_eq!(workspace.session.label.sample, Some(0));
    assert!(
        workspace.session.selected_sample().is_some(),
        "the sample stays readable"
    );
    assert!(workspace.session.active_letter().is_some());

    // a drag across the long stroke is not a loop: a hint, and no cut
    let before = workspace.session.neural_item();
    harness.mouse_move(screen(300.0, 900.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
    for y in [700.0, 500.0, 300.0, 100.0, -100.0, -300.0] {
        harness.mouse_move(screen(300.0, y));
    }
    harness.mouse_button_release(Some(PointerButton::Primary));
    dispatch(&mut harness, &mut workspace);
    assert!(workspace.session.label.error.is_some());
    assert_eq!(workspace.session.neural_item(), before);
    assert!(workspace.session.selected_sample().is_some());

    // a click inside one of the file's lasso polygons picks it up
    harness.mouse_move(screen(-1280.0, 300.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    dispatch(&mut harness, &mut workspace);
    assert!(matches!(
        workspace.session.label.selected,
        Some(LabelSelection::Region(_))
    ));
    assert_eq!(workspace.session.label.error, None);

    // a short drag that stays on the ink is not a loop: no change
    harness.mouse_move(screen(-1480.0, 400.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
    for x in [-1460.0, -1440.0, -1420.0] {
        harness.mouse_move(screen(x, 400.0));
    }
    harness.mouse_button_release(Some(PointerButton::Primary));
    dispatch(&mut harness, &mut workspace);
    assert!(workspace.session.neural_item().samples[0].cuts.is_empty());
}

#[test]
fn metaball_pointer_gesture_adds_live_source_with_one_undo_step() {
    let mut editor = widget();
    editor.tool = Tool::Metaball;
    let original = projected_glyph(&editor.session);
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    harness.mouse_move(Point::new(300.0, 200.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(Point::new(330.0, 220.0));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| {
        let session = &root.widget.session;
        let source = session.metaball_data().unwrap();
        assert_eq!(source.groups.len(), 1);
        assert_eq!(source.groups[0].balls.len(), 1);
        assert_eq!(
            projected_glyph(session).contours,
            original.contours,
            "live metaballs do not create font points"
        );
        assert!(session.pending_canonical.is_some());
        assert!(!session.gesture_in_progress());
        assert!(!session.metaball_preview.elements().is_empty());
    });
}

#[test]
fn select_tool_moves_an_existing_metaball_without_creating_one() {
    let mut editor = widget();
    editor.tool = Tool::Metaball;
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));

    let center = Point::new(300.0, 200.0);
    harness.mouse_move(center);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    let (id, original) = harness.edit_root_widget(|root| {
        root.widget.tool = Tool::Select;
        root.widget.session.metaballs.selected.clear();
        let source = root.widget.session.metaball_data().unwrap();
        let ball = &source.groups[0].balls[0];
        ((source.groups[0].id, ball.id), Point::new(ball.x, ball.y))
    });

    harness.mouse_move(center);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(Point::new(330.0, 220.0));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| {
        let source = root.widget.session.metaball_data().unwrap();
        assert_eq!(source.groups.len(), 1);
        assert_eq!(source.groups[0].balls.len(), 1);
        assert!(root.widget.session.metaballs.selected.contains(&id));
        let ball = &source.groups[0].balls[0];
        assert_ne!(Point::new(ball.x, ball.y), original);
    });

    harness.mouse_move(Point::new(20.0, 20.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| {
        let source = root.widget.session.metaball_data().unwrap();
        assert_eq!(source.groups.len(), 1);
        assert_eq!(source.groups[0].balls.len(), 1);
        assert!(root.widget.session.metaballs.selected.is_empty());
    });
}

#[test]
fn brush_pointer_event_changes_only_session_ink() {
    let path = std::env::temp_dir().join(format!(
        "runebender-brush-event-{}-{}.ufo",
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
    let original = projected_glyph(&workspace.session);
    let revision = workspace.font.project.document_revision();
    let undo_depth = workspace.metadata_undo.len();
    let mut editor = widget();
    editor.session = (*workspace.session).clone();
    editor.tool = Tool::Sketch;
    editor.sketch = workspace.sketch.clone();
    editor.sketch_source = workspace.font.active();
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
    harness.mouse_button_release(Some(PointerButton::Primary));
    let mut sketch_events = 0;
    while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
        harness.edit_root_widget(|root| {
            dispatch_editor_event(
                &mut workspace,
                &mut root.widget.session,
                event,
                |app, event| {
                    assert!(matches!(event, EditorEvent::SketchChanged));
                    sketch_events += 1;
                    app.note = "Sketch ink changed".into();
                },
            );
        });
    }
    assert_eq!(sketch_events, 1);
    assert!(workspace.sketch.lock().unwrap().has_ink());
    assert_eq!(projected_glyph(&workspace.session), original);
    assert_eq!(workspace.font.project.document_revision(), revision);
    assert_eq!(workspace.metadata_undo.len(), undo_depth);
    assert!(!workspace.modified);
    std::fs::remove_dir_all(path).expect("the fixture is removed");
}

fn check_metaball_drag_preview(cancel: bool) {
    let path = std::env::temp_dir().join(format!(
        "runebender-metaball-live-proof-{}-{}.ufo",
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
    let original = workspace.session.outline_arc();
    let revision = workspace.font.project.document_revision();
    let undo_depth = workspace.metadata_undo.len();
    let redo_depth = workspace.metadata_redo.len();
    let mut editor = widget();
    editor.session = (*workspace.session).clone();
    editor.tool = Tool::Metaball;
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let dispatch = |harness: &mut TestHarness<EditorWidget>, workspace: &mut Workspace| {
        let mut edited = 0;
        while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
            harness.edit_root_widget(|root| {
                dispatch_editor_event(workspace, &mut root.widget.session, event, |app, event| {
                    match event {
                        EditorEvent::Edited => {
                            edited += 1;
                            app.finish_open_glyph_refresh();
                        }
                        EditorEvent::Selection(count) => app.selected_points = count,
                        _ => panic!("unexpected metaball gesture event"),
                    }
                });
            });
        }
        edited
    };

    harness.mouse_move(Point::new(300.0, 200.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
    assert_eq!(dispatch(&mut harness, &mut workspace), 0);
    let mut previous = workspace.session.outline_arc();
    assert_ne!(previous, original, "placing a center updates the proof");
    for at in [Point::new(320.0, 210.0), Point::new(340.0, 230.0)] {
        harness.mouse_move(at);
        assert_eq!(dispatch(&mut harness, &mut workspace), 0);
        let proof = workspace.session.outline_arc();
        assert_ne!(proof, previous, "the proof follows each pointer move");
        harness.edit_root_widget(|root| {
            assert_eq!(proof, root.widget.session.outline_arc());
            assert!(root.widget.session.gesture_in_progress());
            assert!(root.widget.session.pending_canonical.is_none());
        });
        assert_eq!(workspace.font.project.document_revision(), revision);
        assert_eq!(workspace.metadata_undo.len(), undo_depth);
        assert_eq!(workspace.metadata_redo.len(), redo_depth);
        assert!(!workspace.modified);
        previous = proof;
    }

    if cancel {
        harness.process_pointer_event(PointerEvent::Cancel(PRIMARY_MOUSE));
        assert_eq!(dispatch(&mut harness, &mut workspace), 0);
        assert_eq!(workspace.session.outline_arc(), original);
        assert_eq!(workspace.font.project.document_revision(), revision);
        assert_eq!(workspace.metadata_undo.len(), undo_depth);
        assert_eq!(workspace.metadata_redo.len(), redo_depth);
        assert!(!workspace.modified);
    } else {
        harness.mouse_button_release(Some(PointerButton::Primary));
        assert_eq!(dispatch(&mut harness, &mut workspace), 1);
        assert_eq!(workspace.session.outline_arc(), previous);
        assert_eq!(workspace.font.project.document_revision(), revision + 1);
        assert_eq!(workspace.metadata_undo.len(), undo_depth + 1);
        workspace.undo_open_glyph(false);
        assert_eq!(workspace.session.outline_arc(), original);
    }
    assert!(!workspace.session.gesture_in_progress());
    std::fs::remove_dir_all(path).expect("the fixture is removed");
}

#[test]
fn metaball_drag_updates_the_proof_before_one_release_commit() {
    check_metaball_drag_preview(false);
}

#[test]
fn cancelled_metaball_drag_restores_the_live_proof_without_committing() {
    check_metaball_drag_preview(true);
}

#[test]
fn a_measure_drag_lays_a_ruler_without_editing_and_a_click_clears_it() {
    let mut editor = widget();
    editor.tool = Tool::Measure;
    let points = editor.session.points().len();
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));

    harness.mouse_move(Point::new(100.0, 200.0));
    assert_eq!(harness.cursor_icon(), CursorIcon::Crosshair);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(Point::new(400.0, 210.0));
    harness.mouse_button_release(Some(PointerButton::Primary));
    let (ruler, selection, after) = harness.edit_root_widget(|root| {
        (
            root.widget.ruler,
            root.widget.session.selection.len(),
            root.widget.session.points().len(),
        )
    });
    let ruler = ruler.expect("the drag leaves a ruler");
    assert!(ruler.length() > 1.0);
    assert_eq!((selection, after), (0, points), "measuring edits nothing");

    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert!(harness.edit_root_widget(|root| root.widget.ruler.is_none()));
}

#[test]
fn shift_holds_a_ruler_to_45_degree_steps() {
    let from = Point::new(10.0, 10.0);
    assert_eq!(constrain_to_45(from, Point::new(110.0, 14.0)).y, 10.0);
    let diagonal = constrain_to_45(from, Point::new(110.0, 100.0));
    assert!((diagonal.x - diagonal.y).abs() < 1e-9);
}

#[test]
fn hand_tool_primary_drag_pans_without_editing() {
    let mut editor = widget();
    editor.tool = Tool::Hand;
    let selection = editor.session.selection.clone();
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let before = harness.edit_root_widget(|root| root.widget.session.viewport.offset);

    harness.mouse_move(Point::new(240.0, 180.0));
    assert_eq!(harness.cursor_icon(), CursorIcon::Grab);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(Point::new(275.0, 205.0));
    assert_eq!(harness.cursor_icon(), CursorIcon::Grabbing);
    harness.mouse_button_release(Some(PointerButton::Primary));

    let (after, after_selection) = harness.edit_root_widget(|root| {
        (
            root.widget.session.viewport.offset,
            root.widget.session.selection.clone(),
        )
    });
    assert_eq!(after - before, kurbo::Vec2::new(35.0, 25.0));
    assert_eq!(
        after_selection, selection,
        "panning does not select or move points"
    );
}

#[test]
fn hoi_pointer_drag_control_cancel_and_undo_preserve_the_master() {
    use crate::application::editor::tools::interpolation::InterpolationEvent;
    use kurbo::{CubicBez, ParamCurve, Vec2};
    use masonry::core::keyboard::{Code, KeyboardEvent, Modifiers};
    let mut app = Workspace::open(std::path::Path::new(
        "assets/font-sources/variable-fonts/HOIShapes/HOIShapes.designspace",
    ))
    .unwrap();
    app.open_glyph(app.font.index_of("box").unwrap());
    app.select_tool(Tool::Interpolation);
    app.set_interpolation_interval(0);
    let original = app.session.projected_glyph().unwrap();
    let mut editor = widget();
    editor.session = (*app.session).clone();
    editor.tool = Tool::Interpolation;
    editor.hoi = Some(app.interpolation_canvas().unwrap());
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (1000, 700));
    let dispatch =
        |harness: &mut TestHarness<EditorWidget>, app: &mut Workspace| {
            while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
                harness.edit_root_widget(|root| {
                    dispatch_editor_event(app, &mut root.widget.session, event, |app, event| {
                        match event {
                            EditorEvent::Interpolation(event) => app.interpolation_event(event),
                            EditorEvent::Undo => app.undo_open_glyph(false),
                            EditorEvent::Redo => app.undo_open_glyph(true),
                            _ => panic!("unexpected outline event"),
                        }
                    });
                    root.widget.hoi = Some(app.interpolation_canvas().unwrap());
                });
            }
        };
    let at = harness.edit_root_widget(|root| {
        let p = root.widget.hoi.as_ref().unwrap().connections[0].points;
        root.widget.session.viewport.affine() * CubicBez::new(p[0], p[1], p[2], p[3]).eval(0.5)
    });
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    dispatch(&mut harness, &mut app);
    for dy in [10.0, 20.0, 40.0] {
        harness.mouse_move(at + Vec2::new(0.0, -dy));
        dispatch(&mut harness, &mut app);
    }
    assert!(
        app.font.project.hoi_trajectories().unwrap().is_empty(),
        "drag stays transient"
    );
    harness.mouse_button_release(Some(PointerButton::Primary));
    dispatch(&mut harness, &mut app);
    assert_eq!(app.metadata_undo.len(), 1);
    let committed = app.font.project.hoi_trajectories().unwrap();
    assert_eq!(committed.len(), 1);
    assert_eq!(app.session.projected_glyph().unwrap(), original);

    let control = harness.edit_root_widget(|root| {
        root.widget.session.viewport.affine()
            * root.widget.hoi.as_ref().unwrap().connections[0].points[1]
    });
    harness.mouse_move(control);
    harness.mouse_button_press(Some(PointerButton::Primary));
    dispatch(&mut harness, &mut app);
    harness.mouse_move(control + Vec2::new(30.0, -20.0));
    dispatch(&mut harness, &mut app);
    harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
        state: KeyState::Down,
        key: Key::Named(NamedKey::Escape),
        code: Code::Escape,
        ..KeyboardEvent::default()
    }));
    dispatch(&mut harness, &mut app);
    harness.mouse_button_release(Some(PointerButton::Primary));
    dispatch(&mut harness, &mut app);
    assert_eq!(app.font.project.hoi_trajectories().unwrap(), committed);
    assert_eq!(app.metadata_undo.len(), 1);

    harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
        state: KeyState::Down,
        key: Key::Character("z".into()),
        code: Code::KeyZ,
        modifiers: Modifiers::META,
        ..KeyboardEvent::default()
    }));
    dispatch(&mut harness, &mut app);
    assert!(app.font.project.hoi_trajectories().unwrap().is_empty());
    assert_eq!(app.session.projected_glyph().unwrap(), original);
    app.interpolation_event(InterpolationEvent::SelectAll);
    assert_eq!(app.interpolation_canvas().unwrap().selected.len(), 4);
    app.set_interpolation_preview(1);
    let viewport = harness.edit_root_widget(|root| {
        root.widget.hoi = Some(app.interpolation_canvas().unwrap());
        assert!(root.widget.interpolation_hit(at).is_none());
        root.widget.session.viewport.affine()
    });
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(at + Vec2::new(30.0, -20.0));
    harness.mouse_button_release(Some(PointerButton::Primary));
    for key in [NamedKey::ArrowUp, NamedKey::Delete] {
        harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Named(key),
            ..KeyboardEvent::default()
        }));
    }
    assert!(
        harness.pop_action::<EditorEvent>().is_none(),
        "preview clicks and keys cannot edit"
    );
    harness.edit_root_widget(|root| {
        assert_eq!(root.widget.session.viewport.affine(), viewport);
    });
}
