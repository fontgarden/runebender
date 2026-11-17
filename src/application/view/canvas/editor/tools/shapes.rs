// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The rectangle, ellipse, knife and AI shapes tools: a drag spans a box or a line.

use super::super::*;
use masonry::core::KeyboardEvent;

/// Start a drag that spans a shape or a knife line.
pub(in super::super) fn press(editor: &mut EditorWidget, ctx: &mut EventCtx<'_>, at: Point) {
    ctx.request_focus();
    ctx.capture_pointer();
    let d = editor.screen_to_glyph_design(at);
    editor.drag = Drag::Shape {
        start: d,
        current: d,
    };
}

/// Start resizing the AI shapes box from the edge or corner `handle`.
pub(in super::super) fn press_resize_handle(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    handle: (i8, i8),
) {
    ctx.request_focus();
    ctx.capture_pointer();
    editor.drag = Drag::AiResize { handle };
}

/// Update the hovered resize handle of the AI shapes box.
pub(in super::super) fn hover(editor: &mut EditorWidget, ctx: &mut EventCtx<'_>, at: Point) {
    if editor.tool == Tool::AiShape && matches!(editor.drag, Drag::None) {
        let hover = editor.ai_box_handle(at);
        if hover != editor.ai_hover {
            editor.ai_hover = hover;
            ctx.request_render();
        }
    }
}

/// Move the free end of the shape, or the dragged side of the AI shapes box.
pub(in super::super) fn drag(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    glyph_design: Point,
) {
    match &mut editor.drag {
        Drag::Shape { current, .. } => {
            *current = glyph_design;
            ctx.request_render();
        }
        Drag::AiResize { handle } => {
            let handle = *handle;
            editor.ai_resize(handle, glyph_design);
            ctx.request_render();
        }
        _ => {}
    }
}

/// Add the shape, cut along the knife line, or settle the AI shapes box.
pub(in super::super) fn release(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    cancelled: bool,
) {
    match &editor.drag {
        Drag::Shape { start, current } => {
            if !cancelled {
                let (s0, c0) = (*start, *current);
                match editor.tool {
                    Tool::Rect => editor.session.add_rect(s0.x, s0.y, c0.x, c0.y),
                    Tool::Ellipse => {
                        editor.session.add_ellipse(s0.x, s0.y, c0.x, c0.y);
                    }
                    Tool::Knife => {
                        editor.session.knife_cut(s0, c0);
                    }
                    Tool::AiShape => {
                        editor.begin_ai_box(ctx, s0, c0);
                        editor.drag = Drag::None;
                        ctx.request_render();
                        return;
                    }
                    _ => {}
                }
            }
            editor.drag = Drag::None;
            editor.emit(ctx, !cancelled);
        }
        Drag::AiResize { .. } => {
            editor.drag = Drag::None;
            if !cancelled && let Some(rect) = editor.ai_box {
                let rect = Rect::new(
                    rect.x0.round(),
                    rect.y0.round(),
                    rect.x1.round(),
                    rect.y1.round(),
                );
                editor.ai_box = Some(rect);
                // The shape is drawn again for the new box; until
                // it arrives the old ghost is shown stretched.
                ctx.submit_action::<EditorEvent>(EditorEvent::AiShapeBox { rect });
            }
            ctx.request_render();
        }
        _ => {}
    }
}

/// Handle a key typed into the field of the AI shapes box.
/// Returns true when the key was consumed.
pub(in super::super) fn key(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    key: &KeyboardEvent,
) -> bool {
    if editor.tool == Tool::AiShape
        && editor.ai_box.is_some()
        && !(key.modifiers.meta() || key.modifiers.ctrl())
        && editor.ai_key(ctx, key)
    {
        ctx.request_render();
        ctx.set_handled();
        return true;
    }
    false
}
