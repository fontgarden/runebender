// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The floating metrics card: its field positions, key handling and painting.

use super::*;

const METRIC_CAPTION: &str = "LSB Width RSB";

impl EditorWidget {
    /// The metrics panel that floats over the drawing, centered at its bottom.
    ///
    /// The numbers being edited sit with the drawing instead of in a column at
    /// the side, which keeps the relationship visible.
    ///
    /// This is painted rather than composed. The view-land way is a
    /// `zstack` around the editor pane, and that one extra container
    /// around an application-sized view tree took the build from under a
    /// minute to over thirty-five, at which point it was killed rather
    /// than finished. Painting it here costs one method and no types.
    /// Where the three metric boxes are, in widget coordinates.
    ///
    /// Paint and hit testing both read this, so the box you click is the
    /// box you can see. Writing it twice is how a painted control drifts.
    pub(super) fn metric_boxes(&self) -> Option<Vec<(MetricField, Rect)>> {
        if self.preview_mode || self.session.neural {
            return None;
        }
        let (left, top) = self.metrics_panel_origin()?;
        let y = top + PANEL_HEADER + PANEL_PAD + DesignStroke::Hairline.px();
        let box_at = |column: f64| {
            let x = left + METRICS_FIELD_START + column * (METRICS_FIELD_WIDTH + METRICS_FIELD_GAP);
            Rect::new(x, y, x + METRICS_FIELD_WIDTH, y + PANEL_ROW)
        };
        let mut fields = vec![
            (MetricField::Lsb, box_at(0.0)),
            (MetricField::Width, box_at(1.0)),
            (MetricField::Rsb, box_at(2.0)),
        ];
        for (field, col) in [(MetricField::KernLeft, -1.0), (MetricField::KernRight, 3.0)] {
            if self.kern_pair(field).is_some() {
                fields.push((field, box_at(col)));
            }
        }
        let down = kurbo::Vec2::new(0.0, PANEL_ROW + 4.0);
        let center = left + PANEL_WIDTH / 2.0;
        let caption_half = text_label::width(METRIC_CAPTION, TextSize::Body.px()) / 2.0;
        let mut group_left = box_at(-1.0) + down;
        let mut group_right = box_at(3.0) + down;
        group_left.x1 = center - caption_half - METRICS_FIELD_GAP;
        group_right.x0 = center + caption_half + METRICS_FIELD_GAP;
        fields.push((MetricField::GroupLeft, group_left));
        fields.push((MetricField::GroupRight, group_right));
        Some(fields)
    }

    pub(super) fn kern_pair(&self, field: MetricField) -> Option<(&str, &str, f64)> {
        let text = self.text.as_ref()?;
        // Another glyph of a cluster, such as the dots of a decomposed letter, has no pair of
        // its own: kerning is between sorts.
        if text.buffer.active_cluster_glyph().is_some() {
            return None;
        }
        let active = text.buffer.active_glyph_name()?;
        if active != self.session.glyph_name || self.session.host.is_some() {
            return None;
        }
        text.buffer
            .active_visual_kern(field == MetricField::KernLeft)
    }

    fn kern_target(
        &self,
        field: MetricField,
        scope: [bool; 2],
    ) -> Option<(&str, &str, Option<f64>)> {
        let (a, b, _) = self.kern_pair(field)?;
        let buffer = &self.text.as_ref()?.buffer;
        let [ga, gb] = buffer.pair_groups(a, b);
        let a = if scope[0] { ga? } else { a };
        let b = if scope[1] { gb? } else { b };
        Some((a, b, buffer.stored_kern(a, b)))
    }

    fn scope_index(&self, field: MetricField) -> usize {
        let rtl = self
            .text
            .as_ref()
            .and_then(|t| t.buffer.active_run_is_rtl())
            .unwrap_or(false);
        usize::from((field == MetricField::GroupRight) != rtl)
    }

    pub(super) fn toggle_kern_scope(&mut self, button: MetricField) -> bool {
        let Some(field @ (MetricField::KernLeft | MetricField::KernRight)) = self.field else {
            return false;
        };
        if !matches!(button, MetricField::GroupLeft | MetricField::GroupRight) {
            return false;
        }
        let mut scope = self.kern_groups;
        let i = self.scope_index(button);
        scope[i] = !scope[i];
        if let Some((_, _, stored)) = self.kern_target(field, scope) {
            self.kern_groups = scope;
            if !self.kern_typed {
                self.field_buf = stored.map(|v| v.to_string()).unwrap_or_default();
            }
        }
        true
    }

    fn card_group(&self, field: MetricField) -> (bool, &str) {
        let rtl = self
            .text
            .as_ref()
            .and_then(|t| t.buffer.active_run_is_rtl())
            .unwrap_or(false);
        let first = (field == MetricField::GroupLeft) == rtl;
        let name = if first {
            &self.groups.0
        } else {
            &self.groups.1
        };
        (
            first,
            name.strip_prefix("public.kern1.")
                .or_else(|| name.strip_prefix("public.kern2."))
                .unwrap_or(name),
        )
    }

    pub(super) fn commit_card_field(&mut self, ctx: &mut EventCtx<'_>) -> bool {
        if let Some(field @ (MetricField::GroupLeft | MetricField::GroupRight)) = self.field {
            let (first, current) = self.card_group(field);
            if current != self.field_buf {
                ctx.submit_action::<EditorEvent>(EditorEvent::KernGroup(
                    first,
                    self.field_buf.clone(),
                ));
            }
            return false;
        }
        if let Some(field @ (MetricField::KernLeft | MetricField::KernRight)) = self.field {
            if self.kern_typed
                && let Some((a, b, stored)) = self.kern_target(field, self.kern_groups)
            {
                if self.field_buf.is_empty() && stored.is_some() {
                    ctx.submit_action::<EditorEvent>(EditorEvent::RemoveKernPair(
                        a.into(),
                        b.into(),
                    ));
                } else if let Ok(v) = self.field_buf.parse::<i32>()
                    && Some(f64::from(v)) != stored
                {
                    ctx.submit_action::<EditorEvent>(EditorEvent::KernPair(
                        a.into(),
                        b.into(),
                        v.into(),
                    ));
                }
            }
            return false;
        }
        self.commit_metric()
    }

    /// The panel's top left corner, or `None` when it does not fit.
    pub(super) fn metrics_panel_origin(&self) -> Option<(f64, f64)> {
        let top = self.size.height - PANEL_HEIGHT - METRICS_CARD_BOTTOM;
        if top < 0.0 || PANEL_WIDTH + PANEL_PAD * 2.0 > self.size.width {
            return None;
        }
        Some(((self.size.width - PANEL_WIDTH) / 2.0, top))
    }

    pub(super) fn metric_key(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &masonry::core::KeyboardEvent,
    ) -> (bool, bool) {
        let Some(field) = self.field else {
            return (false, false);
        };
        match &key.key {
            Key::Character(typed) => {
                for character in typed.chars() {
                    if matches!(field, MetricField::GroupLeft | MetricField::GroupRight)
                        || character.is_ascii_digit()
                        || character == '-'
                    {
                        if !self.kern_typed
                            && matches!(field, MetricField::KernLeft | MetricField::KernRight)
                        {
                            self.field_buf.clear();
                            self.kern_typed = true;
                        }
                        self.field_buf.push(character);
                    }
                }
                (false, true)
            }
            Key::Named(NamedKey::Backspace) => {
                if !self.kern_typed
                    && matches!(field, MetricField::KernLeft | MetricField::KernRight)
                {
                    self.field_buf.clear();
                    self.kern_typed = true;
                } else {
                    self.field_buf.pop();
                }
                (false, true)
            }
            Key::Named(NamedKey::Escape) => {
                self.field = None;
                (false, true)
            }
            Key::Named(NamedKey::Tab) => {
                let fields = self.metric_boxes().unwrap_or_default();
                let Some(at) = fields.iter().position(|(f, _)| *f == field) else {
                    return (false, true);
                };
                let next = fields[(at + 1) % fields.len()].0;
                let edited = self.commit_card_field(ctx);
                self.focus_metric(next);
                (edited, true)
            }
            Key::Named(NamedKey::Enter) => {
                let edited = self.commit_card_field(ctx);
                self.field = None;
                (edited, true)
            }
            _ => (false, false),
        }
    }

    /// Put the caret in a box and seed it with the value it shows.
    pub(super) fn focus_metric(&mut self, field: MetricField) {
        self.kern_typed = false;
        self.field_buf = match (field, self.session.side_bearings()) {
            (MetricField::GroupLeft | MetricField::GroupRight, _) => {
                self.card_group(field).1.to_owned()
            }
            (MetricField::KernLeft | MetricField::KernRight, _) => {
                self.kern_groups = self
                    .kern_pair(field)
                    .map(|(a, b, _)| self.text.as_ref().unwrap().buffer.kern_scope(a, b))
                    .unwrap_or_default();
                self.kern_target(field, self.kern_groups)
                    .and_then(|(_, _, v)| v)
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            }
            (MetricField::Lsb, Some(sb)) => sb.lsb.to_string(),
            (MetricField::Width, Some(sb)) => format!("{:.0}", sb.advance),
            (MetricField::Rsb, Some(sb)) => sb.rsb.to_string(),
            // A blank glyph has no ink bounds, so its sidebearings are
            // undefined. Keep the controls usable: width remains its stored
            // advance, while a right sidebearing is the full advance from the
            // origin to the right margin.
            (MetricField::Lsb, None) => "0".into(),
            (MetricField::Width | MetricField::Rsb, None) => {
                format!("{:.0}", self.session.advance())
            }
        };
        self.field = Some(field);
    }

    /// Apply what was typed. The sidebearing rules are the ones the
    /// inspector's fields use: the left one moves the outline, the right
    /// one moves the advance.
    pub(super) fn commit_metric(&mut self) -> bool {
        let Some(field) = self.field else {
            return false;
        };
        let Ok(value) = self.field_buf.trim().parse::<f64>() else {
            return false;
        };
        match field {
            MetricField::GroupLeft
            | MetricField::GroupRight
            | MetricField::KernLeft
            | MetricField::KernRight => return false,
            MetricField::Lsb => {
                if let Some(sb) = self.session.side_bearings() {
                    self.session.shift_glyph(value - sb.min_x);
                } else {
                    return false;
                }
            }
            MetricField::Width => self.session.set_advance(value),
            MetricField::Rsb => {
                let width = self
                    .session
                    .side_bearings()
                    .map_or(value, |sb| sb.max_x + value);
                self.session.set_advance(width);
            }
        }
        true
    }

    pub(super) fn paint_metrics(&self, painter: &mut Painter<'_>) {
        const PAD: f64 = PANEL_PAD;
        // A neural item has no advance or sidebearings to show.
        if self.session.neural {
            return;
        }
        let pal = &self.palette;
        let bearings = self.session.side_bearings();
        let Some((left, top)) = self.metrics_panel_origin() else {
            return;
        };
        let width = PANEL_WIDTH;
        let frame = Rect::new(left, top, left + width, top + PANEL_HEIGHT);
        let rounded = crate::application::design::rounded_rect_path;
        let radius = pal.corner_radius;
        painter
            .fill(
                rounded(frame + kurbo::Vec2::new(-4.0, 4.0), radius),
                pal.cell_shadow().with_alpha(0.5),
            )
            .draw();
        painter.fill(rounded(frame, radius), pal.panel).draw();
        let (header_bg, header_ink) = pal.floating_header_colors(self.mark);
        painter.with_fill_clip(rounded(frame, radius), |painter| {
            painter
                .fill(
                    Rect::new(left, top, left + width, top + PANEL_HEADER),
                    header_bg,
                )
                .draw();
        });
        painter
            .stroke(
                rounded(frame, radius),
                &Stroke::new(DesignStroke::Hairline.px()),
                pal.outline,
            )
            .draw();
        painter
            .stroke(
                Line::new(
                    (left, top + PANEL_HEADER),
                    (left + width, top + PANEL_HEADER),
                ),
                &Stroke::new(DesignStroke::Hairline.px()),
                pal.outline,
            )
            .draw();

        let header_text = |painter: &mut Painter<'_>, x: f64, s: &str, size: f32, anchor| {
            text_label::draw(
                painter,
                Point::new(left + x, top + PANEL_HEADER / 2.0),
                s,
                size,
                header_ink,
                anchor,
            );
        };
        // Inside a component, the header says which composite it is open in.
        let title = match self.field {
            Some(field @ (MetricField::KernLeft | MetricField::KernRight)) => {
                self.kern_pair(field).map(|(a, b, _)| {
                    if self.scope_index(MetricField::GroupLeft) == 1 {
                        format!("{b} · {a}")
                    } else {
                        format!("{a} · {b}")
                    }
                })
            }
            _ => None,
        }
        .unwrap_or_else(|| match &self.session.host {
            Some(host) => format!("{} in {}", self.session.glyph_name, host.root),
            None => self.session.glyph_name.clone(),
        });
        let feedback = match &self.drag {
            Drag::Kern {
                first,
                second,
                rtl,
                value,
                ..
            } => {
                let trim = |n: &String| {
                    n.trim_start_matches("public.kern1.")
                        .trim_start_matches("public.kern2.")
                        .to_string()
                };
                let (left, right) = if *rtl {
                    (second, first)
                } else {
                    (first, second)
                };
                Some((
                    format!("{} · {}", trim(left), trim(right)),
                    format!("{value:+.0}"),
                ))
            }
            _ if self.tool == Tool::Text && self.field.is_none() => self
                .kern_hover
                .as_ref()
                .map(|(_, pair)| (pair.clone(), "Option-drag to kern".into())),
            _ => None,
        };
        if let Some((pair, hint)) = &feedback {
            let size = TextSize::Body.px();
            text_label::draw_elided(
                painter,
                Point::new(left + PAD, top + PANEL_HEADER / 2.0),
                pair,
                size,
                header_ink,
                Anchor::Start,
                (width - 3.0 * PAD - text_label::width(hint, size)).max(0.0),
            );
            header_text(painter, width - PAD, hint, size, Anchor::End);
        } else {
            header_text(painter, PAD, &title, TextSize::Body.px(), Anchor::Start);
        }
        // The codepoint, right aligned on the same line, as the GPUI
        // build has it. This one is free because the session carries the
        // glyph; the kerning groups it also shows are not, because they
        // live in the font model, and reaching them means threading two
        // strings through the widget, the view, `build`, `rebuild` and
        // the constructor.
        if feedback.is_none()
            && let Some(codepoint) = self.session.codepoint()
        {
            header_text(
                painter,
                width - PAD,
                &format!("{:04X}", codepoint as u32),
                TextSize::Body.px(),
                Anchor::End,
            );
        }
        // Three boxes you can type in. Each one is drawn here and hit tested
        // from the same rectangles, because a painted control that computes
        // its geometry twice will drift the moment either copy is edited.
        //
        // The card is glyph chrome, not an outline measurement. Keep its
        // inputs present when the final point is deleted. An empty glyph has
        // no measured ink bounds, so the left value is the origin and the
        // right value is its full advance.
        if let Some(boxes) = self.metric_boxes() {
            let middle = boxes[1].1.center();
            text_label::draw(
                painter,
                Point::new(middle.x, middle.y + PANEL_ROW + 4.0),
                METRIC_CAPTION,
                TextSize::Body.px(),
                pal.text,
                Anchor::Middle,
            );
            for (field, rect) in boxes {
                let focused = self.field == Some(field);
                let mut ink = pal.text;
                let value = if focused {
                    self.field_buf.clone()
                } else {
                    match (field, bearings) {
                        (MetricField::GroupLeft | MetricField::GroupRight, _) => {
                            if matches!(
                                self.field,
                                Some(MetricField::KernLeft | MetricField::KernRight)
                            ) {
                                if self.kern_groups[self.scope_index(field)] {
                                    "Group".into()
                                } else {
                                    "Glyph".into()
                                }
                            } else {
                                let name = self.card_group(field).1;
                                if name.is_empty() {
                                    ink = pal.placeholder_ink();
                                    "Group".into()
                                } else {
                                    name.to_owned()
                                }
                            }
                        }
                        (MetricField::KernLeft | MetricField::KernRight, _) => self
                            .kern_pair(field)
                            .map(|(_, _, v)| {
                                if v == 0.0
                                    && ![[false, false], [false, true], [true, false], [true, true]]
                                        .into_iter()
                                        .any(|scope| {
                                            self.kern_target(field, scope)
                                                .is_some_and(|(_, _, v)| v.is_some())
                                        })
                                {
                                    ink = pal.placeholder_ink();
                                    "Kern".into()
                                } else {
                                    format!("{v}")
                                }
                            })
                            .unwrap_or_default(),
                        (MetricField::Lsb, Some(sb)) => sb.lsb.to_string(),
                        (MetricField::Width, Some(sb)) => format!("{:.0}", sb.advance),
                        (MetricField::Rsb, Some(sb)) => sb.rsb.to_string(),
                        (MetricField::Lsb, None) => "0".into(),
                        (MetricField::Width | MetricField::Rsb, None) => {
                            format!("{:.0}", self.session.advance())
                        }
                    }
                };
                let border = if focused { pal.text } else { pal.field_outline };
                painter
                    .fill(rounded(rect, pal.control_radius), pal.field())
                    .draw();
                // Like the native inputs, the field's keyline stays inside
                // its bounds. A centered exterior stroke blurs the edge.
                let width = DesignStroke::Hairline.px();
                let half = width / 2.0;
                let keyline = Rect::new(
                    rect.x0 + half,
                    rect.y0 + half,
                    rect.x1 - half,
                    rect.y1 - half,
                );
                painter
                    .stroke(
                        rounded(keyline, (pal.control_radius - half).max(0.0)),
                        &Stroke::new(width),
                        border,
                    )
                    .draw();
                let size = TextSize::Body.px();
                let available =
                    rect.width() - 2.0 * crate::application::design::INPUT_HORIZONTAL_INSET;
                if focused {
                    // Keep the insertion point visible as a long name is typed.
                    let mut visible = value.as_str();
                    while text_label::width(visible, size) > available {
                        let Some(first) = visible.chars().next() else {
                            break;
                        };
                        visible = &visible[first.len_utf8()..];
                    }
                    text_label::draw(painter, rect.center(), visible, size, ink, Anchor::Middle);
                    let end = rect.center().x + text_label::width(visible, size) / 2.0;
                    painter
                        .fill(
                            Rect::new(end, rect.y0 + 3.0, end + 1.0, rect.y1 - 3.0),
                            pal.outline,
                        )
                        .draw();
                } else {
                    text_label::draw_elided(
                        painter,
                        rect.center(),
                        &value,
                        size,
                        ink,
                        Anchor::Middle,
                        available,
                    );
                }
            }
        }
    }
}
