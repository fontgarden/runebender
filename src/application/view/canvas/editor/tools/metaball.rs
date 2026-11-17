// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The metaball tool, and the metaball drags that the Select tool starts.

use super::super::*;
use masonry::core::KeyboardEvent;
use masonry::ui_events::pointer::PointerState;

/// Add or pick a metaball at the pointer and start dragging it.
pub(in super::super) fn press(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    at: Point,
    state: &PointerState,
) {
    ctx.request_focus();
    let at = editor.screen_to_glyph_design(at);
    let changed = editor.session.metaball_click(
        at,
        HIT_RADIUS_PX / editor.session.viewport.zoom,
        state.modifiers.shift(),
    );
    editor.drag = Drag::Metaballs { last: at, changed };
    editor.emit(ctx, false);
    ctx.request_render();
}

/// Move the dragged metaballs with the pointer.
pub(in super::super) fn drag(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    glyph_design: Point,
) {
    if let Drag::Metaballs { last, changed } = &mut editor.drag {
        let moved = editor.session.move_metaballs(glyph_design - *last, true);
        *changed |= moved;
        *last = glyph_design;
        if moved {
            // Publish the live draft to the proof strip and inspector.
            // Only release emits Edited and commits this gesture.
            editor.emit(ctx, false);
            ctx.request_render();
        }
    }
}

/// Commit or cancel the metaball drag.
pub(in super::super) fn release(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    cancelled: bool,
) {
    if let Drag::Metaballs { changed, .. } = &editor.drag {
        let changed = *changed;
        if cancelled {
            editor.session.cancel_metaball_drag();
        } else {
            editor.session.end_metaball_drag();
        }
        editor.drag = Drag::None;
        editor.emit(ctx, changed && !cancelled);
    }
}

/// Handle a key for the metaballs.
/// Returns true when the key was consumed.
pub(in super::super) fn key(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    key: &KeyboardEvent,
    cmd: bool,
    step: f64,
) -> bool {
    let edits_metaballs = editor.tool == Tool::Metaball
        || (editor.tool == Tool::Select
            && !editor.session.metaballs.selected.is_empty()
            && !editor.session.has_selection()
            && editor.session.selected_component.is_none());
    if edits_metaballs && editor.field.is_none() {
        let mut edited = false;
        let handled = match &key.key {
            Key::Character(c) if cmd && c.eq_ignore_ascii_case("a") => {
                editor.session.select_all_metaballs();
                true
            }
            Key::Named(NamedKey::Backspace | NamedKey::Delete) => {
                edited = editor.session.delete_metaballs();
                true
            }
            Key::Named(NamedKey::ArrowLeft) => {
                edited = editor
                    .session
                    .move_metaballs(kurbo::Vec2::new(-step, 0.0), false);
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                edited = editor
                    .session
                    .move_metaballs(kurbo::Vec2::new(step, 0.0), false);
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                edited = editor
                    .session
                    .move_metaballs(kurbo::Vec2::new(0.0, step), false);
                true
            }
            Key::Named(NamedKey::ArrowDown) => {
                edited = editor
                    .session
                    .move_metaballs(kurbo::Vec2::new(0.0, -step), false);
                true
            }
            Key::Named(NamedKey::Escape) => {
                editor.session.clear_metaball_selection();
                true
            }
            _ => false,
        };
        if handled {
            editor.emit(ctx, edited);
            ctx.request_render();
            ctx.set_handled();
            return true;
        }
    }
    false
}
