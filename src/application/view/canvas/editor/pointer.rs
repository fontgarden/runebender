// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Pointer handling of the editor widget: presses, drags, releases and scrolling for every tool.

use super::tools::{hand, label, measure, metaball, pen, select, shapes, sketch, text};
use super::*;

impl EditorWidget {
    fn update_kern_hover(&mut self, ctx: &mut EventCtx<'_>, at: Point, alt: bool) {
        let design = self.session.viewport.screen_to_design(at);
        let over_card = self
            .metrics_panel_origin()
            .is_some_and(|(x, y)| Rect::new(x, y, x + PANEL_WIDTH, y + PANEL_HEIGHT).contains(at));
        let hint = self
            .text
            .as_ref()
            .filter(|_| !over_card && self.interp.is_none())
            .and_then(|t| {
                let index = t.kern_hit(design)?;
                Some((index, t.buffer.drag_pair(index)?))
            })
            .map(|(index, (a, b, _, rtl, _))| {
                (index, {
                    if rtl {
                        format!("{b} · {a}")
                    } else {
                        format!("{a} · {b}")
                    }
                })
            });
        if self.kern_hover != hint || self.kern_alt_down != alt {
            self.kern_alt_down = alt;
            self.kern_hover = hint;
            ctx.request_cursor_icon_change();
            ctx.request_render();
        }
    }

    /// Handle a pointer event.
    /// [`Widget::on_pointer_event`] calls this.
    pub(super) fn handle_pointer_event(&mut self, ctx: &mut EventCtx<'_>, event: &PointerEvent) {
        if self.tool == Tool::Interpolation && self.interpolation_pointer(ctx, event) {
            return;
        }
        // Off a master the view is a read-only instance: swallow edit clicks.
        if self.interp.is_some()
            && let PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary | PointerButton::Secondary),
                ..
            }) = event
        {
            ctx.set_handled();
            return;
        }
        // A click in a metric box goes to the box, not to the drawing
        // underneath it. The panel is painted, so nothing else will do
        // this for us: a composed panel would have taken the click by
        // being in front.
        if let PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            state,
            ..
        }) = event
        {
            let at = ctx.local_position(state.position);
            if let Some(boxes) = self.metric_boxes()
                && let Some((field, _)) = boxes.iter().find(|(_, rect)| rect.contains(at))
            {
                if self.toggle_kern_scope(*field) {
                    ctx.request_render();
                    ctx.set_handled();
                    return;
                }
                let edited = self.commit_card_field(ctx);
                self.focus_metric(*field);
                ctx.request_focus();
                self.emit(ctx, edited);
                ctx.set_handled();
                return;
            }
            if self.tool == Tool::Text && state.modifiers.alt() {
                let design = self.session.viewport.screen_to_design(at);
                if text::begin_kern(self, design) {
                    ctx.request_focus();
                    ctx.capture_pointer();
                    ctx.request_cursor_icon_change();
                    ctx.request_render();
                }
                ctx.set_handled();
                return;
            }
            // A click anywhere else puts the value away.
            if self.field.is_some() {
                let edited = self.commit_card_field(ctx);
                self.field = None;
                self.emit(ctx, edited);
            }
        }
        match event {
            PointerEvent::Down(PointerButtonEvent { button, state, .. }) => {
                ctx.request_focus();
                let at = ctx.local_position(state.position);
                // GPUI and Web let any editing tool follow a composed sort on
                // double-click. Activating it before replacing the live glyph
                // session is what keeps the surrounding word on the canvas.
                if state.count >= 2
                    && *button == Some(PointerButton::Primary)
                    && self.tool == Tool::Select
                    && self.hit_point(at).is_none()
                    && let Some(event) = self.component_double_click(at)
                {
                    ctx.submit_action::<EditorEvent>(event);
                    self.drag = Drag::None;
                    ctx.request_render();
                    ctx.set_handled();
                    return;
                }
                if state.count >= 2
                    && *button == Some(PointerButton::Primary)
                    && let Some(text) = self.text.as_mut()
                {
                    let design = self.session.viewport.screen_to_design(at);
                    if let Some(glyph) = text.activate_at(design) {
                        ctx.submit_action::<EditorEvent>(EditorEvent::EditGlyph {
                            name: glyph,
                            tool: self.tool,
                        });
                        self.emit_text_proof_selection(ctx);
                        self.drag = Drag::None;
                        ctx.request_render();
                        ctx.set_handled();
                        return;
                    }
                }
                // Text tool: a click is a caret placement, and a click on
                // a sort makes that glyph the one being edited.
                // Text tool: a click is a caret placement, and a click on
                // a sort makes that glyph the one being edited.
                if self.tool == Tool::Text
                    && self.text.is_some()
                    && *button == Some(PointerButton::Primary)
                {
                    text::press(self, ctx, at);
                    ctx.set_handled();
                    return;
                }
                if *button == Some(PointerButton::Secondary) {
                    // A layer, not a rectangle painted into this canvas:
                    // it is rooted in window space, so it can hang past
                    // the editor's edge like a menu should.
                    if self.menu.is_none() {
                        self.open_context_menu(ctx, at);
                    }
                    ctx.set_handled();
                    return;
                }
                ctx.capture_pointer();
                match button {
                    Some(PointerButton::Primary) if self.tool == Tool::Sketch => {
                        sketch::press(self, ctx, at);
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Metaball => {
                        metaball::press(self, ctx, at, state);
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Label => {
                        label::press(self, ctx, at, state);
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Hand => {
                        hand::press(self, at);
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Lasso => {
                        select::press_lasso(self, ctx, at, state);
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::HyperPen => {
                        pen::press_hyper(self, ctx, at, state);
                    }
                    Some(PointerButton::Primary)
                        if self.tool == Tool::AiShape
                            && let Some(handle) = self.ai_box_handle(at) =>
                    {
                        shapes::press_resize_handle(self, ctx, handle);
                    }
                    Some(PointerButton::Primary)
                        if matches!(
                            self.tool,
                            Tool::Rect | Tool::Ellipse | Tool::Knife | Tool::AiShape
                        ) =>
                    {
                        shapes::press(self, ctx, at);
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Measure => {
                        measure::press(self, ctx, at);
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Pen => {
                        pen::press(self, ctx, at);
                    }
                    Some(PointerButton::Primary) => select::press(self, ctx, at, state),
                    _ => self.drag = Drag::Pan { last: at },
                }
                ctx.set_handled();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let at = ctx.local_position(current.position);
                if self.tool == Tool::Text && matches!(self.drag, Drag::None) {
                    self.update_kern_hover(ctx, at, current.modifiers.alt());
                }
                let glyph_design = self.screen_to_glyph_design(at);
                let glyph_affine = self.glyph_affine();
                shapes::hover(self, ctx, at);
                measure::hover(self, ctx, glyph_design, current);
                pen::hover(self, ctx, at, glyph_design);
                label::hover(self, ctx, glyph_design);
                match self.drag {
                    Drag::Kern { .. } => {
                        if let Some(event) =
                            text::move_kern(self, self.session.viewport.screen_to_design(at).x)
                        {
                            ctx.submit_action::<EditorEvent>(event);
                        }
                        ctx.request_render();
                        ctx.set_handled();
                    }
                    Drag::Sketch { .. } => sketch::drag(self, ctx, glyph_design),
                    Drag::Metaballs { .. } => metaball::drag(self, ctx, glyph_design),
                    Drag::Image { .. }
                    | Drag::AdvanceLine
                    | Drag::LeftLine { .. }
                    | Drag::Component { .. }
                    | Drag::Points { .. }
                    | Drag::Marquee { .. }
                    | Drag::Lasso { .. } => {
                        select::drag(self, ctx, at, glyph_design, glyph_affine, current);
                    }
                    Drag::Pen { .. } => pen::drag(self, ctx, at, glyph_design, glyph_affine),
                    Drag::Pan { .. } => hand::drag(self, ctx, at),
                    Drag::Label(_) => label::drag(self, ctx, at, glyph_design),
                    Drag::Measure { .. } => measure::drag(self, ctx, at, glyph_design, current),
                    Drag::Shape { .. } | Drag::AiResize { .. } => {
                        shapes::drag(self, ctx, glyph_design);
                    }
                    Drag::None | Drag::Enter { .. } => {}
                }
            }
            event @ (PointerEvent::Up(_) | PointerEvent::Cancel(_)) => {
                let cancelled = matches!(event, PointerEvent::Cancel(_));
                match self.drag {
                    Drag::Kern { .. } => {
                        if let PointerEvent::Up(up) = event {
                            if up.button != Some(PointerButton::Primary) {
                                return;
                            }
                            let pos = self
                                .session
                                .viewport
                                .screen_to_design(ctx.local_position(up.state.position));
                            let _ = text::move_kern(self, pos.x);
                            self.update_kern_hover(
                                ctx,
                                ctx.local_position(up.state.position),
                                up.state.modifiers.alt(),
                            );
                        }
                        ctx.request_cursor_icon_change();
                        if let Some(event) = text::finish_kern(self, cancelled) {
                            ctx.submit_action::<EditorEvent>(event);
                        }
                        ctx.request_render();
                        ctx.set_handled();
                    }
                    Drag::Sketch { .. } => sketch::release(self, ctx),
                    Drag::Metaballs { .. } => metaball::release(self, ctx, cancelled),
                    Drag::Points { .. }
                    | Drag::Marquee { .. }
                    | Drag::Image { .. }
                    | Drag::Lasso { .. }
                    | Drag::AdvanceLine
                    | Drag::LeftLine { .. }
                    | Drag::Enter { .. }
                    | Drag::Component { .. } => select::release(self, ctx, cancelled),
                    Drag::Pen { .. } => pen::release(self, ctx, cancelled),
                    Drag::Label(_) => label::release(self, ctx, cancelled),
                    Drag::Measure { .. } => measure::release(self, ctx, cancelled),
                    Drag::Shape { .. } | Drag::AiResize { .. } => {
                        shapes::release(self, ctx, cancelled);
                    }
                    Drag::Pan { .. } => hand::release(self),
                    Drag::None => {}
                }
            }
            PointerEvent::Scroll(PointerScrollEvent { delta, state, .. }) => {
                let at = ctx.local_position(state.position);
                let dy = match delta {
                    ScrollDelta::PixelDelta(p) => p.y,
                    ScrollDelta::LineDelta(_, y) => f64::from(*y) * 20.0,
                    _ => 0.0,
                };
                let factor = (dy / 300.0).exp();
                self.session.viewport.zoom_about(at, factor, 0.02, 64.0);
                ctx.request_render();
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

/// Cheap equality for the interpolation overlay: same Arc, or both absent.
/// `to`, moved onto the nearest line through `from` at a multiple of 45 degrees.
pub(super) fn constrain_to_45(from: Point, to: Point) -> Point {
    let d = to - from;
    let step = std::f64::consts::FRAC_PI_4;
    let angle = (d.y.atan2(d.x) / step).round() * step;
    let direction = kurbo::Vec2::new(angle.cos(), angle.sin());
    from + direction * d.dot(direction)
}
