// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Tests of the floating metrics card: field positions, focus and commits.

use super::*;

#[test]
fn space_preview_hides_metrics_hit_targets() {
    let mut widget = widget();
    widget.size = Size::new(600.0, 400.0);
    assert!(widget.metric_boxes().is_some());
    widget.preview_mode = true;
    assert!(
        widget.metric_boxes().is_none(),
        "hidden preview chrome cannot intercept a pan gesture"
    );
}

/// Typing in the width box changes the advance, and only on Enter.
#[test]
fn metric_box_commits_on_enter() {
    let mut widget = widget();
    widget.size = Size::new(600.0, 400.0);
    let before = widget.session.advance();
    widget.focus_metric(MetricField::Width);
    widget.field_buf.clear();
    widget.field_buf.push_str("900");
    assert_eq!(widget.session.advance(), before, "not until Enter");
    assert!(widget.commit_metric());
    assert_eq!(widget.session.advance(), 900.0);
}

#[test]
fn empty_glyph_keeps_metrics_inputs_and_can_change_its_advance() {
    let mut widget = widget();
    widget.session = empty_session();
    widget.size = Size::new(600.0, 400.0);

    assert!(widget.session.side_bearings().is_none());
    assert_eq!(widget.metric_boxes().expect("the panel fits").len(), 5);

    widget.focus_metric(MetricField::Width);
    assert_eq!(widget.field_buf, "500");
    widget.field_buf = "720".into();
    assert!(widget.commit_metric());
    assert_eq!(widget.session.advance(), 720.0);

    widget.focus_metric(MetricField::Rsb);
    assert_eq!(widget.field_buf, "720");
}

/// The boxes are where the centered panel paints them.
#[test]
fn metric_boxes_sit_in_the_panel() {
    let mut widget = widget();
    widget.size = Size::new(600.0, 400.0);
    let boxes = widget.metric_boxes().expect("the panel fits");
    let origin = widget.metrics_panel_origin().expect("it fits");
    let rect = |field| boxes.iter().find(|(f, _)| *f == field).unwrap().1;
    let left = rect(MetricField::GroupLeft);
    let right = rect(MetricField::GroupRight);
    let center = origin.0 + PANEL_WIDTH / 2.0;
    assert_eq!(rect(MetricField::Width).center().x, center);
    assert_eq!(left.x0, origin.0 + PANEL_PAD);
    assert_eq!(right.x1, origin.0 + PANEL_WIDTH - PANEL_PAD);
    assert!((left.width() - right.width()).abs() < 1e-9);
    for (_, rect) in boxes {
        assert!(rect.x0 >= origin.0, "inside the panel's left edge");
        assert!(rect.x1 <= origin.0 + PANEL_WIDTH, "inside its right edge");
        assert!(rect.y0 >= origin.1, "below its top");
    }
}

#[test]
fn moved_metrics_fields_receive_clicks_and_commit_through_keyboard_events() {
    use masonry::core::keyboard::KeyboardEvent;
    let mut harness =
        TestHarness::create_with_size(default_property_set(), widget().prepare(), (786, 510));
    let boxes = harness.edit_root_widget(|root| root.widget.metric_boxes().unwrap());
    for &(field, rect) in &boxes {
        harness.mouse_move(rect.center());
        harness.mouse_button_press(Some(PointerButton::Primary));
        assert!(harness.edit_root_widget(|root| root.widget.field == Some(field)));
    }
    let width = boxes[1].1;
    harness.mouse_move(width.center());
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.edit_root_widget(|root| root.widget.field_buf = "900".into());
    harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
        state: KeyState::Down,
        key: Key::Named(NamedKey::Enter),
        ..KeyboardEvent::default()
    }));
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.session.advance()),
        900.0
    );
    assert!(harness.edit_root_widget(|root| root.widget.field.is_none()));
}

#[test]
fn metrics_card_keeps_its_bottom_clearance_and_hides_when_it_cannot_fit() {
    let mut widget = widget();
    for width in [606.0, 786.0] {
        widget.size = Size::new(width, 510.0);
        let (left, top) = widget.metrics_panel_origin().unwrap();
        assert_eq!(left + PANEL_WIDTH / 2.0, width / 2.0);
        assert_eq!(top + PANEL_HEIGHT, 498.0);
        let boxes = widget.metric_boxes().unwrap();
        assert_eq!(boxes[1].1.center().x, width / 2.0);
        assert_eq!(boxes[1].1.x0 - boxes[0].1.x1, METRICS_FIELD_GAP);
        assert_eq!(boxes[2].1.x0 - boxes[1].1.x1, METRICS_FIELD_GAP);
    }
    widget.size = Size::new(280.0, 510.0);
    assert!(widget.metric_boxes().is_none());
    widget.size = Size::new(786.0, 60.0);
    assert!(widget.metric_boxes().is_none());
}

#[test]
fn left_kern_field_submits_the_shaped_arabic_pair_on_enter() {
    use masonry::core::keyboard::KeyboardEvent;
    use runebender::text::buffer::TextBuffer;
    let mut editor = widget();
    let mut state = crate::application::editor::tools::text::TextState::test_buffer();
    state.buffer = TextBuffer::new();
    state.buffer.insert_glyph("reh-ar.fina", Some('ر'), 496.0);
    state.buffer.insert_glyph("teh-ar.init", Some('ت'), 260.0);
    state.buffer.activate_sort(0);
    editor.session.glyph_name = "reh-ar.fina".into();
    editor.text = Some(state);
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (786, 510));
    let rect = harness.edit_root_widget(|root| {
        root.widget
            .metric_boxes()
            .unwrap()
            .into_iter()
            .find(|(f, _)| *f == MetricField::KernLeft)
            .unwrap()
            .1
    });
    harness.mouse_move(rect.center());
    harness.mouse_button_press(Some(PointerButton::Primary));
    for key in [Key::Character("-32".into()), Key::Named(NamedKey::Enter)] {
        harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key,
            ..KeyboardEvent::default()
        }));
    }
    let mut pairs = Vec::new();
    while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
        if let EditorEvent::KernPair(a, b, v) = event {
            pairs.push((a, b, v));
        }
    }
    assert_eq!(
        pairs,
        vec![("reh-ar.fina".into(), "teh-ar.init".into(), -32.0)]
    );
}

#[test]
fn rtl_pair_scope_can_remove_an_exception_or_edit_the_group() {
    use masonry::core::keyboard::KeyboardEvent;
    let mut editor = widget();
    let mut state = crate::application::editor::tools::text::TextState::test_buffer();
    state.buffer = runebender::text::buffer::TextBuffer::new();
    state.buffer.insert_glyph("reh-ar.fina", Some('ر'), 496.0);
    state.buffer.insert_glyph("teh-ar.init", Some('ت'), 260.0);
    state.buffer.activate_sort(0);
    state.buffer.set_kerning_model(serde_json::from_value(serde_json::json!({
        "rightGroups":{"reh-ar.fina":"public.kern1.reh"},
        "leftGroups":{"teh-ar.init":"public.kern2.teh"},
        "kerning":{"reh-ar.fina":{"public.kern2.teh":0},"public.kern1.reh":{"public.kern2.teh":-20}}
    })).unwrap());
    editor.session.glyph_name = "reh-ar.fina".into();
    editor.text = Some(state);
    editor.focus_metric(MetricField::KernLeft);
    assert_eq!(editor.field_buf, "0");
    assert_eq!(editor.kern_groups, [false, true]);
    assert!(editor.toggle_kern_scope(MetricField::GroupRight));
    assert_eq!(editor.field_buf, "-20");
    assert!(editor.toggle_kern_scope(MetricField::GroupRight));
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (786, 510));
    let rect = harness.edit_root_widget(|r| {
        r.widget
            .metric_boxes()
            .unwrap()
            .into_iter()
            .find(|(f, _)| *f == MetricField::KernLeft)
            .unwrap()
            .1
    });
    harness.mouse_move(rect.center());
    harness.mouse_button_press(Some(PointerButton::Primary));
    for key in [NamedKey::Backspace, NamedKey::Enter] {
        harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Named(key),
            ..Default::default()
        }));
    }
    let mut removed = Vec::new();
    while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
        if let EditorEvent::RemoveKernPair(a, b) = event {
            removed.push((a, b));
        }
    }
    assert_eq!(
        removed,
        vec![("reh-ar.fina".to_string(), "public.kern2.teh".to_string())]
    );
}

/// A composite glyph has ink where its component draws, so its bearings count that outline.
/// Without it the card showed the glyph as blank, and a typed right bearing became the advance.
#[test]
fn a_composite_glyph_measures_its_bearings_from_the_component_outline() {
    let mut editor = composite_editor();
    let bearings = editor
        .session
        .side_bearings()
        .expect("a glyph with a component has ink");
    assert_eq!((bearings.lsb, bearings.rsb), (0, 100));

    editor.field = Some(MetricField::Rsb);
    editor.field_buf = "50".into();
    assert!(editor.commit_metric());
    assert_eq!(editor.session.advance(), 450.0);
}
