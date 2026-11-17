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
    assert_eq!(widget.metric_boxes().expect("the panel fits").len(), 3);

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
    let (_, origin) = (0, widget.metrics_panel_origin().expect("it fits"));
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
    for (field, rect) in boxes {
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
        assert_eq!(boxes[0].1.x0, left + 70.0);
        assert_eq!(boxes[1].1.x0, left + 132.0);
        assert_eq!(boxes[2].1.x0, left + 194.0);
    }
    widget.size = Size::new(280.0, 510.0);
    assert!(widget.metric_boxes().is_none());
    widget.size = Size::new(786.0, 60.0);
    assert!(widget.metric_boxes().is_none());
}
