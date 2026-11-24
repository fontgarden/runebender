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
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/arabic/Arabic-Regular.ufo");
    let project = runebender::font::project::Project::load(&path).expect("the fixture loads");
    let mut app =
        Workspace::from_model(FontModel::from_project(project)).expect("the fixture opens");
    let index = app.font.index_of(glyph).expect("the fixture has the glyph");
    app.open_glyph(index);
    app
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
