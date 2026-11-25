// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The select and lasso tools: points, anchors, components, the picture and the sidebearings.
//! A press of a tool without its own press handling also lands here.

use super::super::*;
use masonry::core::KeyboardEvent;
use masonry::ui_events::pointer::PointerState;

/// Apply a primary press of the Select tool, or of a tool without its own press.
pub(in super::super) fn press(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    at: Point,
    state: &PointerState,
) {
    let shift = state.modifiers.shift();
    if editor.tool == Tool::Select {
        let design = editor.screen_to_glyph_design(at);
        if editor.session.select_metaball_at(
            design,
            HIT_RADIUS_PX / editor.session.viewport.zoom,
            shift,
        ) {
            ctx.request_focus();
            editor.drag = Drag::Metaballs {
                last: design,
                changed: false,
            };
            editor.emit(ctx, false);
            ctx.request_render();
            return;
        }
    }
    // Sidebearing lines (only when not near a point).
    let affine = editor.glyph_affine();
    let segment = (editor.tool == Tool::Select
        && editor.hit_point(at).is_none()
        && editor.hit_part_point(at).is_none())
    .then(|| {
        runebender::outline::segment_ops::nearest_ordinary_layer_segment_with_t(
            editor.session.current_layer()?,
            editor.screen_to_glyph_design(at),
            HIT_RADIUS_PX / editor.session.viewport.zoom,
        )
    })
    .flatten();
    let adv_x = (affine * Point::new(editor.session.advance(), 0.0)).x;
    let lsb_x = (affine * Point::new(0.0, 0.0)).x;
    if segment.is_none()
        && editor.hit_point(at).is_none()
        && editor.hit_part_point(at).is_none()
        && !editor.session.neural
        && editor.session.host.is_none()
    {
        if (at.x - adv_x).abs() <= 4.0 {
            editor.drag = Drag::AdvanceLine;
            return;
        }
        if (at.x - lsb_x).abs() <= 4.0 {
            editor.drag = Drag::LeftLine { last_x: at.x };
            return;
        }
    }
    // An anchor selects and drags like a point, and wins over a point
    // under it.
    if let Some(anchor) = editor.session.anchor_at(
        editor.screen_to_glyph_design(at),
        HIT_RADIUS_PX / editor.session.viewport.zoom,
    ) {
        editor.session.press_anchor(anchor, shift);
        editor.session.begin_point_drag();
        editor.drag = Drag::Points { start: at };
        editor.emit(ctx, false);
        return;
    }
    if editor.tool == Tool::Select && state.modifiers.alt() && editor.hit_point(at).is_none() {
        let position = editor.screen_to_glyph_design(at);
        let radius = HIT_RADIUS_PX / editor.session.viewport.zoom;
        let changed = if shift {
            editor.session.add_quadratic_handle_at(position, radius)
        } else {
            editor.session.add_segment_handles_at(position, radius)
        };
        if changed {
            editor.emit(ctx, true);
            return;
        }
    }
    if editor.hit_point(at).is_none()
        && let Some((owner, point)) = editor.hit_part_point(at)
    {
        ctx.submit_action::<EditorEvent>(EditorEvent::EnterPart {
            owner,
            point: Some(point),
        });
        editor.drag = Drag::Enter { start: at };
        return;
    }
    match editor.hit_point(at) {
        // A double click on an on-curve point switches it between corner
        // and smooth, as in Glyphs.
        Some(id) if state.count >= 2 && editor.session.toggle_smooth(id) => {
            editor.session.replace_point_selection([id]);
            editor.emit(ctx, true);
        }
        Some(id) => {
            editor.session.press_point(id, shift);
            editor.session.begin_point_drag();
            editor.drag = Drag::Points { start: at };
            editor.emit(ctx, false);
        }
        None => {
            let design = editor.screen_to_glyph_design(at);
            if let Some((segment, _)) = segment {
                let ids = segment.point_ids();
                if !shift {
                    editor.session.select_only(SelectionTarget::Nothing);
                }
                if shift && ids.iter().all(|id| editor.session.selection.contains(id)) {
                    for id in ids {
                        editor.session.selection.remove(&id);
                    }
                } else {
                    editor.session.select_points(ids, true);
                }
                editor.session.begin_point_drag();
                editor.drag = Drag::Points { start: at };
                editor.emit(ctx, false);
                ctx.request_render();
                return;
            }
            if let Some(component) = editor.session.component_at(design) {
                editor
                    .session
                    .select_only(SelectionTarget::Component(component));
                editor.drag = Drag::Component { last: design };
                editor.emit(ctx, false);
                return;
            }
            if !shift && let Some(drag) = editor.image_press(design) {
                editor.session.select_only(SelectionTarget::Image);
                editor.drag = drag;
                editor.emit(ctx, false);
                ctx.request_render();
                return;
            }
            if !shift {
                editor.session.select_only(SelectionTarget::Nothing);
                editor.emit(ctx, false);
            }
            editor.drag = Drag::Marquee {
                start: at,
                current: at,
                additive: shift,
            };
        }
    }
}

/// Start a freehand selection boundary.
pub(in super::super) fn press_lasso(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    at: Point,
    state: &PointerState,
) {
    ctx.request_focus();
    editor.drag = Drag::Lasso {
        points: vec![at],
        additive: state.modifiers.shift(),
    };
}

/// Move what the press picked up, or extend the marquee or lasso.
pub(in super::super) fn drag(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    at: Point,
    glyph_design: Point,
    glyph_affine: Affine,
    current: &PointerState,
) {
    match &mut editor.drag {
        Drag::Image {
            start,
            origin,
            anchor,
            corner,
        } => {
            editor.image_preview = Some(EditorWidget::image_drag_transform(
                *start,
                *origin,
                *anchor,
                *corner,
                glyph_design,
            ));
            ctx.request_render();
        }
        Drag::AdvanceLine => {
            let d = glyph_design;
            editor.session.drag_advance(d.x.round());
            editor.follow_gesture();
            ctx.request_render();
        }
        Drag::LeftLine { last_x } => {
            // Only the whole units applied are consumed, so a slow drag
            // keeps its remainder for the next event.
            let zoom = editor.session.viewport.zoom;
            let dx = ((at.x - *last_x) / zoom).round();
            if dx != 0.0 {
                *last_x += dx * zoom;
                editor.session.shift_glyph(dx);
                editor.follow_gesture();
                ctx.request_render();
            }
        }
        Drag::Component { last } => {
            let design = glyph_design;
            let delta = design - *last;
            *last = design;
            if editor.session.drag_component_by(delta.x, delta.y) {
                editor.follow_gesture();
                ctx.request_render();
            }
        }
        Drag::Points { start } => {
            // Through the inverse, so a flipped or scaled component follows the
            // pointer too.
            let inverse = glyph_affine.inverse();
            let delta = inverse * at - inverse * *start;
            let total = (delta.x, delta.y);
            // Option moves on-curve points without their handles.
            let independent = current.modifiers.alt();
            if editor.session.drag_points_to(total, independent) {
                // Selected anchors move with the points, and marks in the line follow them.
                editor.follow_gesture();
                ctx.request_render();
            }
        }
        Drag::Marquee { current, .. } => {
            *current = at;
            ctx.request_render();
        }
        Drag::Lasso { points, .. } if points.last().is_none_or(|last| last.distance(at) >= 2.0) => {
            points.push(at);
            ctx.request_render();
        }
        _ => {}
    }
}

/// Commit or cancel the drag, or select what the marquee or lasso holds.
pub(in super::super) fn release(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    cancelled: bool,
) {
    match &editor.drag {
        Drag::Points { .. } => {
            if cancelled {
                editor.session.cancel_point_drag();
            } else {
                if editor.view.maintain_g2 {
                    editor.session.constrain_point_drag();
                }
                editor.session.end_point_drag();
            }
            editor.drag = Drag::None;
            editor.end_gesture(cancelled);
            editor.emit(ctx, !cancelled);
        }
        // A cancelled marquee leaves the selection as it was.
        Drag::Marquee { .. } if cancelled => {
            editor.drag = Drag::None;
            editor.emit(ctx, false);
        }
        Drag::Marquee {
            start,
            current,
            additive,
        } => {
            let rect = Rect::from_points(*start, *current);
            let additive = *additive;
            let affine = editor.glyph_affine();
            editor
                .session
                .select_within(SelectionShape::Rect(rect), affine, additive);
            // A locked picture cannot be moved, so a drag over it is a marquee;
            // a click on it still selects it, to inspect or unlock it.
            let clicked = rect.width().max(rect.height()) < 3.0;
            if clicked && !additive && !editor.session.has_selection() {
                let design = editor.screen_to_glyph_design(*start);
                let on_image = editor.image_frame(true).is_some_and(|(t, w, h)| {
                    (t * Rect::new(0.0, 0.0, w, h).to_path(0.1))
                        .bounding_box()
                        .contains(design)
                });
                editor.session.set_image_selected(on_image);
            }
            editor.drag = Drag::None;
            editor.emit(ctx, false);
        }
        Drag::Image { origin, .. } => {
            let origin = *origin;
            let moved = editor.image_preview.take();
            let changed = !cancelled
                && moved.is_some_and(|transform| {
                    transform != origin && editor.session.set_image_transform(transform)
                });
            editor.drag = Drag::None;
            editor.emit(ctx, changed);
        }
        Drag::Lasso { points, additive } => {
            if !cancelled {
                let additive = *additive;
                if points.len() >= 3 {
                    let mut path = kurbo::BezPath::new();
                    path.move_to(points[0]);
                    for point in &points[1..] {
                        path.line_to(*point);
                    }
                    path.close_path();
                    let affine = editor.glyph_affine();
                    editor
                        .session
                        .select_within(SelectionShape::Path(&path), affine, additive);
                } else {
                    let chosen = points.last().and_then(|point| editor.hit_point(*point));
                    editor.session.select_points(chosen, additive);
                }
                editor.emit(ctx, false);
            }
            editor.drag = Drag::None;
            ctx.request_render();
        }
        Drag::AdvanceLine | Drag::LeftLine { .. } => {
            if cancelled {
                editor.session.cancel_metric_drag();
            } else {
                editor.session.end_metric_drag();
            }
            editor.drag = Drag::None;
            editor.end_gesture(cancelled);
            editor.emit(ctx, !cancelled);
        }
        Drag::Enter { .. } => editor.drag = Drag::None,
        Drag::Component { .. } => {
            if cancelled {
                editor.session.cancel_component_drag();
            } else {
                editor.session.end_component_drag();
            }
            editor.drag = Drag::None;
            editor.end_gesture(cancelled);
            editor.emit(ctx, !cancelled);
        }
        _ => {}
    }
}

/// Handle a key while the layer picture is selected.
/// Returns true when the key was consumed.
pub(in super::super) fn image_key(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    key: &KeyboardEvent,
    cmd: bool,
    step: f64,
) -> bool {
    if editor.tool == Tool::Select
        && editor.session.image_selected
        && editor.session.selection.is_empty()
        && editor.field.is_none()
        && !cmd
        && let Some(transform) = editor.session.layer_image().map(|image| image.transform())
    {
        let nudge = |dx: f64, dy: f64| Affine::translate((dx, dy)) * transform;
        let locked = editor.session.image_locked();
        let (handled, edited) = match &key.key {
            Key::Named(NamedKey::Escape) => {
                editor.session.set_image_selected(false);
                (true, false)
            }
            // A locked picture can be selected and nothing more.
            _ if locked => (false, false),
            Key::Named(NamedKey::ArrowLeft) => {
                (true, editor.session.set_image_transform(nudge(-step, 0.0)))
            }
            Key::Named(NamedKey::ArrowRight) => {
                (true, editor.session.set_image_transform(nudge(step, 0.0)))
            }
            Key::Named(NamedKey::ArrowUp) => {
                (true, editor.session.set_image_transform(nudge(0.0, step)))
            }
            Key::Named(NamedKey::ArrowDown) => {
                (true, editor.session.set_image_transform(nudge(0.0, -step)))
            }
            Key::Named(NamedKey::Backspace | NamedKey::Delete) => {
                (true, editor.session.remove_image())
            }
            _ => (false, false),
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
