// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Pointer handling of the editor widget: presses, drags, releases and scrolling for every tool.

use super::*;

impl EditorWidget {
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
                let edited = self.commit_metric();
                self.focus_metric(*field);
                ctx.request_focus();
                self.emit(ctx, edited);
                ctx.set_handled();
                return;
            }
            // A click anywhere else puts the value away.
            if self.field.is_some() {
                let edited = self.commit_metric();
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
                if self.tool == Tool::Text
                    && let Some(text) = self.text.as_mut()
                    && *button == Some(PointerButton::Primary)
                {
                    self.cursor_blink_elapsed_ns = 0;
                    self.cursor_visible = true;
                    ctx.request_anim_frame();
                    let design = self.session.viewport.screen_to_design(at);
                    if let Some(index) = text.click(design)
                        && let Some(glyph) = text.activate(index)
                    {
                        ctx.submit_action::<EditorEvent>(EditorEvent::EditGlyph {
                            name: glyph,
                            tool: Tool::Text,
                        });
                    }
                    self.emit_text_proof_selection(ctx);
                    ctx.request_render();
                    ctx.set_handled();
                    return;
                }
                if *button == Some(PointerButton::Secondary) {
                    // A layer, not a rectangle painted into this canvas:
                    // it is rooted in window space, so it can hang past
                    // the editor's edge like a menu should.
                    if self.menu.is_none() {
                        let design = self.screen_to_glyph_design(at);
                        let mut rows = MENU_ITEMS.to_vec();
                        let on_image = self.image_frame(true).is_some_and(|(t, w, h)| {
                            (t * Rect::new(0.0, 0.0, w, h).to_path(0.1))
                                .bounding_box()
                                .contains(design)
                        });
                        if on_image {
                            let locked = self.session.image_locked();
                            rows.splice(
                                0..0,
                                [
                                    MenuRow {
                                        label: std::borrow::Cow::Borrowed(if locked {
                                            "Unlock Image"
                                        } else {
                                            "Lock Image"
                                        }),
                                        action: MenuAction::Op(|s| s.toggle_image_lock()),
                                    },
                                    MenuRow {
                                        label: std::borrow::Cow::Borrowed("Remove Image"),
                                        action: MenuAction::Op(|s| s.remove_image()),
                                    },
                                ],
                            );
                        }
                        if let Some(component) = self.session.component_at(design) {
                            self.session.select_component_id(component);
                            self.emit(ctx, false);
                            let aligned = self.session.selected_component_aligned() == Some(true);
                            let mut component_rows = vec![MenuRow {
                                label: std::borrow::Cow::Borrowed("Edit Component in Place"),
                                action: MenuAction::EditComponent(component),
                            }];
                            if let Some(base) = self.session.component_reference(component) {
                                component_rows.push(MenuRow {
                                    label: std::borrow::Cow::Owned(format!("Open {base}")),
                                    action: MenuAction::OpenBaseGlyph(base),
                                });
                            }
                            component_rows.push(MenuRow {
                                label: std::borrow::Cow::Borrowed(if aligned {
                                    "Disable Automatic Alignment"
                                } else {
                                    "Enable Automatic Alignment"
                                }),
                                action: MenuAction::ToggleComponentAlignment,
                            });
                            rows.splice(0..0, component_rows);
                        }
                        if let Some(host) = &self.session.host {
                            rows.insert(
                                0,
                                MenuRow {
                                    label: std::borrow::Cow::Owned(format!(
                                        "Back to {}",
                                        host.root
                                    )),
                                    action: MenuAction::ExitComponent,
                                },
                            );
                            rows.insert(
                                1,
                                MenuRow {
                                    label: std::borrow::Cow::Owned(format!(
                                        "Open {}",
                                        self.session.glyph_name
                                    )),
                                    action: MenuAction::OpenBaseGlyph(
                                        self.session.glyph_name.clone(),
                                    ),
                                },
                            );
                        }
                        let menu = ContextMenu::new(
                            ctx.widget_id(),
                            MenuTarget::Editor,
                            rows,
                            self.palette.clone(),
                            design,
                        );
                        let menu = NewWidget::new(menu);
                        self.menu = Some(menu.id());
                        ctx.create_layer(LayerType::Other, menu, ctx.to_window(at));
                    }
                    ctx.set_handled();
                    return;
                }
                ctx.capture_pointer();
                match button {
                    Some(PointerButton::Primary) if self.tool == Tool::Sketch => {
                        let design = self.screen_to_glyph_design(at);
                        let mut sketch = self
                            .sketch
                            .lock()
                            .unwrap_or_else(|error| error.into_inner());
                        if !sketch.matches(&self.session.glyph_name, self.sketch_source) {
                            *sketch = sketch.for_glyph(
                                self.session.glyph_name.clone(),
                                self.sketch_source,
                                self.session.advance(),
                            );
                        }
                        let changed = sketch.stroke((design.x, design.y), (design.x, design.y));
                        self.drag = Drag::Sketch {
                            last: design,
                            changed,
                        };
                        ctx.request_render();
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Metaball => {
                        ctx.request_focus();
                        let at = self.screen_to_glyph_design(at);
                        let changed = self.session.metaball_click(
                            at,
                            HIT_RADIUS_PX / self.session.viewport.zoom,
                            state.modifiers.shift(),
                        );
                        self.drag = Drag::Metaballs { last: at, changed };
                        self.emit(ctx, false);
                        ctx.request_render();
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Label => {
                        ctx.request_focus();
                        let design = self.screen_to_glyph_design(at);
                        let reach = HIT_RADIUS_PX / self.session.viewport.zoom;
                        let modifiers = LabelModifiers {
                            shift: state.modifiers.shift(),
                            alt: state.modifiers.alt(),
                        };
                        match self.session.label_press(design, reach, modifiers) {
                            LabelPress::Selected => self.emit(ctx, false),
                            LabelPress::Corner { region, detach } => {
                                self.drag = Drag::LabelCorner { region, detach };
                                ctx.capture_pointer();
                                self.emit(ctx, false);
                            }
                            LabelPress::NewCorner { region } => {
                                self.drag = Drag::LabelCorner {
                                    region,
                                    detach: false,
                                };
                                ctx.capture_pointer();
                                self.emit(ctx, true);
                            }
                            LabelPress::Corners { start } => {
                                self.drag = Drag::LabelCorners { start };
                                ctx.capture_pointer();
                                self.emit(ctx, false);
                            }
                            LabelPress::Cut { cut, end, start } => {
                                self.drag = Drag::LabelCut { cut, end, start };
                                ctx.capture_pointer();
                                self.emit(ctx, false);
                            }
                            LabelPress::Region { region, start } => {
                                self.drag = Drag::LabelRegion { region, start };
                                ctx.capture_pointer();
                                self.emit(ctx, false);
                            }
                            LabelPress::Marquee { start } => {
                                self.drag = Drag::LabelMarquee {
                                    start,
                                    current: start,
                                };
                                ctx.capture_pointer();
                            }
                            LabelPress::Freehand => self.drag = Drag::Label { points: vec![at] },
                            LabelPress::Ignored => {}
                        }
                        ctx.request_render();
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Hand => {
                        self.drag = Drag::Pan { last: at };
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Lasso => {
                        ctx.request_focus();
                        self.drag = Drag::Lasso {
                            points: vec![at],
                            additive: state.modifiers.shift(),
                        };
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::HyperPen => {
                        let affine = self.glyph_affine();
                        let near_first = self
                            .session
                            .first_contour_point()
                            .map(|p| (affine * p).distance(at) <= HIT_RADIUS_PX)
                            .unwrap_or(false);
                        let corner = state.modifiers.alt();
                        if near_first && self.session.hyper_is_active() {
                            self.session.hyper_close();
                        } else {
                            let d = self.screen_to_glyph_design(at);
                            self.session.hyper_add(d.x, d.y, corner);
                        }
                        self.emit(ctx, true);
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary)
                        if self.tool == Tool::AiShape
                            && let Some(handle) = self.ai_box_handle(at) =>
                    {
                        ctx.request_focus();
                        ctx.capture_pointer();
                        self.drag = Drag::AiResize { handle };
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary)
                        if matches!(
                            self.tool,
                            Tool::Rect | Tool::Ellipse | Tool::Knife | Tool::AiShape
                        ) =>
                    {
                        ctx.request_focus();
                        ctx.capture_pointer();
                        let d = self.screen_to_glyph_design(at);
                        self.drag = Drag::Shape {
                            start: d,
                            current: d,
                        };
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Measure => {
                        ctx.request_focus();
                        let start = self.measure_snap(at);
                        self.ruler = Some(Line::new(start, start));
                        self.drag = Drag::Measure { start };
                        ctx.request_render();
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Pen => {
                        let affine = self.glyph_affine();
                        let near_first = self
                            .session
                            .pen_first_point()
                            .map(|p| (affine * p).distance(at) <= HIT_RADIUS_PX)
                            .unwrap_or(false);
                        let on_segment = !self.session.pen_is_active()
                            && self.hit_point(at).is_none()
                            && self.session.insert_point_at(
                                self.screen_to_glyph_design(at),
                                HIT_RADIUS_PX / self.session.viewport.zoom,
                            );
                        if near_first && self.session.pen_is_active() {
                            self.session.pen_close();
                            self.drag = Drag::None;
                            self.emit(ctx, true);
                        } else if on_segment {
                            self.insert_preview = None;
                            // A click on an outline adds a point to that
                            // segment rather than starting a new contour.
                            self.drag = Drag::None;
                            self.emit(ctx, true);
                            ctx.request_render();
                        } else {
                            let origin = self.screen_to_glyph_design(at);
                            let (point_count, active_contour) = self.session.pen_checkpoint();
                            self.drag = Drag::Pen {
                                origin,
                                dragging: false,
                                point_count,
                                active_contour,
                            };
                        }
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) => {
                        let shift = state.modifiers.shift();
                        if self.tool == Tool::Select {
                            let design = self.screen_to_glyph_design(at);
                            if self.session.select_metaball_at(
                                design,
                                HIT_RADIUS_PX / self.session.viewport.zoom,
                                shift,
                            ) {
                                ctx.request_focus();
                                self.drag = Drag::Metaballs {
                                    last: design,
                                    changed: false,
                                };
                                self.emit(ctx, false);
                                ctx.request_render();
                                ctx.set_handled();
                                return;
                            }
                        }
                        // Sidebearing lines (only when not near a point).
                        let affine = self.glyph_affine();
                        let adv_x = (affine * Point::new(self.session.advance(), 0.0)).x;
                        let lsb_x = (affine * Point::new(0.0, 0.0)).x;
                        if self.hit_point(at).is_none()
                            && self.hit_part_point(at).is_none()
                            && !self.session.neural
                            && self.session.host.is_none()
                        {
                            if (at.x - adv_x).abs() <= 4.0 {
                                self.drag = Drag::AdvanceLine;
                                ctx.set_handled();
                                return;
                            }
                            if (at.x - lsb_x).abs() <= 4.0 {
                                self.drag = Drag::LeftLine { last_x: at.x };
                                ctx.set_handled();
                                return;
                            }
                        }
                        // An anchor selects and drags like a point, and wins over a point
                        // under it.
                        if let Some(anchor) = self.session.anchor_at(
                            self.screen_to_glyph_design(at),
                            HIT_RADIUS_PX / self.session.viewport.zoom,
                        ) {
                            self.session.press_anchor(anchor, shift);
                            self.session.begin_point_drag();
                            self.drag = Drag::Points { start: at };
                            self.emit(ctx, false);
                            ctx.set_handled();
                            return;
                        }
                        // Option-click on a segment gives it two curve handles, as in Glyphs.
                        if self.tool == Tool::Select
                            && state.modifiers.alt()
                            && self.hit_point(at).is_none()
                            && self.session.add_segment_handles_at(
                                self.screen_to_glyph_design(at),
                                HIT_RADIUS_PX / self.session.viewport.zoom,
                            )
                        {
                            self.emit(ctx, true);
                            ctx.set_handled();
                            return;
                        }
                        if self.hit_point(at).is_none()
                            && let Some((owner, point)) = self.hit_part_point(at)
                        {
                            ctx.submit_action::<EditorEvent>(EditorEvent::EnterPart {
                                owner,
                                point: Some(point),
                            });
                            self.drag = Drag::Enter { start: at };
                            ctx.set_handled();
                            return;
                        }
                        match self.hit_point(at) {
                            // A double click on an on-curve point switches it between corner
                            // and smooth, as in Glyphs.
                            Some(id) if state.count >= 2 && self.session.toggle_smooth(id) => {
                                self.session.replace_point_selection([id]);
                                self.emit(ctx, true);
                            }
                            Some(id) => {
                                self.session.press_point(id, shift);
                                self.session.begin_point_drag();
                                self.drag = Drag::Points { start: at };
                                self.emit(ctx, false);
                            }
                            None => {
                                let design = self.screen_to_glyph_design(at);
                                if let Some(component) = self.session.component_at(design) {
                                    self.session
                                        .select_only(SelectionTarget::Component(component));
                                    self.drag = Drag::Component { last: design };
                                    self.emit(ctx, false);
                                    ctx.set_handled();
                                    return;
                                }
                                if !shift && let Some(drag) = self.image_press(design) {
                                    self.session.select_only(SelectionTarget::Image);
                                    self.drag = drag;
                                    self.emit(ctx, false);
                                    ctx.request_render();
                                    ctx.set_handled();
                                    return;
                                }
                                if !shift {
                                    self.session.select_only(SelectionTarget::Nothing);
                                    self.emit(ctx, false);
                                }
                                self.drag = Drag::Marquee {
                                    start: at,
                                    current: at,
                                    additive: shift,
                                };
                            }
                        }
                    }
                    _ => self.drag = Drag::Pan { last: at },
                }
                ctx.set_handled();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let at = ctx.local_position(current.position);
                let glyph_design = self.screen_to_glyph_design(at);
                let glyph_affine = self.glyph_affine();
                if self.tool == Tool::AiShape && matches!(self.drag, Drag::None) {
                    let hover = self.ai_box_handle(at);
                    if hover != self.ai_hover {
                        self.ai_hover = hover;
                        ctx.request_render();
                    }
                }
                if self.tool == Tool::Measure {
                    self.hover = Some(glyph_design);
                    self.hover_vertical = current.modifiers.alt();
                    ctx.request_render();
                }
                if matches!(self.tool, Tool::Pen | Tool::HyperPen) {
                    self.hover = Some(glyph_design);
                    if self.session.contour_drawing_is_active() {
                        ctx.request_render();
                    }
                }
                let insert_preview = (self.tool == Tool::Pen
                    && matches!(self.drag, Drag::None)
                    && !self.session.pen_is_active()
                    && self.hit_point(at).is_none())
                .then(|| {
                    self.session
                        .segment_insert_at(glyph_design, HIT_RADIUS_PX / self.session.viewport.zoom)
                })
                .flatten()
                .map(|insert| (insert.point, insert.curve));
                if insert_preview != self.insert_preview {
                    self.insert_preview = insert_preview;
                    ctx.request_render();
                }
                if self.tool == Tool::Label && matches!(self.drag, Drag::None) {
                    use crate::application::editor::tools::label::LabelHit;
                    let hit = self
                        .session
                        .label_hit(glyph_design, HIT_RADIUS_PX / self.session.viewport.zoom);
                    let hover = (hit != LabelHit::Nothing).then_some(hit);
                    if hover != self.label_hover {
                        self.label_hover = hover;
                        ctx.request_render();
                    }
                }
                match &mut self.drag {
                    Drag::Sketch { last, changed } => {
                        let mut sketch = self
                            .sketch
                            .lock()
                            .unwrap_or_else(|error| error.into_inner());
                        *changed |=
                            sketch.stroke((last.x, last.y), (glyph_design.x, glyph_design.y));
                        *last = glyph_design;
                        ctx.request_render();
                    }
                    Drag::Metaballs { last, changed } => {
                        let moved = self.session.move_metaballs(glyph_design - *last, true);
                        *changed |= moved;
                        *last = glyph_design;
                        if moved {
                            // Publish the live draft to the proof strip and inspector.
                            // Only release emits Edited and commits this gesture.
                            self.emit(ctx, false);
                            ctx.request_render();
                        }
                    }
                    Drag::Image {
                        start,
                        origin,
                        anchor,
                        corner,
                    } => {
                        self.image_preview = Some(Self::image_drag_transform(
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
                        self.session.drag_advance(d.x.round());
                        ctx.request_render();
                    }
                    Drag::LeftLine { last_x } => {
                        // Only the whole units applied are consumed, so a slow drag
                        // keeps its remainder for the next event.
                        let zoom = self.session.viewport.zoom;
                        let dx = ((at.x - *last_x) / zoom).round();
                        if dx != 0.0 {
                            *last_x += dx * zoom;
                            self.session.shift_glyph(dx);
                            ctx.request_render();
                        }
                    }
                    Drag::Component { last } => {
                        let design = glyph_design;
                        let delta = design - *last;
                        *last = design;
                        if self.session.drag_component_by(delta.x, delta.y) {
                            ctx.request_render();
                        }
                    }
                    Drag::Pen {
                        origin, dragging, ..
                    } => {
                        let origin = *origin;
                        let to = glyph_design;
                        let moved_px = (glyph_affine * origin).distance(at);
                        if *dragging {
                            self.session.pen_smooth_drag(origin, to);
                            ctx.request_render();
                        } else if moved_px > 4.0 {
                            if let Drag::Pen { dragging, .. } = &mut self.drag {
                                *dragging = true;
                            }
                            self.session.pen_smooth_begin(origin, to);
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
                        if self.session.drag_points_to(total, independent) {
                            ctx.request_render();
                        }
                    }
                    Drag::Pan { last } => {
                        let d = at - *last;
                        *last = at;
                        self.session.viewport.pan(d.x, d.y);
                        ctx.request_render();
                    }
                    Drag::Marquee { current, .. } => {
                        *current = at;
                        ctx.request_render();
                    }
                    Drag::LabelCorner { region, detach } => {
                        let reach = HIT_RADIUS_PX / self.session.viewport.zoom;
                        if self
                            .session
                            .drag_label_corner(*region, *detach, glyph_design, reach)
                        {
                            ctx.request_render();
                        }
                    }
                    Drag::LabelCorners { start } => {
                        self.session.drag_label_corners(*start, glyph_design);
                        ctx.request_render();
                    }
                    Drag::LabelMarquee { current, .. } => {
                        *current = glyph_design;
                        ctx.request_render();
                    }
                    Drag::LabelRegion { region, start } => {
                        self.session
                            .drag_label_region(*region, *start, glyph_design);
                        ctx.request_render();
                    }
                    Drag::LabelCut { cut, end, start } => {
                        self.session
                            .drag_label_cut(*cut, *end, *start, glyph_design);
                        ctx.request_render();
                    }
                    Drag::Lasso { points, .. } | Drag::Label { points, .. } => {
                        if points.last().is_none_or(|last| last.distance(at) >= 2.0) {
                            points.push(at);
                            ctx.request_render();
                        }
                    }
                    Drag::Measure { start } => {
                        let start = *start;
                        let end = if current.modifiers.shift() {
                            constrain_to_45(start, glyph_design)
                        } else {
                            self.measure_snap(at)
                        };
                        self.ruler = Some(Line::new(start, end));
                        ctx.request_render();
                    }
                    Drag::Shape { current, .. } => {
                        *current = glyph_design;
                        ctx.request_render();
                    }
                    Drag::AiResize { handle } => {
                        let handle = *handle;
                        self.ai_resize(handle, glyph_design);
                        ctx.request_render();
                    }
                    Drag::None | Drag::Enter { .. } => {}
                }
            }
            event @ (PointerEvent::Up(_) | PointerEvent::Cancel(_)) => {
                let cancelled = matches!(event, PointerEvent::Cancel(_));
                match &self.drag {
                    Drag::Sketch { changed, .. } => {
                        if *changed {
                            ctx.submit_action::<EditorEvent>(EditorEvent::SketchChanged);
                        }
                        self.drag = Drag::None;
                    }
                    Drag::Metaballs { changed, .. } => {
                        let changed = *changed;
                        if cancelled {
                            self.session.cancel_metaball_drag();
                        } else {
                            self.session.end_metaball_drag();
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, changed && !cancelled);
                    }
                    Drag::Points { .. } => {
                        if cancelled {
                            self.session.cancel_point_drag();
                        } else {
                            self.session.end_point_drag();
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, !cancelled);
                    }
                    Drag::Pen {
                        origin,
                        dragging,
                        point_count,
                        active_contour,
                    } => {
                        if cancelled {
                            self.session
                                .cancel_pen_gesture(*point_count, *active_contour);
                        } else if !dragging {
                            self.session.pen_corner(origin.x, origin.y);
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, !cancelled);
                    }
                    // A cancelled marquee leaves the selection as it was.
                    Drag::Marquee { .. } if cancelled => {
                        self.drag = Drag::None;
                        self.emit(ctx, false);
                    }
                    Drag::Marquee {
                        start,
                        current,
                        additive,
                    } => {
                        let rect = Rect::from_points(*start, *current);
                        let additive = *additive;
                        let affine = self.glyph_affine();
                        self.session
                            .select_within(SelectionShape::Rect(rect), affine, additive);
                        // A locked picture cannot be moved, so a drag over it is a marquee;
                        // a click on it still selects it, to inspect or unlock it.
                        let clicked = rect.width().max(rect.height()) < 3.0;
                        if clicked && !additive && !self.session.has_selection() {
                            let design = self.screen_to_glyph_design(*start);
                            let on_image = self.image_frame(true).is_some_and(|(t, w, h)| {
                                (t * Rect::new(0.0, 0.0, w, h).to_path(0.1))
                                    .bounding_box()
                                    .contains(design)
                            });
                            self.session.set_image_selected(on_image);
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, false);
                    }
                    Drag::Image { origin, .. } => {
                        let origin = *origin;
                        let moved = self.image_preview.take();
                        let changed = !cancelled
                            && moved.is_some_and(|transform| {
                                transform != origin && self.session.set_image_transform(transform)
                            });
                        self.drag = Drag::None;
                        self.emit(ctx, changed);
                    }
                    Drag::LabelMarquee { start, current } => {
                        let rect = Rect::from_points(*start, *current);
                        if !cancelled {
                            self.session.select_label_corners_in(rect);
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, false);
                    }
                    Drag::LabelCorner { .. }
                    | Drag::LabelCorners { .. }
                    | Drag::LabelCut { .. }
                    | Drag::LabelRegion { .. } => {
                        let changed = self.session.finish_label_move(!cancelled);
                        self.drag = Drag::None;
                        self.emit(ctx, changed);
                    }
                    Drag::Label { points } => {
                        let mut changed = false;
                        if !cancelled {
                            let design: Vec<Point> = points
                                .iter()
                                .map(|p| self.screen_to_glyph_design(*p))
                                .collect();
                            let travelled = points
                                .iter()
                                .fold(Rect::from_points(points[0], points[0]), |r, p| {
                                    r.union_pt(*p)
                                });
                            let moved = travelled.width().max(travelled.height()) > 4.0;
                            let near = 12.0 / self.session.viewport.zoom;
                            changed = self.session.finish_label_freehand(&design, moved, near);
                        }
                        self.drag = Drag::None;
                        self.label_hover = None;
                        self.emit(ctx, changed);
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
                                let affine = self.glyph_affine();
                                self.session.select_within(
                                    SelectionShape::Path(&path),
                                    affine,
                                    additive,
                                );
                            } else {
                                let chosen = points.last().and_then(|point| self.hit_point(*point));
                                self.session.select_points(chosen, additive);
                            }
                            self.emit(ctx, false);
                        }
                        self.drag = Drag::None;
                        ctx.request_render();
                    }
                    Drag::AdvanceLine | Drag::LeftLine { .. } => {
                        if cancelled {
                            self.session.cancel_metric_drag();
                        } else {
                            self.session.end_metric_drag();
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, !cancelled);
                    }
                    Drag::Enter { .. } => self.drag = Drag::None,
                    Drag::Measure { .. } => {
                        // A click without a drag clears the ruler and brings back the scan line.
                        if cancelled || self.ruler.is_some_and(|ruler| ruler.length() < 1.0) {
                            self.ruler = None;
                        }
                        self.drag = Drag::None;
                        ctx.request_render();
                    }
                    Drag::Component { .. } => {
                        if cancelled {
                            self.session.cancel_component_drag();
                        } else {
                            self.session.end_component_drag();
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, !cancelled);
                    }
                    Drag::Shape { start, current } => {
                        if !cancelled {
                            let (s0, c0) = (*start, *current);
                            match self.tool {
                                Tool::Rect => self.session.add_rect(s0.x, s0.y, c0.x, c0.y),
                                Tool::Ellipse => {
                                    self.session.add_ellipse(s0.x, s0.y, c0.x, c0.y);
                                }
                                Tool::Knife => {
                                    self.session.knife_cut(s0, c0);
                                }
                                Tool::AiShape => {
                                    self.begin_ai_box(ctx, s0, c0);
                                    self.drag = Drag::None;
                                    ctx.request_render();
                                    return;
                                }
                                _ => {}
                            }
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, !cancelled);
                    }
                    Drag::AiResize { .. } => {
                        self.drag = Drag::None;
                        if !cancelled && let Some(rect) = self.ai_box {
                            let rect = Rect::new(
                                rect.x0.round(),
                                rect.y0.round(),
                                rect.x1.round(),
                                rect.y1.round(),
                            );
                            self.ai_box = Some(rect);
                            // The shape is drawn again for the new box; until
                            // it arrives the old ghost is shown stretched.
                            ctx.submit_action::<EditorEvent>(EditorEvent::AiShapeBox { rect });
                        }
                        ctx.request_render();
                    }
                    Drag::Pan { .. } => self.drag = Drag::None,
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
