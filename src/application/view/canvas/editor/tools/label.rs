// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The label tool: lasso polygons and their corners, which label the ink of a neural item.

use super::super::*;
use masonry::core::KeyboardEvent;
use masonry::ui_events::pointer::PointerState;

/// A drag of the label tool.
/// The session holds where a moved corner or polygon is shown.
pub(in super::super) enum LabelDrag {
    /// A press in screen space.
    /// A loop becomes a polygon; a click lets go of the picked shape.
    Freehand { points: Vec<Point> },
    /// A polygon corner of a label region follows the pointer.
    Corner {
        region: usize,
        /// Option is held: the corner is being pulled away from others, so it does not snap
        /// back onto a neighbor's corner.
        detach: bool,
    },
    /// Shift-drag: a box that selects the corners inside it, in design space.
    Marquee { start: Point, current: Point },
    /// Several label corners follow the pointer.
    Corners { start: Point },
    /// A whole lasso polygon follows the pointer.
    Region { region: usize, start: Point },
}

/// Apply a press of the label tool and start the drag it names.
pub(in super::super) fn press(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    at: Point,
    state: &PointerState,
) {
    ctx.request_focus();
    let design = editor.screen_to_glyph_design(at);
    let reach = HIT_RADIUS_PX / editor.session.viewport.zoom;
    let modifiers = LabelModifiers {
        shift: state.modifiers.shift(),
        alt: state.modifiers.alt(),
    };
    match editor.session.label_press(design, reach, modifiers) {
        LabelPress::Selected => editor.emit(ctx, false),
        LabelPress::Corner { region, detach } => {
            editor.drag = Drag::Label(LabelDrag::Corner { region, detach });
            ctx.capture_pointer();
            editor.emit(ctx, false);
        }
        LabelPress::NewCorner { region } => {
            editor.drag = Drag::Label(LabelDrag::Corner {
                region,
                detach: false,
            });
            ctx.capture_pointer();
            editor.emit(ctx, true);
        }
        LabelPress::Corners { start } => {
            editor.drag = Drag::Label(LabelDrag::Corners { start });
            ctx.capture_pointer();
            editor.emit(ctx, false);
        }
        LabelPress::Region { region, start } => {
            editor.drag = Drag::Label(LabelDrag::Region { region, start });
            ctx.capture_pointer();
            editor.emit(ctx, false);
        }
        LabelPress::Marquee { start } => {
            editor.drag = Drag::Label(LabelDrag::Marquee {
                start,
                current: start,
            });
            ctx.capture_pointer();
        }
        LabelPress::Freehand => editor.drag = Drag::Label(LabelDrag::Freehand { points: vec![at] }),
        LabelPress::Ignored => {}
    }
    ctx.request_render();
}

/// Update what the pointer is over, for the glow, the cursor and the caption.
pub(in super::super) fn hover(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    glyph_design: Point,
) {
    if editor.tool == Tool::Label && matches!(editor.drag, Drag::None) {
        use crate::application::editor::tools::label::LabelHit;
        let hit = editor
            .session
            .label_hit(glyph_design, HIT_RADIUS_PX / editor.session.viewport.zoom);
        let hover = (hit != LabelHit::Nothing).then_some(hit);
        if hover != editor.label_hover {
            editor.label_hover = hover;
            ctx.request_render();
        }
    }
}

/// Move the dragged corner or polygon, or extend the freehand path.
pub(in super::super) fn drag(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    at: Point,
    glyph_design: Point,
) {
    match &mut editor.drag {
        Drag::Label(LabelDrag::Corner { region, detach }) => {
            let reach = HIT_RADIUS_PX / editor.session.viewport.zoom;
            if editor
                .session
                .drag_label_corner(*region, *detach, glyph_design, reach)
            {
                ctx.request_render();
            }
        }
        Drag::Label(LabelDrag::Corners { start }) => {
            editor.session.drag_label_corners(*start, glyph_design);
            ctx.request_render();
        }
        Drag::Label(LabelDrag::Marquee { current, .. }) => {
            *current = glyph_design;
            ctx.request_render();
        }
        Drag::Label(LabelDrag::Region { region, start }) => {
            editor
                .session
                .drag_label_region(*region, *start, glyph_design);
            ctx.request_render();
        }
        Drag::Label(LabelDrag::Freehand { points })
            if points.last().is_none_or(|last| last.distance(at) >= 2.0) =>
        {
            points.push(at);
            ctx.request_render();
        }
        _ => {}
    }
}

/// Finish a drag of the label tool.
pub(in super::super) fn release(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    cancelled: bool,
) {
    match &editor.drag {
        Drag::Label(LabelDrag::Marquee { start, current }) => {
            let rect = Rect::from_points(*start, *current);
            if !cancelled {
                editor.session.select_label_corners_in(rect);
            }
            editor.drag = Drag::None;
            editor.emit(ctx, false);
        }
        Drag::Label(LabelDrag::Corner { .. })
        | Drag::Label(LabelDrag::Corners { .. })
        | Drag::Label(LabelDrag::Region { .. }) => {
            let changed = editor.session.finish_label_move(!cancelled);
            editor.drag = Drag::None;
            editor.emit(ctx, changed);
        }
        Drag::Label(LabelDrag::Freehand { points }) => {
            let mut changed = false;
            if !cancelled {
                let design: Vec<Point> = points
                    .iter()
                    .map(|p| editor.screen_to_glyph_design(*p))
                    .collect();
                let travelled = points
                    .iter()
                    .fold(Rect::from_points(points[0], points[0]), |r, p| {
                        r.union_pt(*p)
                    });
                let moved = travelled.width().max(travelled.height()) > 4.0;
                let near = 12.0 / editor.session.viewport.zoom;
                changed = editor.session.finish_label_freehand(&design, moved, near);
            }
            editor.drag = Drag::None;
            editor.label_hover = None;
            editor.emit(ctx, changed);
        }
        _ => {}
    }
}

/// Handle a key for the label tool.
/// Returns true when the key was consumed.
pub(in super::super) fn key(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    key: &KeyboardEvent,
    cmd: bool,
    shift: bool,
) -> bool {
    if editor.tool == Tool::Label && editor.field.is_none() && !cmd {
        let mut edited = false;
        let handled = match &key.key {
            Key::Named(NamedKey::Tab) if shift => {
                editor.session.step_label_letter(-1);
                true
            }
            Key::Named(NamedKey::Tab | NamedKey::Enter) => {
                editor.session.next_label_letter();
                true
            }
            Key::Named(NamedKey::Backspace | NamedKey::Delete) => {
                edited = editor.session.delete_label_selection();
                true
            }
            Key::Named(
                arrow @ (NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::ArrowUp
                | NamedKey::ArrowDown),
            ) if editor.session.label.selected.is_some() => {
                // One grid step, or eight with Shift.
                use crate::application::editor::tools::label::LABEL_GRID;
                let step = LABEL_GRID * if shift { 8.0 } else { 1.0 };
                let by = match arrow {
                    NamedKey::ArrowLeft => kurbo::Vec2::new(-step, 0.0),
                    NamedKey::ArrowRight => kurbo::Vec2::new(step, 0.0),
                    NamedKey::ArrowUp => kurbo::Vec2::new(0.0, step),
                    _ => kurbo::Vec2::new(0.0, -step),
                };
                edited = editor.session.nudge_label_selection(by);
                true
            }
            Key::Named(NamedKey::Escape) if editor.session.label.selected.is_some() => {
                editor.session.clear_label_selection();
                true
            }
            Key::Named(NamedKey::Escape) if editor.session.label.sample.is_some() => {
                editor.session.select_sample(None);
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
