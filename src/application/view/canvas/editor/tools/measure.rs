// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The measure tool: a scan line under the pointer, and a ruler that a drag lays down.

use super::super::pointer::constrain_to_45;
use super::super::*;
use masonry::ui_events::pointer::PointerState;

/// Start a ruler at the pointer.
pub(in super::super) fn press(editor: &mut EditorWidget, ctx: &mut EventCtx<'_>, at: Point) {
    ctx.request_focus();
    let start = editor.measure_snap(at);
    editor.ruler = Some(Line::new(start, start));
    editor.drag = Drag::Measure { start };
    ctx.request_render();
}

/// Move the scan line to the pointer.
pub(in super::super) fn hover(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    glyph_design: Point,
    current: &PointerState,
) {
    if editor.tool == Tool::Measure {
        editor.hover = Some(glyph_design);
        editor.hover_vertical = current.modifiers.alt();
        ctx.request_render();
    }
}

/// Move the free end of the ruler.
pub(in super::super) fn drag(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    at: Point,
    glyph_design: Point,
    current: &PointerState,
) {
    if let Drag::Measure { start } = &mut editor.drag {
        let start = *start;
        let end = if current.modifiers.shift() {
            constrain_to_45(start, glyph_design)
        } else {
            editor.measure_snap(at)
        };
        editor.ruler = Some(Line::new(start, end));
        ctx.request_render();
    }
}

/// Finish the ruler: a click without a drag clears it.
pub(in super::super) fn release(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    cancelled: bool,
) {
    if let Drag::Measure { .. } = &editor.drag {
        // A click without a drag clears the ruler and brings back the scan line.
        if cancelled || editor.ruler.is_some_and(|ruler| ruler.length() < 1.0) {
            editor.ruler = None;
        }
        editor.drag = Drag::None;
        ctx.request_render();
    }
}
