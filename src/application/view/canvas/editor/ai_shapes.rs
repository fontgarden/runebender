// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The box of the local AI shapes tool: its resize handles, keys and painting.

use super::*;

/// The eight resize handles of a shapes box in screen space: which
/// sides each moves, in design terms, and where it is drawn.
fn ai_handles(screen: Rect) -> [((i8, i8), Point); 8] {
    let (cx, cy) = (screen.center().x, screen.center().y);
    // Screen y grows down and design y grows up, so the top edge on
    // screen is the box's y1.
    [
        ((-1, 1), Point::new(screen.x0, screen.y0)),
        ((0, 1), Point::new(cx, screen.y0)),
        ((1, 1), Point::new(screen.x1, screen.y0)),
        ((-1, 0), Point::new(screen.x0, cy)),
        ((1, 0), Point::new(screen.x1, cy)),
        ((-1, -1), Point::new(screen.x0, screen.y1)),
        ((0, -1), Point::new(cx, screen.y1)),
        ((1, -1), Point::new(screen.x1, screen.y1)),
    ]
}

/// The side of a resize handle, at rest and when hovered or dragged.
const AI_HANDLE_PX: f64 = 7.0;
const AI_HANDLE_HOT_PX: f64 = 9.0;

impl EditorWidget {
    /// Type a key into the focused metric box. Returns whether the glyph
    /// changed, and whether the key was ours.
    /// The local AI shapes tool: a box was dragged. Keep it, open the
    /// word field under it, and tell the app the old ghost is stale.
    pub(super) fn begin_ai_box(&mut self, ctx: &mut EventCtx<'_>, s0: Point, c0: Point) {
        let rect = Rect::from_points(s0, c0);
        if rect.width() < 8.0 || rect.height() < 8.0 {
            return;
        }
        let rect = Rect::new(
            rect.x0.round(),
            rect.y0.round(),
            rect.x1.round(),
            rect.y1.round(),
        );
        self.ai_box = Some(rect);
        self.ai_ghost = None;
        ctx.submit_action::<EditorEvent>(EditorEvent::AiShapeBox { rect });
        ctx.request_focus();
    }

    /// Which edge or corner of the shapes box is under `at`, in screen
    /// space, as the sides that a drag there moves.
    pub(super) fn ai_box_handle(&self, at: Point) -> Option<(i8, i8)> {
        let rect = self.ai_box?;
        let screen = self.glyph_affine().transform_rect_bbox(rect);
        ai_handles(screen)
            .into_iter()
            .find(|(_, center)| center.distance(at) <= HIT_RADIUS_PX)
            .map(|(handle, _)| handle)
    }

    /// Move the sides named by `handle` to `to`, keeping the box at
    /// least a few units wide and tall.
    pub(super) fn ai_resize(&mut self, handle: (i8, i8), to: Point) {
        let Some(mut rect) = self.ai_box else {
            return;
        };
        const MIN: f64 = 8.0;
        match handle.0 {
            -1 => rect.x0 = to.x.min(rect.x1 - MIN),
            1 => rect.x1 = to.x.max(rect.x0 + MIN),
            _ => {}
        }
        match handle.1 {
            -1 => rect.y0 = to.y.min(rect.y1 - MIN),
            1 => rect.y1 = to.y.max(rect.y0 + MIN),
            _ => {}
        }
        self.ai_box = Some(rect);
    }

    /// Keys while the shapes tool has a box: Enter keeps the ghost, Tab
    /// asks for another, Esc drops the box.
    pub(super) fn ai_key(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &masonry::core::KeyboardEvent,
    ) -> bool {
        let Some(rect) = self.ai_box else {
            return false;
        };
        match &key.key {
            Key::Named(NamedKey::Escape) => {
                self.ai_box = None;
                self.ai_hover = None;
                self.ai_ghost = None;
                ctx.submit_action::<EditorEvent>(EditorEvent::AiShapeDrop);
                true
            }
            Key::Named(NamedKey::Enter) => {
                if let Some(ghost) = self.ai_ghost.clone() {
                    self.ai_box = None;
                    self.ai_ghost = None;
                    ctx.submit_action::<EditorEvent>(EditorEvent::AiShapeKept { ghost });
                } else {
                    ctx.submit_action::<EditorEvent>(EditorEvent::AiShapeDraw { rect, seed: 0 });
                }
                true
            }
            Key::Named(NamedKey::Tab) => {
                let seed = self.ai_ghost.as_ref().map_or(1, |g| g.seed + 1);
                ctx.submit_action::<EditorEvent>(EditorEvent::AiShapeDraw { rect, seed });
                true
            }
            _ => false,
        }
    }

    /// The shapes tool over the drawing: the box, the ghost the model
    /// drew, the word field under the box, and one line of status.
    pub(super) fn paint_ai_shape(&self, painter: &mut Painter<'_>, affine: Affine, pal: &Palette) {
        if self.tool != Tool::AiShape || self.preview_mode {
            return;
        }
        let Some(rect) = self.ai_box else {
            return;
        };
        let accent = pal.tool_feedback();
        let screen = affine.transform_rect_bbox(rect);
        painter
            .stroke(
                screen,
                &Stroke::new(pal.stroke).with_dashes(0.0, [6.0, 4.0]),
                accent,
            )
            .draw();
        if let Some(ghost) = &self.ai_ghost {
            // The ghost fills the box it was drawn for; while the box is
            // being resized it is stretched to the new one.
            let bounds = ghost.path.bounding_box();
            let fit = if bounds.width() > 0.0 && bounds.height() > 0.0 {
                Affine::translate((rect.x0, rect.y0))
                    * Affine::scale_non_uniform(
                        rect.width() / bounds.width(),
                        rect.height() / bounds.height(),
                    )
                    * Affine::translate((-bounds.x0, -bounds.y0))
            } else {
                Affine::IDENTITY
            };
            painter
                .fill(&ghost.path, accent.with_alpha(0.35))
                .transform(affine * fit)
                .draw();
            painter
                .stroke(&ghost.path, &Stroke::new(1.5), accent)
                .transform(affine * fit)
                .draw();
        }
        // The size, so the box reads in font units while it is dragged.
        text_label::draw(
            painter,
            Point::new(screen.x0, screen.y0 - 6.0),
            &format!("{} × {}", rect.width().round(), rect.height().round()),
            TextSize::Caption.px(),
            pal.text_muted,
            Anchor::Start,
        );
        // The resize handles: outlined at rest, filled when hovered,
        // filled and larger while dragged.
        let active = match self.drag {
            Drag::AiResize { handle } => Some(handle),
            _ => None,
        };
        for (handle, center) in ai_handles(screen) {
            let hot = active == Some(handle) || (active.is_none() && self.ai_hover == Some(handle));
            let side = if active == Some(handle) {
                AI_HANDLE_HOT_PX
            } else {
                AI_HANDLE_PX
            };
            let square = Rect::from_center_size(center, (side, side));
            let fill = if hot { accent } else { pal.canvas };
            painter.fill(square.to_path(0.1), fill).draw();
            painter
                .stroke(square, &Stroke::new(pal.stroke), accent)
                .draw();
        }
    }
}
