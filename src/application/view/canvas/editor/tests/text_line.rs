// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Tests of the text line during gestures and refreshes, on the repository-owned Arabic
//! fixture: live values during a drag (rule 4) and the anchored view (rule 8) of section 5.2
//! in `docs/ARABIC-TEXT-LAYOUT-PLAN.md`.

use super::*;
use crate::application::editor::tools::text::{TextInputs, TextState};
use crate::application::font_model::FontModel;

fn arabic_workspace(glyph: &str) -> Workspace {
    workspace_at(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/arabic/Arabic-Regular.ufo"),
        glyph,
    )
}

fn workspace_at(path: &std::path::Path, glyph: &str) -> Workspace {
    let project = runebender::font::project::Project::load(path).expect("the fixture loads");
    let mut app =
        Workspace::from_model(FontModel::from_project(project)).expect("the fixture opens");
    let index = app.font.index_of(glyph).expect("the fixture has the glyph");
    app.open_glyph(index);
    app
}

/// Drag the anchor `name` of the open glyph by `steps` total offsets, a step each.
fn drag_anchor(editor: &mut EditorWidget, name: &str, steps: &[(f64, f64)]) {
    let anchor = editor
        .session
        .current_layer()
        .and_then(|layer| layer.anchors().find(|anchor| anchor.name() == name))
        .expect("the glyph has the anchor")
        .id();
    editor.session.press_anchor(anchor, false);
    editor.session.begin_point_drag();
    editor.drag = Drag::Points {
        start: Point::ORIGIN,
    };
    for step in steps {
        assert!(editor.session.drag_points_to(*step, false));
        assert!(editor.follow_gesture(), "each step reshapes the line");
    }
}

/// End a point drag and commit it, as the release and the canvas view do.
fn release_and_commit(app: &mut Workspace, editor: &mut EditorWidget, text: &str) {
    editor.session.end_point_drag();
    editor.drag = Drag::None;
    editor.end_gesture(false);
    commit(app, editor, text);
}

/// An editor widget on the open glyph of `app`, with `text` in its line.
fn line_editor(app: &Workspace, text: &str) -> EditorWidget {
    let mut editor = widget();
    editor.session = (*app.session).clone();
    let inputs = TextInputs::for_editor(app).with_text(text);
    assert!(
        TextState::new(&inputs).has_live_layout(),
        "the line is shaped by the live layout of the master"
    );
    editor.text = Some(TextState::new(&inputs));
    editor.text_inputs = Some(inputs);
    editor
}

/// Each drawn sort and its origin.
fn line(editor: &EditorWidget) -> Vec<(usize, Point)> {
    editor
        .text
        .as_ref()
        .expect("a text line")
        .placed()
        .iter()
        .map(|sort| (sort.index, sort.origin))
        .collect()
}

fn origin_of(line: &[(usize, Point)], index: usize) -> Point {
    line.iter()
        .find(|(sort, _)| *sort == index)
        .map(|(_, origin)| *origin)
        .expect("the sort is drawn")
}

/// The end of a gesture as the canvas view runs it: the workspace commits the session, and the
/// view hands the widget the text inputs that the app reads again.
fn commit(app: &mut Workspace, editor: &mut EditorWidget, text: &str) {
    dispatch_editor_event(app, &mut editor.session, EditorEvent::Edited, |app, _| {
        app.finish_open_glyph_refresh();
    });
    refresh(app, editor, text);
}

fn refresh(app: &Workspace, editor: &mut EditorWidget, text: &str) {
    let inputs = TextInputs::for_editor(app).with_text(text);
    editor.refresh_text(&inputs);
    editor.text_inputs = Some(inputs);
}

/// Where a fresh text line shows `text` after the commit.
fn fresh_line(app: &Workspace, text: &str) -> Vec<(usize, Point)> {
    line(&line_editor(app, text))
}

#[test]
fn an_anchor_drag_moves_the_marks_on_every_base_and_the_commit_moves_nothing() {
    // Two isolated behs, each with a kasra at its `bottom` anchor.
    const TEXT: &str = "بِ بِ";
    let mut app = arabic_workspace("beh-ar");
    let mut editor = line_editor(&app, TEXT);
    let before = line(&editor);
    let bottom = editor
        .session
        .current_layer()
        .and_then(|layer| layer.anchors().find(|anchor| anchor.name() == "bottom"))
        .expect("beh has a bottom anchor")
        .id();
    editor.session.press_anchor(bottom, false);
    editor.session.begin_point_drag();
    editor.drag = Drag::Points {
        start: Point::ORIGIN,
    };
    for step in 1..=4 {
        let step = f64::from(step);
        assert!(
            editor
                .session
                .drag_points_to((10.0 * step, -5.0 * step), false)
        );
        assert!(editor.follow_gesture(), "each step reshapes the line");
    }
    let dragged = line(&editor);
    for kasra in [1, 4] {
        assert_eq!(
            origin_of(&dragged, kasra) - origin_of(&before, kasra),
            kurbo::Vec2::new(40.0, -20.0),
            "the kasra on sort {} follows the anchor in the same frame",
            kasra - 1
        );
    }
    for base in [0, 3] {
        assert_eq!(origin_of(&dragged, base), origin_of(&before, base));
    }

    let screen = editor.glyph_affine() * Point::ORIGIN;
    editor.session.end_point_drag();
    editor.drag = Drag::None;
    editor.follow_gesture();
    commit(&mut app, &mut editor, TEXT);
    assert_eq!(line(&editor), dragged, "the commit does not move the line");
    assert_eq!(editor.glyph_affine() * Point::ORIGIN, screen);
    assert_eq!(
        fresh_line(&app, TEXT),
        dragged,
        "a drag ends where a fresh layout of the committed values is"
    );
}

#[test]
fn an_advance_drag_moves_the_line_and_keeps_the_dragged_glyph_in_place() {
    // Right to left: reh is the active sort, at the right; beh is at the left.
    const TEXT: &str = "رب";
    let mut app = arabic_workspace("reh-ar");
    let editor = line_editor(&app, TEXT);
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (800, 400));
    let screen = |harness: &mut TestHarness<EditorWidget>| {
        harness.edit_root_widget(|root| root.widget.glyph_affine() * Point::ORIGIN)
    };
    let start = screen(&mut harness);
    let advance = harness.edit_root_widget(|root| root.widget.session.advance());
    for step in 1..=3 {
        harness.edit_root_widget(|mut root| {
            let editor = &mut root.widget;
            editor.drag = Drag::AdvanceLine;
            editor
                .session
                .drag_advance(advance + 20.0 * f64::from(step));
            assert!(editor.follow_gesture());
        });
        assert_eq!(screen(&mut harness), start, "the dragged glyph stays put");
    }
    let dragged = harness.edit_root_widget(|root| line(root.widget));
    harness.edit_root_widget(|mut root| {
        let editor = &mut root.widget;
        editor.session.end_metric_drag();
        editor.drag = Drag::None;
        editor.follow_gesture();
        commit(&mut app, editor, TEXT);
        assert_eq!(line(editor), dragged, "the commit does not move the line");
    });
    assert_eq!(screen(&mut harness), start);
    assert_eq!(fresh_line(&app, TEXT), dragged);
}

#[test]
fn an_advance_edit_left_of_the_active_glyph_keeps_it_in_place() {
    // Left to right: V is the active sort, to the right of A.
    const TEXT: &str = "AV";
    let mut app = arabic_workspace("V");
    let editor = line_editor(&app, TEXT);
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (800, 400));
    let (screen, design) = harness.edit_root_widget(|root| {
        (
            root.widget.glyph_affine() * Point::ORIGIN,
            root.widget.active_sort_origin(),
        )
    });

    // A committed edit, as the metrics card or Undo makes it.
    let layer = app
        .font
        .project
        .document_sources()
        .next()
        .expect("a source")
        .default_layer();
    app.font
        .project
        .edit_document_layer("A", &layer, |draft| {
            let width = draft.view().width();
            draft.set_width(width + 50.0)?;
            Ok(())
        })
        .expect("the width edit commits");
    let index = app.font.index_of("A").expect("A");
    app.font.refresh_entry(index);
    harness.edit_root_widget(|root| refresh(&app, root.widget, TEXT));

    let (after, moved) = harness.edit_root_widget(|root| {
        (
            root.widget.glyph_affine() * Point::ORIGIN,
            root.widget.active_sort_origin(),
        )
    });
    assert_eq!(moved.x - design.x, 50.0, "the layout moved V");
    assert_eq!(after, screen, "the view moved with it");
}

#[test]
fn a_kerning_drag_shapes_with_the_dragged_value_and_the_commit_moves_nothing() {
    const TEXT: &str = "را";
    let mut app = arabic_workspace("reh-ar");
    let mut editor = line_editor(&app, TEXT);
    editor.tool = Tool::Text;
    let before = line(&editor);
    let screen = editor.glyph_affine() * Point::ORIGIN;
    let (first, second, boundary, rtl, original) = {
        let text = editor.text.as_ref().expect("a text line");
        let (a, b, boundary, rtl, original) = text.buffer.drag_pair(1).expect("a pair");
        let [ga, gb] = text.buffer.pair_groups(a, b);
        (
            ga.expect("reh has a group").to_string(),
            gb.expect("alef has a group").to_string(),
            boundary,
            rtl,
            original,
        )
    };
    assert_eq!((rtl, original), (true, -60.0));
    editor.drag = Drag::Kern {
        grabbed: 1,
        first,
        second,
        boundary,
        start_x: 0.0,
        start_offset: editor.session.viewport.offset.x,
        rtl,
        original,
        value: original,
    };
    // Right to left, a pointer to the left widens the pair.
    for x in [-10.0, -20.0, -40.0] {
        assert!(super::super::tools::text::move_kern(&mut editor, x).is_some());
    }
    let dragged = line(&editor);
    assert_ne!(dragged, before, "the line follows the dragged value");
    assert_eq!(
        editor.glyph_affine() * Point::ORIGIN,
        screen,
        "the active reh stays put"
    );
    let Some(EditorEvent::KernPair(first, second, value)) =
        super::super::tools::text::finish_kern(&mut editor, false)
    else {
        panic!("the drag commits a pair");
    };
    // The value snaps to the dot grid, so it is near the 40 units the pointer moved.
    assert!(
        value > original + 30.0 && value < original + 50.0,
        "{value}"
    );
    app.kern_first_buf = first;
    app.kern_second_buf = second;
    app.kern_value_buf = value.to_string();
    app.set_kern_pair_from_bufs();
    refresh(&app, &mut editor, TEXT);
    assert_eq!(line(&editor), dragged, "the commit does not move the line");
    assert_eq!(editor.glyph_affine() * Point::ORIGIN, screen);
    assert_eq!(fresh_line(&app, TEXT), dragged);
}

/// Print the time of one drag step of an anchor, an advance and a kerning pair on Virtua
/// Grotesk: the table build and the shaping of the line.
///
/// Run with `cargo test --features integration-fonts drag_step_timings -- --ignored --nocapture`.
#[cfg(feature = "integration-fonts")]
#[test]
#[ignore = "prints the time of each drag step on the Virtua Grotesk sources"]
#[expect(
    clippy::print_stdout,
    reason = "the timing probe reports to the terminal"
)]
fn drag_step_timings() {
    use std::time::{Duration, Instant};
    const TEXT: &str = "اَ بِسْمِ ٱللَّهِ AVATAR";
    let path = crate::integration_fonts::dir().join("VirtuaGrotesk.designspace");
    let project = runebender::font::project::Project::load(&path).expect("Virtua loads");
    let mut app = Workspace::from_model(FontModel::from_project(project)).expect("Virtua opens");
    let index = app.font.index_of("alef-ar").expect("alef-ar");
    app.open_glyph(index);
    let mut editor = line_editor(&app, TEXT);
    let report = |label: &str, mut samples: Vec<Duration>| {
        samples.sort();
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        println!(
            "{label:<10} fastest {:>6.2} ms   median {:>6.2} ms   slowest {:>6.2} ms",
            ms(samples[0]),
            ms(samples[samples.len() / 2]),
            ms(samples[samples.len() - 1])
        );
    };

    let anchor = editor
        .session
        .current_layer()
        .and_then(|layer| layer.anchors().next())
        .expect("alef has an anchor")
        .id();
    editor.session.press_anchor(anchor, false);
    editor.session.begin_point_drag();
    editor.drag = Drag::Points {
        start: Point::ORIGIN,
    };
    let mut samples = Vec::new();
    for step in 1..=20 {
        assert!(editor.session.drag_points_to((f64::from(step), 0.0), false));
        let start = Instant::now();
        assert!(editor.follow_gesture());
        samples.push(start.elapsed());
    }
    report("anchor", samples);
    editor.session.cancel_point_drag();
    editor.drag = Drag::None;
    editor.follow_gesture();

    let advance = editor.session.advance();
    editor.drag = Drag::AdvanceLine;
    let mut samples = Vec::new();
    for step in 1..=20 {
        editor.session.drag_advance(advance + f64::from(step));
        let start = Instant::now();
        assert!(editor.follow_gesture());
        samples.push(start.elapsed());
    }
    report("advance", samples);
    editor.session.cancel_metric_drag();
    editor.drag = Drag::None;
    editor.follow_gesture();

    let (grabbed, first, second, boundary, rtl, original) = {
        let buffer = &editor.text.as_ref().expect("a line").buffer;
        (1..buffer.len())
            .find_map(|index| {
                let (a, b, boundary, rtl, original) = buffer.drag_pair(index)?;
                (original != 0.0)
                    .then(|| (index, a.to_string(), b.to_string(), boundary, rtl, original))
            })
            .expect("a kerned pair in the line")
    };
    let mut samples = Vec::new();
    for step in 1..=20 {
        editor.drag = Drag::Kern {
            grabbed,
            first: first.clone(),
            second: second.clone(),
            boundary,
            start_x: 0.0,
            start_offset: editor.session.viewport.offset.x,
            rtl,
            original,
            value: original + 10.0 * f64::from(step),
        };
        let start = Instant::now();
        assert!(editor.follow_gesture());
        samples.push(start.elapsed());
    }
    report("kerning", samples);
}

#[test]
fn a_contour_drag_rebuilds_no_table() {
    const TEXT: &str = "بِ بِ";
    let mut app = arabic_workspace("beh-ar");
    let mut editor = line_editor(&app, TEXT);
    let live = app.font.live_layout(&app.axis_values);
    let source = live.source.expect("live layout");
    let counts = source.build_counts();
    let before = line(&editor);
    let points: Vec<_> = editor
        .session
        .current_layer()
        .and_then(|layer| layer.contours().next())
        .map(|contour| contour.points().map(|point| point.id()).collect())
        .expect("beh has a contour");
    editor.session.select_points(points, false);
    editor.session.begin_point_drag();
    editor.drag = Drag::Points {
        start: Point::ORIGIN,
    };
    assert!(editor.session.drag_points_to((12.0, 8.0), false));
    assert!(
        !editor.follow_gesture(),
        "moving contour points changes no layout"
    );
    editor.session.end_point_drag();
    editor.drag = Drag::None;
    editor.follow_gesture();
    commit(&mut app, &mut editor, TEXT);
    assert_eq!(line(&editor), before);
    assert_eq!(source.build_counts(), counts, "no step of a build ran");
    assert!(Arc::ptr_eq(
        live.font.as_ref().expect("live layout"),
        app.font
            .live_layout(&app.axis_values)
            .font
            .as_ref()
            .expect("live layout"),
    ));
}

/// The Arabic designspace with the sliders halfway between Regular and Bold.
fn between_masters(glyph: &str) -> Workspace {
    let mut app = workspace_at(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/arabic/Arabic.designspace"),
        glyph,
    );
    let axis = app.font.axes.first().expect("a weight axis").clone();
    app.set_axis(0, (axis.min + axis.max) / 2.0);
    assert!(!app.on_active_master());
    app
}

#[test]
fn an_anchor_drag_between_masters_moves_the_marks_by_the_weight_of_the_master() {
    const TEXT: &str = "بِ بِ";
    let mut app = between_masters("beh-ar");
    let mut editor = line_editor(&app, TEXT);
    let before = line(&editor);
    drag_anchor(&mut editor, "bottom", &[(20.0, -10.0), (40.0, -20.0)]);
    let dragged = line(&editor);
    for kasra in [1, 4] {
        let moved = origin_of(&dragged, kasra) - origin_of(&before, kasra);
        assert!(
            moved.x > 0.0 && moved.x < 40.0 && moved.y < 0.0 && moved.y > -20.0,
            "the kasra moves by part of the drag: {moved:?}"
        );
    }
    release_and_commit(&mut app, &mut editor, TEXT);
    assert_eq!(line(&editor), dragged, "the commit does not move the line");
    assert_eq!(fresh_line(&app, TEXT), dragged);
}

#[test]
fn a_kerning_drag_between_masters_shapes_with_the_dragged_value() {
    const TEXT: &str = "را";
    let mut app = between_masters("reh-ar");
    let mut editor = line_editor(&app, TEXT);
    editor.tool = Tool::Text;
    let before = line(&editor);
    let (first, second, boundary, rtl, original) = {
        let text = editor.text.as_ref().expect("a text line");
        let (a, b, boundary, rtl, original) = text.buffer.drag_pair(1).expect("a pair");
        let [ga, gb] = text.buffer.pair_groups(a, b);
        (
            ga.expect("reh has a group").to_string(),
            gb.expect("alef has a group").to_string(),
            boundary,
            rtl,
            original,
        )
    };
    editor.drag = Drag::Kern {
        grabbed: 1,
        first,
        second,
        boundary,
        start_x: 0.0,
        start_offset: editor.session.viewport.offset.x,
        rtl,
        original,
        value: original,
    };
    for x in [-20.0, -40.0] {
        assert!(super::super::tools::text::move_kern(&mut editor, x).is_some());
    }
    let dragged = line(&editor);
    assert_ne!(dragged, before, "the line follows the dragged value");
    let Some(EditorEvent::KernPair(first, second, value)) =
        super::super::tools::text::finish_kern(&mut editor, false)
    else {
        panic!("the drag commits a pair");
    };
    assert_eq!(
        line(&editor),
        dragged,
        "the release keeps the dragged value"
    );
    app.kern_first_buf = first;
    app.kern_second_buf = second;
    app.kern_value_buf = value.to_string();
    app.set_kern_pair_from_bufs();
    refresh(&app, &mut editor, TEXT);
    assert_eq!(line(&editor), dragged, "the commit does not move the line");
    assert_eq!(fresh_line(&app, TEXT), dragged);
}

/// A composite with no anchors of its own takes them from its components, so a drag of an
/// anchor of the base moves the marks on the composite during the drag.
#[cfg(feature = "integration-fonts")]
#[test]
fn a_base_anchor_drag_moves_the_marks_on_composites_that_use_the_base() {
    // `beh-ar.init` is `behDotless-ar.init` and a dot below, and has no anchors of its own.
    const TEXT: &str = "بَب";
    let path = crate::integration_fonts::dir().join("VirtuaGrotesk.designspace");
    let mut app = workspace_at(&path, "behDotless-ar.init");
    let mut editor = line_editor(&app, TEXT);
    let before = line(&editor);
    let names: Vec<String> = editor
        .text
        .as_ref()
        .expect("a line")
        .buffer
        .iter()
        .map(|sort| sort.glyph_name().unwrap_or_default().to_string())
        .collect();
    assert_eq!(names[0], "beh-ar.init");
    assert!(names[1].starts_with("fatha"), "{names:?}");
    drag_anchor(&mut editor, "top", &[(15.0, -10.0), (30.0, -20.0)]);
    let dragged = line(&editor);
    assert_eq!(
        origin_of(&dragged, 1) - origin_of(&before, 1),
        kurbo::Vec2::new(30.0, -20.0),
        "the fatha on the composite follows the anchor of its base"
    );
    release_and_commit(&mut app, &mut editor, TEXT);
    assert_eq!(line(&editor), dragged, "the commit does not move the line");
    assert_eq!(fresh_line(&app, TEXT), dragged);
}

/// The frames around a release, rendered by the whole application: the last drag frame, the
/// frame after the release and before the commit reaches the app, and the frame after it.
#[test]
fn the_frames_around_a_release_show_the_same_layout() {
    use crate::application::platform::screenshot::Headless;
    use crate::application::view::render::app_logic;
    use masonry::core::{PointerButtonEvent, PointerUpdate};
    use std::time::Duration;
    use xilem::view::sized_box;

    let mut app = arabic_workspace("beh-ar");
    app.set_editor_text("بِ بِ".into());
    let logic = |app: &mut Workspace| sized_box(app_logic(app));
    let mut headless = Headless::new(app, logic, (1400, 900), 1.0);
    for _ in 0..2 {
        headless.rebuild(logic);
        headless.anim_frame(Duration::from_millis(500));
        headless.redraw();
    }
    // The active beh's origin and the kasra under it, on the screen.
    let frame = |headless: &Headless<_, _>| {
        let canvas = headless
            .find_widget::<EditorWidget>()
            .expect("the editor canvas is in the tree");
        let editor = canvas.inner();
        let viewport = editor.session.viewport.affine();
        (
            editor.glyph_affine() * Point::ORIGIN,
            viewport * origin_of(&line(editor), 1),
        )
    };
    let press = {
        let canvas = headless
            .find_widget::<EditorWidget>()
            .expect("the editor canvas is in the tree");
        let editor = canvas.inner();
        let bottom = editor
            .session
            .named_anchors()
            .into_iter()
            .find(|(name, _)| name == "bottom")
            .expect("beh has a bottom anchor")
            .1;
        canvas.ctx().window_transform() * (editor.glyph_affine() * bottom)
    };
    let state = |at: Point, down: bool| {
        let mut buttons = masonry::ui_events::pointer::PointerButtons::default();
        if down {
            buttons.insert(PointerButton::Primary);
        }
        PointerState {
            position: PhysicalPosition::new(at.x, at.y),
            buttons,
            ..PointerState::default()
        }
    };
    let start = frame(&headless);
    headless.pointer(PointerEvent::Move(PointerUpdate {
        pointer: PRIMARY_MOUSE,
        current: state(press, false),
        coalesced: vec![],
        predicted: vec![],
    }));
    headless.pointer(PointerEvent::Down(PointerButtonEvent {
        pointer: PRIMARY_MOUSE,
        button: Some(PointerButton::Primary),
        state: state(press, true),
    }));
    let end = press + kurbo::Vec2::new(24.0, 12.0);
    for step in 1..=4 {
        let at = press + kurbo::Vec2::new(6.0, 3.0) * f64::from(step);
        headless.pointer(PointerEvent::Move(PointerUpdate {
            pointer: PRIMARY_MOUSE,
            current: state(at, true),
            coalesced: vec![],
            predicted: vec![],
        }));
        headless.redraw();
    }
    let last_drag = frame(&headless);
    assert_eq!(last_drag.0, start.0, "the active beh stays put");
    assert_ne!(last_drag.1, start.1, "the kasra follows the anchor");

    headless.pointer(PointerEvent::Up(PointerButtonEvent {
        pointer: PRIMARY_MOUSE,
        button: Some(PointerButton::Primary),
        state: state(end, false),
    }));
    headless.redraw();
    assert_eq!(
        frame(&headless),
        last_drag,
        "the release frame shows the dragged values"
    );

    // The window delivers the widget's action to the app; do that by hand.
    let mut session = headless
        .find_widget::<EditorWidget>()
        .expect("the editor canvas is in the tree")
        .inner()
        .session
        .clone();
    assert_eq!(
        headless.app.sync_session_from(&mut session),
        SessionSyncOutcome::Changed
    );
    headless.app.finish_open_glyph_refresh();
    headless.rebuild(logic);
    headless.redraw();
    assert_eq!(
        frame(&headless),
        last_drag,
        "the commit frame shows the same layout"
    );
}

/// `ccmp` decomposes each teh into the dotless skeleton and the dots: the line draws all four
/// glyphs, and the dots, open in the editor, are edited at their own place in the line.
#[test]
fn a_decomposed_letter_draws_every_glyph_and_edits_its_dots_in_place() {
    use crate::application::platform::screenshot::Headless;
    use crate::application::view::render::app_logic;
    use std::time::Duration;
    use xilem::view::sized_box;
    const TEXT: &str = "تت";

    let app = arabic_workspace("twodotshorizontalabove-ar");
    let mut editor = line_editor(&app, TEXT);
    let text = editor.text.as_mut().expect("a text line");
    let placed = text.placed();
    let origins: Vec<Point> = placed.iter().map(|sort| sort.origin).collect();
    assert_eq!(
        origins,
        [
            Point::new(270.0, 320.0),
            Point::new(0.0, 0.0),
            Point::new(600.0, 320.0),
            Point::new(480.0, 0.0),
        ]
    );
    assert_eq!(
        placed.iter().map(|sort| sort.index).collect::<Vec<_>>(),
        [1, 1, 0, 0]
    );
    // The dots of the first teh are the glyph being edited.
    assert_eq!(
        text.buffer.active_glyph_name(),
        Some("twodotshorizontalabove-ar")
    );
    let active: Vec<Point> = placed
        .iter()
        .filter(|sort| sort.active)
        .map(|sort| sort.origin)
        .collect();
    assert_eq!(active, [Point::new(600.0, 320.0)]);
    assert_eq!(text.active_origin(), Some(Point::new(600.0, 320.0)));
    // The two dots share one outline, and the skeletons have their own.
    assert!(Arc::ptr_eq(&placed[0].outline, &placed[2].outline));
    assert!(!Arc::ptr_eq(&placed[1].outline, &placed[3].outline));

    // A selection of the first teh covers its skeleton and its dots.
    text.buffer.select_range(0, 1);
    let selected: Vec<bool> = text.placed().iter().map(|sort| sort.selected).collect();
    assert_eq!(selected, [false, false, true, true]);
    // The caret between the letters is at the left edge of the first.
    text.buffer.set_cursor(1);
    assert_eq!(text.caret(), Point::new(480.0, 0.0));

    // A click on the ink of the dots of the second teh edits those dots; a click on its
    // skeleton edits the skeleton.
    assert_eq!(text.click(Point::new(190.0, 420.0)), Some((1, Some(0))));
    assert_eq!(
        text.activate(1, Some(0)).as_deref(),
        Some("twodotshorizontalabove-ar")
    );
    assert_eq!(text.active_origin(), Some(Point::new(270.0, 320.0)));
    assert_eq!(text.click(Point::new(240.0, 40.0)), Some((1, None)));
    assert_eq!(
        text.activate(1, None).as_deref(),
        Some("behDotless-ar.fina")
    );

    // The whole application draws the line, the strip and the canvas without a glyph lost.
    let mut app = arabic_workspace("twodotshorizontalabove-ar");
    app.set_editor_text(TEXT.into());
    let logic = |app: &mut Workspace| sized_box(app_logic(app));
    let mut headless = Headless::new(app, logic, (1400, 900), 1.0);
    for _ in 0..2 {
        headless.rebuild(logic);
        headless.anim_frame(Duration::from_millis(500));
        headless.redraw();
    }
    let canvas = headless
        .find_widget::<EditorWidget>()
        .expect("the editor canvas is in the tree");
    let editor = canvas.inner();
    let text = editor.text.as_ref().expect("a text line");
    assert_eq!(text.placed().len(), 4);
    assert_eq!(
        text.buffer.active_glyph_name(),
        Some("twodotshorizontalabove-ar")
    );
    let active = text.active_origin().expect("an active glyph");
    assert_eq!(
        editor.glyph_affine() * Point::ORIGIN,
        editor.session.viewport.affine() * active,
        "the glyph editor draws the dots at their place in the line"
    );
}
