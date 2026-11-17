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
                        use crate::application::editor::tools::label::{
                            LabelHit, LabelMoving, LabelSelection,
                        };
                        ctx.request_focus();
                        let design = self.screen_to_glyph_design(at);
                        let reach = HIT_RADIUS_PX / self.session.viewport.zoom;
                        // A new press starts fresh: the last gesture's hint goes away.
                        self.session.label.error = None;
                        match self.session.label_hit(design, reach) {
                            LabelHit::Corner { region, corner } => {
                                self.session.activate_label_region(region);
                                let shift = state.modifiers.shift();
                                let mut picked = self.session.selected_corners();
                                if shift {
                                    // Shift adds a corner to the set, or takes it out.
                                    match picked.iter().position(|c| *c == (region, corner)) {
                                        Some(at) => {
                                            picked.remove(at);
                                        }
                                        None => picked.push((region, corner)),
                                    }
                                    self.session.label.selected =
                                        Some(LabelSelection::Corners(picked));
                                    self.emit(ctx, false);
                                } else {
                                    if state.modifiers.alt() || !picked.contains(&(region, corner))
                                    {
                                        // Corners that meet here move as one; Option takes
                                        // only the one on top, to pull it away.
                                        let at = self.session.corner_point(region, corner);
                                        picked = if state.modifiers.alt() {
                                            Vec::new()
                                        } else {
                                            at.map(|at| self.session.corners_at(at))
                                                .unwrap_or_default()
                                        };
                                        if picked.is_empty() {
                                            picked.push((region, corner));
                                        }
                                        self.session.label.selected =
                                            Some(LabelSelection::Corners(picked.clone()));
                                    }
                                    if picked.len() == 1 {
                                        self.session.label.moving = Some(LabelMoving::Corner {
                                            region,
                                            corner,
                                            to: design,
                                        });
                                        self.drag = Drag::LabelCorner {
                                            region,
                                            detach: state.modifiers.alt(),
                                        };
                                    } else {
                                        self.session.label.moving = Some(LabelMoving::Corners {
                                            by: kurbo::Vec2::ZERO,
                                        });
                                        self.drag = Drag::LabelCorners { start: design };
                                    }
                                    ctx.capture_pointer();
                                    self.emit(ctx, false);
                                }
                            }
                            LabelHit::Edge {
                                region,
                                after,
                                at: on,
                            } => {
                                // A corner appears on the edge and follows the pointer.
                                self.session.activate_label_region(region);
                                if let Some(corner) =
                                    self.session.add_label_corner(region, after, on)
                                {
                                    self.session.label.selected =
                                        Some(LabelSelection::Corners(vec![(region, corner)]));
                                    self.session.label.moving = Some(LabelMoving::Corner {
                                        region,
                                        corner,
                                        to: on,
                                    });
                                    self.drag = Drag::LabelCorner {
                                        region,
                                        detach: false,
                                    };
                                    ctx.capture_pointer();
                                    self.emit(ctx, true);
                                }
                            }
                            LabelHit::CutEnd { cut, end } => {
                                self.session.label.selected = Some(LabelSelection::Cut(cut));
                                self.session.label.moving = Some(LabelMoving::CutEnd {
                                    cut,
                                    end,
                                    to: design,
                                });
                                self.drag = Drag::LabelCut {
                                    cut,
                                    end: Some(end),
                                    start: design,
                                };
                                ctx.capture_pointer();
                                self.emit(ctx, false);
                            }
                            LabelHit::Cut { cut } => {
                                self.session.label.selected = Some(LabelSelection::Cut(cut));
                                self.session.label.moving = Some(LabelMoving::Cut {
                                    cut,
                                    by: kurbo::Vec2::ZERO,
                                });
                                self.drag = Drag::LabelCut {
                                    cut,
                                    end: None,
                                    start: design,
                                };
                                ctx.capture_pointer();
                                self.emit(ctx, false);
                            }
                            LabelHit::Lasso { region } => {
                                // A press inside a lasso picks it up: its letter becomes
                                // active and its corners show. A drag moves the whole shape.
                                self.session.activate_label_region(region);
                                self.session.label.selected = Some(LabelSelection::Region(region));
                                self.session.label.moving = Some(LabelMoving::Region {
                                    region,
                                    by: kurbo::Vec2::ZERO,
                                });
                                self.drag = Drag::LabelRegion {
                                    region,
                                    start: design,
                                };
                                ctx.capture_pointer();
                                self.emit(ctx, false);
                            }
                            LabelHit::Nothing
                                if state.modifiers.shift()
                                    && self.session.label.sample.is_some() =>
                            {
                                // Shift-drag: a box that selects corners.
                                self.drag = Drag::LabelMarquee {
                                    start: design,
                                    current: design,
                                };
                                ctx.capture_pointer();
                            }
                            LabelHit::Nothing => {
                                // Words are not a mode: a press outside the open word
                                // opens the word under it, or closes the open one.
                                let open = self.session.label.sample;
                                let inside_open = open
                                    .is_some_and(|open| self.session.sample_contains(open, design));
                                let under = (!inside_open)
                                    .then(|| self.session.sample_at(design))
                                    .flatten();
                                if open.is_some() && !inside_open && under.is_none() {
                                    self.session.select_sample(None);
                                }
                                if let Some(sample) = under {
                                    // A click on a word opens it.
                                    self.session.select_sample(Some(sample));
                                    self.emit(ctx, false);
                                } else {
                                    self.drag = Drag::Label { points: vec![at] };
                                }
                            }
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
                            self.session.metaballs.selected.clear();
                            self.session.image_selected = false;
                            self.session.selected_component = None;
                            if shift {
                                if !self.session.selected_anchors.remove(&anchor) {
                                    self.session.selected_anchors.insert(anchor);
                                }
                            } else if !self.session.selected_anchors.contains(&anchor) {
                                self.session.clear_selection();
                                self.session.selected_anchors.insert(anchor);
                            }
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
                                self.session.selection = std::collections::HashSet::from([id]);
                                self.emit(ctx, true);
                            }
                            Some(id) => {
                                self.session.metaballs.selected.clear();
                                self.session.image_selected = false;
                                if shift {
                                    if !self.session.selection.remove(&id) {
                                        self.session.selection.insert(id);
                                    }
                                } else if !self.session.selection.contains(&id) {
                                    self.session.clear_selection();
                                    self.session.selection.insert(id);
                                }
                                self.session.selected_component = None;
                                self.session.begin_point_drag();
                                self.drag = Drag::Points { start: at };
                                self.emit(ctx, false);
                            }
                            None => {
                                let design = self.screen_to_glyph_design(at);
                                if let Some(component) = self.session.component_at(design) {
                                    self.session.metaballs.selected.clear();
                                    self.session.select_component_id(component);
                                    self.drag = Drag::Component { last: design };
                                    self.emit(ctx, false);
                                    ctx.set_handled();
                                    return;
                                }
                                if !shift && let Some(drag) = self.image_press(design) {
                                    self.session.metaballs.selected.clear();
                                    self.session.clear_selection();
                                    self.session.selected_component = None;
                                    self.session.image_selected = true;
                                    self.drag = drag;
                                    self.emit(ctx, false);
                                    ctx.request_render();
                                    ctx.set_handled();
                                    return;
                                }
                                if !shift {
                                    self.session.metaballs.selected.clear();
                                    self.session.clear_selection();
                                    self.session.selected_component = None;
                                    self.session.image_selected = false;
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
                        use crate::application::editor::tools::label::{LabelMoving, on_grid};
                        let (region, detach) = (*region, *detach);
                        if let Some(LabelMoving::Corner { corner, .. }) = self.session.label.moving
                        {
                            let to = if detach {
                                on_grid(glyph_design)
                            } else {
                                self.session.snapped_label_corner(
                                    glyph_design,
                                    HIT_RADIUS_PX / self.session.viewport.zoom,
                                    Some(region),
                                )
                            };
                            self.session.label.moving =
                                Some(LabelMoving::Corner { region, corner, to });
                            ctx.request_render();
                        }
                    }
                    Drag::LabelCorners { start } => {
                        use crate::application::editor::tools::label::LabelMoving;
                        let start = *start;
                        // Whole grid steps, so the shape shows where it will land.
                        self.session.label.moving = Some(LabelMoving::Corners {
                            by: crate::application::editor::tools::label::grid_step(
                                glyph_design - start,
                            ),
                        });
                        ctx.request_render();
                    }
                    Drag::LabelMarquee { current, .. } => {
                        *current = glyph_design;
                        ctx.request_render();
                    }
                    Drag::LabelRegion { region, start } => {
                        use crate::application::editor::tools::label::LabelMoving;
                        let (region, start) = (*region, *start);
                        self.session.label.moving = Some(LabelMoving::Region {
                            region,
                            by: crate::application::editor::tools::label::grid_step(
                                glyph_design - start,
                            ),
                        });
                        ctx.request_render();
                    }
                    Drag::LabelCut { cut, end, start } => {
                        use crate::application::editor::tools::label::LabelMoving;
                        let (cut, end, start) = (*cut, *end, *start);
                        self.session.label.moving = Some(match end {
                            Some(end) => LabelMoving::CutEnd {
                                cut,
                                end,
                                to: glyph_design,
                            },
                            None => LabelMoving::Cut {
                                cut,
                                by: glyph_design - start,
                            },
                        });
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
                        if !additive {
                            self.session.clear_selection();
                        }
                        for (id, sp, _, _, _) in self.screen_points() {
                            if rect.contains(sp) {
                                self.session.selection.insert(id);
                            }
                        }
                        let affine = self.glyph_affine();
                        for (id, position) in self.session.anchor_points() {
                            if rect.contains(affine * position) {
                                self.session.selected_anchors.insert(id);
                            }
                        }
                        // A locked picture cannot be moved, so a drag over it is a marquee;
                        // a click on it still selects it, to inspect or unlock it.
                        let clicked = rect.width().max(rect.height()) < 3.0;
                        if clicked && !additive && !self.session.has_selection() {
                            let design = self.screen_to_glyph_design(*start);
                            self.session.image_selected =
                                self.image_frame(true).is_some_and(|(t, w, h)| {
                                    (t * Rect::new(0.0, 0.0, w, h).to_path(0.1))
                                        .bounding_box()
                                        .contains(design)
                                });
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
                        use crate::application::editor::tools::label::LabelSelection;
                        let rect = Rect::from_points(*start, *current);
                        if !cancelled {
                            let picked = self.session.corners_in(rect);
                            self.session.label.selected = if picked.is_empty() {
                                None
                            } else {
                                Some(LabelSelection::Corners(picked))
                            };
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
                            let contours = self.session.label_contours();
                            let near = 12.0 / self.session.viewport.zoom;
                            if !moved {
                                // A click on nothing lets go of the picked shape.
                                self.session.label.selected = None;
                            } else {
                                // Labels are polygons: a loop makes one, any other drag
                                // does nothing.
                                use crate::application::editor::tools::label::{DragKind, drag_is};
                                if drag_is(&design, &contours, near) == DragKind::Lasso {
                                    changed = self.session.add_label_lasso(&design);
                                } else {
                                    self.session.label.error =
                                        Some("Drag a loop: close it near where it began".into());
                                }
                            }
                        }
                        self.drag = Drag::None;
                        self.label_hover = None;
                        self.emit(ctx, changed);
                    }
                    Drag::Lasso { points, additive } => {
                        if !cancelled {
                            let additive = *additive;
                            let mut path = kurbo::BezPath::new();
                            if let Some(first) = points.first() {
                                path.move_to(*first);
                                for point in &points[1..] {
                                    path.line_to(*point);
                                }
                                path.close_path();
                            }
                            let chosen: Vec<_> = if points.len() >= 3 {
                                self.screen_points()
                                    .into_iter()
                                    .filter(|(_, point, _, _, _)| path.winding(*point) != 0)
                                    .map(|(id, _, _, _, _)| id)
                                    .collect()
                            } else {
                                points
                                    .last()
                                    .and_then(|point| self.hit_point(*point))
                                    .into_iter()
                                    .collect()
                            };
                            let affine = self.glyph_affine();
                            let anchors: Vec<_> = if points.len() >= 3 {
                                self.session
                                    .anchor_points()
                                    .into_iter()
                                    .filter(|(_, position)| path.winding(affine * *position) != 0)
                                    .map(|(id, _)| id)
                                    .collect()
                            } else {
                                Vec::new()
                            };
                            if !additive {
                                self.session.clear_selection();
                            }
                            self.session.selection.extend(chosen);
                            self.session.selected_anchors.extend(anchors);
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
