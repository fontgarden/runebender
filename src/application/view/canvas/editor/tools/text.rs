// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The text tool: caret placement, typing, IME composition and the clipboard.

use super::super::*;
use masonry::core::KeyboardEvent;

/// Handle IME composition and paste, and restart the caret blink.
/// Returns true when the event was consumed.
pub(in super::super) fn input(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    event: &TextEvent,
) -> bool {
    if editor.tool == Tool::Text && editor.text.is_some() {
        editor.cursor_blink_elapsed_ns = 0;
        editor.cursor_visible = true;
        ctx.request_anim_frame();
    }
    if editor.tool == Tool::Text
        && let Some(text) = editor.text.as_mut()
        && let TextEvent::Ime(ime) = event
    {
        let changed = match ime {
            masonry::core::Ime::Preedit(value, _) => {
                text.set_preedit(value.clone());
                false
            }
            masonry::core::Ime::Commit(value) => text.commit_preedit(value),
            masonry::core::Ime::Disabled => {
                text.set_preedit(String::new());
                false
            }
            masonry::core::Ime::Enabled => false,
        };
        if changed {
            editor.emit_text_changed(ctx);
        }
        // Composition can change the visible run even when the commit
        // contains no glyph the document can place. Refit after every
        // IME transition so cancelling or rejecting a preedit cannot
        // leave the viewport sized for stale composition text.
        editor.fit_text();
        ctx.request_render();
        ctx.set_handled();
        return true;
    }
    if editor.tool == Tool::Text
        && let Some(text) = editor.text.as_mut()
        && let TextEvent::ClipboardPaste(value) = event
    {
        if text.commit_preedit(value) {
            editor.fit_text();
            ctx.request_render();
            editor.emit_text_changed(ctx);
        }
        ctx.set_handled();
        return true;
    }
    false
}

/// Handle Command-C, Command-X and Command-A in the text line.
/// Returns true when the key was consumed.
pub(in super::super) fn command_key(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    key: &KeyboardEvent,
    cmd: bool,
) -> bool {
    if editor.tool == Tool::Text
        && let Some(text) = editor.text.as_mut()
        && cmd
        && let Key::Character(character) = &key.key
    {
        if character.eq_ignore_ascii_case("c") || character.eq_ignore_ascii_case("x") {
            if let Some(selected) = text.buffer.selected_text().filter(|text| !text.is_empty()) {
                ctx.set_clipboard(selected);
                if character.eq_ignore_ascii_case("x") {
                    text.buffer.delete_after_cursor();
                    text.buffer.shape_arabic_if_rtl();
                    editor.fit_text();
                    ctx.request_render();
                    editor.emit_text_changed(ctx);
                }
            }
            ctx.set_handled();
            return true;
        }
        if character.eq_ignore_ascii_case("a") {
            text.buffer.select_range(0, text.buffer.len());
            editor.emit_text_proof_selection(ctx);
            ctx.request_render();
            ctx.set_handled();
            return true;
        }
    }
    false
}

/// Type into the text line.
/// Returns true for every key without Command while the text tool holds a composition.
pub(in super::super) fn typing_key(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    key: &KeyboardEvent,
    cmd: bool,
    shift: bool,
) -> bool {
    // The text tool types. Everything a key would otherwise do to
    // the outline is off while it is in hand, because a person
    // typing "n" means the letter, not the pen.
    if editor.tool == Tool::Text
        && let Some(text) = editor.text.as_mut()
        && !cmd
    {
        let (handled, changed_text) = match &key.key {
            // Masonry, GPUI, and Web all deliver ordinary typing as a
            // logical character key. IME commits remain a separate path
            // for composed text.
            Key::Character(value) => {
                let value: String = value.chars().filter(|c| !c.is_control()).collect();
                let changed = !value.is_empty() && text.commit_preedit(&value);
                (true, changed)
            }
            Key::Named(NamedKey::Backspace) => {
                let changed = text.buffer.delete_before_cursor().is_some();
                if changed {
                    text.buffer.shape_arabic_if_rtl();
                }
                (changed, changed)
            }
            Key::Named(NamedKey::Delete) => {
                let changed = text.buffer.delete_after_cursor().is_some();
                if changed {
                    text.buffer.shape_arabic_if_rtl();
                }
                (changed, changed)
            }
            Key::Named(NamedKey::Enter) => {
                text.buffer.insert_line_break();
                (true, true)
            }
            Key::Named(NamedKey::ArrowLeft) => {
                if shift {
                    text.buffer.extend_selection_visual_left();
                } else {
                    text.buffer.move_cursor_visual_left();
                }
                (true, false)
            }
            Key::Named(NamedKey::ArrowRight) => {
                if shift {
                    text.buffer.extend_selection_visual_right();
                } else {
                    text.buffer.move_cursor_visual_right();
                }
                (true, false)
            }
            Key::Named(NamedKey::ArrowUp) => {
                if shift {
                    text.buffer
                        .extend_selection_vertically(-1, text.line_height);
                } else {
                    text.buffer.move_cursor_vertically(-1, text.line_height);
                }
                (true, false)
            }
            Key::Named(NamedKey::ArrowDown) => {
                if shift {
                    text.buffer.extend_selection_vertically(1, text.line_height);
                } else {
                    text.buffer.move_cursor_vertically(1, text.line_height);
                }
                (true, false)
            }
            Key::Named(NamedKey::Home) => {
                if shift {
                    text.buffer.extend_selection_to_line_edge(false);
                } else {
                    text.buffer.move_cursor_to_line_edge(false);
                }
                (true, false)
            }
            Key::Named(NamedKey::End) => {
                if shift {
                    text.buffer.extend_selection_to_line_edge(true);
                } else {
                    text.buffer.move_cursor_to_line_edge(true);
                }
                (true, false)
            }
            _ => (false, false),
        };
        if handled {
            // A longer line needs more room; refit only while the
            // caret is at the end, so it does not fight a person who
            // has zoomed in on a pair.
            let at_end = text.buffer.cursor() == text.buffer.len();
            if at_end {
                editor.fit_text();
            }
            if changed_text {
                editor.emit_text_changed(ctx);
            } else {
                editor.emit_text_proof_selection(ctx);
            }
            ctx.request_render();
            ctx.set_handled();
        }
        return true;
    }
    false
}
