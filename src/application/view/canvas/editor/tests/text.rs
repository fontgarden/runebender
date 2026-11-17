// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Tests of the text line: typing, the cursor, and activating a composed sort.

use super::*;

#[test]
fn text_cursor_animation_hides_then_resets_on_input() {
    use masonry::core::keyboard::{Code, KeyboardEvent};

    let mut editor = widget();
    editor.tool = Tool::Text;
    editor.text = Some(crate::application::editor::tools::text::TextState::test_buffer());
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    harness.focus_on(Some(harness.root_id()));

    harness.animate_ms(501);
    assert!(!harness.edit_root_widget(|root| root.widget.cursor_visible));

    harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
        state: KeyState::Down,
        key: Key::Named(NamedKey::ArrowLeft),
        code: Code::ArrowLeft,
        ..KeyboardEvent::default()
    }));
    assert!(harness.edit_root_widget(|root| root.widget.cursor_visible));
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.cursor_blink_elapsed_ns),
        0
    );
}

#[test]
fn outline_tools_use_the_active_sorts_origin() {
    let mut editor = widget();
    let mut text = crate::application::editor::tools::text::TextState::test_buffer();
    assert!(text.buffer.insert_character('B'));
    assert!(text.buffer.activate_sort(1));
    editor.text = Some(text);

    let origin = editor.active_sort_origin();
    assert_eq!(origin, Point::new(500.0, 0.0));
    assert_eq!(
        editor.glyph_affine() * Point::ORIGIN,
        editor.session.viewport.affine() * origin,
        "the editable glyph is drawn where its active sort sits"
    );
    let screen = editor.glyph_affine() * Point::new(40.0, 60.0);
    let design = editor.screen_to_glyph_design(screen);
    assert!((design.x - 40.0).abs() < 0.001);
    assert!((design.y - 60.0).abs() < 0.001);
}

#[test]
fn parked_text_does_not_consume_outline_tool_typing() {
    use masonry::core::Ime;
    use masonry::core::keyboard::{Code, KeyboardEvent, Modifiers};
    let mut editor = widget();
    editor.text = Some(crate::application::editor::tools::text::TextState::test_buffer());
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    harness.focus_on(Some(harness.root_id()));
    let typed = || {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Character("B".into()),
            code: Code::Unidentified,
            ..KeyboardEvent::default()
        })
    };
    let named = |key| {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Named(key),
            code: Code::Unidentified,
            ..KeyboardEvent::default()
        })
    };
    let shifted = |key| {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Named(key),
            code: Code::Unidentified,
            modifiers: Modifiers::SHIFT,
            ..KeyboardEvent::default()
        })
    };
    let command = |character: &'static str| {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Character(character.into()),
            code: Code::Unidentified,
            modifiers: Modifiers::META,
            ..KeyboardEvent::default()
        })
    };
    harness.process_text_event(typed());
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
        1
    );
    harness.edit_root_widget(|root| root.widget.tool = Tool::Text);
    harness.process_text_event(typed());
    let (event, _) = harness
        .pop_action::<EditorEvent>()
        .expect("ordinary keyboard text reports the complete buffer");
    assert!(
        matches!(event, EditorEvent::TextChanged(text) if text == "AB"),
        "logical character keys type directly, as Masonry, GPUI, and Web do"
    );
    let (event, _) = harness
        .pop_action::<EditorEvent>()
        .expect("typing reports the selected proof context separately");
    assert!(matches!(event, EditorEvent::TextProofSelection(Err(_))));
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
        2,
        "ordinary keyboard input does not depend on an IME commit"
    );
    harness.process_text_event(named(NamedKey::Backspace));
    let (event, _) = harness
        .pop_action::<EditorEvent>()
        .expect("deleting the direct-key test glyph reports the restored buffer");
    assert!(matches!(event, EditorEvent::TextChanged(text) if text == "A"));
    let (event, _) = harness
        .pop_action::<EditorEvent>()
        .expect("deleting refreshes the selected proof context");
    assert!(matches!(event, EditorEvent::TextProofSelection(Err(_))));
    harness.process_text_event(TextEvent::Ime(Ime::Preedit("B".into(), Some((0, 1)))));
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
        1,
        "preedit is visible state, not committed text"
    );
    harness.process_text_event(TextEvent::Ime(Ime::Preedit(String::new(), None)));
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().preedit.clone()),
        "",
        "empty preedit cancels composition"
    );
    harness.process_text_event(TextEvent::Ime(Ime::Preedit("B".into(), Some((0, 1)))));
    harness.process_text_event(TextEvent::Ime(Ime::Commit("B".into())));
    let (event, _) = harness
        .pop_action::<EditorEvent>()
        .expect("a committed edit reports its logical text");
    assert!(
        matches!(event, EditorEvent::TextChanged(text) if text == "AB"),
        "the view receives the complete committed buffer"
    );
    let (event, _) = harness
        .pop_action::<EditorEvent>()
        .expect("IME commit refreshes the selected proof context");
    assert!(matches!(event, EditorEvent::TextProofSelection(Err(_))));
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
        2
    );
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().preedit.clone()),
        "",
        "commit clears the composition preview"
    );
    harness.process_text_event(named(NamedKey::Enter));
    harness.process_text_event(TextEvent::Ime(Ime::Commit("A".into())));
    harness.process_text_event(named(NamedKey::ArrowUp));
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.cursor()),
        1,
        "up preserves the caret column on the preceding line"
    );
    harness.process_text_event(named(NamedKey::End));
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.cursor()),
        2
    );
    harness.process_text_event(named(NamedKey::Home));
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.cursor()),
        0
    );
    harness.process_text_event(named(NamedKey::ArrowDown));
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.cursor()),
        3,
        "down reaches the same column on the following line"
    );
    harness.process_text_event(shifted(NamedKey::End));
    assert_eq!(
        harness.edit_root_widget(|root| root
            .widget
            .text
            .as_ref()
            .unwrap()
            .buffer
            .selection_range()),
        Some(3..4),
        "Shift+End selects the final sort"
    );
    harness.process_text_event(named(NamedKey::Backspace));
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
        3,
        "Backspace removes the selection"
    );
    assert_eq!(
        harness.edit_root_widget(|root| root
            .widget
            .text
            .as_ref()
            .unwrap()
            .buffer
            .selection_range()),
        None
    );
    harness.process_text_event(TextEvent::Ime(Ime::Commit("A".into())));
    harness.edit_root_widget(|root| root.widget.tool = Tool::Select);
    harness.process_text_event(typed());
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
        4
    );
    harness.edit_root_widget(|root| {
        root.widget.tool = Tool::Text;
        let buffer = &mut root.widget.text.as_mut().unwrap().buffer;
        buffer.select_range(0, buffer.len());
    });
    harness.process_text_event(command("c"));
    assert_eq!(harness.clipboard_contents(), "AB\nA");
    harness.process_text_event(command("x"));
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
        0,
        "cut removes exactly the copied logical selection"
    );
    harness.process_text_event(TextEvent::ClipboardPaste("A\r\nB".into()));
    assert_eq!(
        harness.edit_root_widget(|root| {
            let buffer = &mut root.widget.text.as_mut().unwrap().buffer;
            buffer.select_range(0, buffer.len());
            buffer.selected_text()
        }),
        Some("A\nB".into()),
        "paste normalizes platform newlines and preserves text"
    );
}

#[test]
fn text_tool_consumes_space_as_text() {
    use masonry::core::keyboard::{Code, KeyboardEvent};
    let mut editor = widget();
    editor.tool = Tool::Text;
    editor.text = Some(crate::application::editor::tools::text::TextState::test_buffer());
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    harness.focus_on(Some(harness.root_id()));
    harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
        state: KeyState::Down,
        key: Key::Character(" ".into()),
        code: Code::Space,
        ..KeyboardEvent::default()
    }));

    let (event, _) = harness
        .pop_action::<EditorEvent>()
        .expect("the Text tool consumes Space and reports the buffer");
    assert!(matches!(event, EditorEvent::TextChanged(text) if text == "A "));
    let (event, _) = harness
        .pop_action::<EditorEvent>()
        .expect("space refreshes the selected proof context");
    assert!(matches!(event, EditorEvent::TextProofSelection(Err(_))));
}

#[test]
fn select_double_click_activates_the_composed_sort() {
    let mut editor = widget();
    let mut text = crate::application::editor::tools::text::TextState::test_buffer();
    assert!(text.buffer.insert_character('B'));
    editor.text = Some(text);
    editor.tool = Tool::Select;
    let mut harness =
        TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
    let at = harness.edit_root_widget(|root| {
        let editor = &root.widget;
        let text = editor.text.as_ref().unwrap();
        let layout = text.buffer.layout(text.line_height);
        let sort = &layout.items[1];
        editor.session.viewport.affine()
            * Point::new(sort.x + sort.advance_width / 2.0, sort.y + 300.0)
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

    let (event, _) = harness
        .pop_action::<EditorEvent>()
        .expect("double-clicking a sort reports the glyph to edit");
    assert!(matches!(
        event,
        EditorEvent::EditGlyph {
            name,
            tool: Tool::Select
        } if name == "B"
    ));
    let (event, _) = harness
        .pop_action::<EditorEvent>()
        .expect("double-click reports the shaped proof context after activating the glyph");
    assert!(matches!(event, EditorEvent::TextProofSelection(Err(_))));
    assert_eq!(
        harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.active_sort()),
        Some(1),
        "the clicked sort becomes the editable one"
    );
}
