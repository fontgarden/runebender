// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The pen and the hyperbezier pen: clicks add points, and a drag pulls out handles.

use super::super::*;
use masonry::ui_events::pointer::PointerState;

/// Apply a press of the pen.
pub(in super::super) fn press(editor: &mut EditorWidget, ctx: &mut EventCtx<'_>, at: Point) {
    let affine = editor.glyph_affine();
    let near_first = editor
        .session
        .pen_first_point()
        .map(|p| (affine * p).distance(at) <= HIT_RADIUS_PX)
        .unwrap_or(false);
    let on_segment = !editor.session.pen_is_active()
        && editor.hit_point(at).is_none()
        && editor.session.insert_point_at(
            editor.screen_to_glyph_design(at),
            HIT_RADIUS_PX / editor.session.viewport.zoom,
        );
    if near_first && editor.session.pen_is_active() {
        editor.session.pen_close();
        editor.drag = Drag::None;
        editor.emit(ctx, true);
    } else if on_segment {
        editor.insert_preview = None;
        // A click on an outline adds a point to that
        // segment rather than starting a new contour.
        editor.drag = Drag::None;
        editor.emit(ctx, true);
        ctx.request_render();
    } else {
        let origin = editor.screen_to_glyph_design(at);
        let (point_count, active_contour) = editor.session.pen_checkpoint();
        editor.drag = Drag::Pen {
            origin,
            dragging: false,
            point_count,
            active_contour,
        };
    }
}

/// Apply a press of the hyperbezier pen.
pub(in super::super) fn press_hyper(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    at: Point,
    state: &PointerState,
) {
    let affine = editor.glyph_affine();
    let near_first = editor
        .session
        .first_contour_point()
        .map(|p| (affine * p).distance(at) <= HIT_RADIUS_PX)
        .unwrap_or(false);
    let corner = state.modifiers.alt();
    if near_first && editor.session.hyper_is_active() {
        editor.session.hyper_close();
    } else {
        let d = editor.screen_to_glyph_design(at);
        editor.session.hyper_add(d.x, d.y, corner);
    }
    editor.emit(ctx, true);
}

/// Update the pen preview segment and the point a click would add to a segment.
pub(in super::super) fn hover(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    at: Point,
    glyph_design: Point,
) {
    if matches!(editor.tool, Tool::Pen | Tool::HyperPen) {
        editor.hover = Some(glyph_design);
        if editor.session.contour_drawing_is_active() {
            ctx.request_render();
        }
    }
    let insert_preview = (editor.tool == Tool::Pen
        && matches!(editor.drag, Drag::None)
        && !editor.session.pen_is_active()
        && editor.hit_point(at).is_none())
    .then(|| {
        editor
            .session
            .segment_insert_at(glyph_design, HIT_RADIUS_PX / editor.session.viewport.zoom)
    })
    .flatten()
    .map(|insert| (insert.point, insert.curve));
    if insert_preview != editor.insert_preview {
        editor.insert_preview = insert_preview;
        ctx.request_render();
    }
}

/// Pull out the handles of the new point once the drag passes a threshold.
pub(in super::super) fn drag(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    at: Point,
    glyph_design: Point,
    glyph_affine: Affine,
) {
    if let Drag::Pen {
        origin, dragging, ..
    } = &mut editor.drag
    {
        let origin = *origin;
        let to = glyph_design;
        let moved_px = (glyph_affine * origin).distance(at);
        if *dragging {
            editor.session.pen_smooth_drag(origin, to);
            ctx.request_render();
        } else if moved_px > 4.0 {
            if let Drag::Pen { dragging, .. } = &mut editor.drag {
                *dragging = true;
            }
            editor.session.pen_smooth_begin(origin, to);
            ctx.request_render();
        }
    }
}

/// Finish the pen gesture: a click adds a corner point.
pub(in super::super) fn release(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    cancelled: bool,
) {
    if let Drag::Pen {
        origin,
        dragging,
        point_count,
        active_contour,
    } = &editor.drag
    {
        if cancelled {
            editor
                .session
                .cancel_pen_gesture(*point_count, *active_contour);
        } else if !dragging {
            editor.session.pen_corner(origin.x, origin.y);
        }
        editor.drag = Drag::None;
        editor.emit(ctx, !cancelled);
    }
}
