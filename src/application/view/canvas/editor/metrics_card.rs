// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The floating metrics card: its field positions, key handling and painting.

use super::*;

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
    pub(super) fn metric_boxes(&self) -> Option<[(MetricField, Rect); 3]> {
        if self.preview_mode || self.session.neural {
            return None;
        }
        let (left, top) = self.metrics_panel_origin()?;
        let y = top + PANEL_HEADER + PANEL_PAD + DesignStroke::Hairline.px();
        let box_at = |column: f64| {
            let x = left + METRICS_FIELD_START + column * (METRICS_FIELD_WIDTH + METRICS_FIELD_GAP);
            Rect::new(x, y, x + METRICS_FIELD_WIDTH, y + PANEL_ROW)
        };
        Some([
            (MetricField::Lsb, box_at(0.0)),
            (MetricField::Width, box_at(1.0)),
            (MetricField::Rsb, box_at(2.0)),
        ])
    }

    /// The panel's top left corner, or `None` when it does not fit.
    pub(super) fn metrics_panel_origin(&self) -> Option<(f64, f64)> {
        let top = self.size.height - PANEL_HEIGHT - METRICS_CARD_BOTTOM;
        if top < 0.0 || PANEL_WIDTH + PANEL_PAD * 2.0 > self.size.width {
            return None;
        }
        Some(((self.size.width - PANEL_WIDTH) / 2.0, top))
    }

    pub(super) fn metric_key(&mut self, key: &masonry::core::KeyboardEvent) -> (bool, bool) {
        let Some(field) = self.field else {
            return (false, false);
        };
        match &key.key {
            Key::Character(typed) => {
                for character in typed.chars() {
                    if character.is_ascii_digit() || character == '-' {
                        self.field_buf.push(character);
                    }
                }
                (false, true)
            }
            Key::Named(NamedKey::Backspace) => {
                self.field_buf.pop();
                (false, true)
            }
            Key::Named(NamedKey::Escape) => {
                self.field = None;
                (false, true)
            }
            Key::Named(NamedKey::Tab) => {
                let next = match field {
                    MetricField::Lsb => MetricField::Width,
                    MetricField::Width => MetricField::Rsb,
                    MetricField::Rsb => MetricField::Lsb,
                };
                let edited = self.commit_metric();
                self.focus_metric(next);
                (edited, true)
            }
            Key::Named(NamedKey::Enter) => {
                let edited = self.commit_metric();
                self.field = None;
                (edited, true)
            }
            _ => (false, false),
        }
    }

    /// Put the caret in a box and seed it with the value it shows.
    pub(super) fn focus_metric(&mut self, field: MetricField) {
        self.field_buf = match (field, self.session.side_bearings()) {
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
        // Keep read-only group labels inside their end columns. Full names
        // remain available in the inspector's kerning group fields.
        let group_label = |name: &str| {
            let name = name
                .strip_prefix("public.kern1.")
                .or_else(|| name.strip_prefix("public.kern2."))
                .unwrap_or(name);
            if name.chars().count() > 6 {
                format!("{}…", name.chars().take(5).collect::<String>())
            } else {
                name.to_owned()
            }
        };
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
        let title = match &self.session.host {
            Some(host) => format!("{} in {}", self.session.glyph_name, host.root),
            None => self.session.glyph_name.clone(),
        };
        header_text(painter, PAD, &title, TextSize::Body.px(), Anchor::Start);
        // The codepoint, right aligned on the same line, as the GPUI
        // build has it. This one is free because the session carries the
        // glyph; the kerning groups it also shows are not, because they
        // live in the font model, and reaching them means threading two
        // strings through the widget, the view, `build`, `rebuild` and
        // the constructor.
        if let Some(codepoint) = self.session.codepoint() {
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
            let group_box = |column: f64| {
                let x = left + PANEL_PAD + column * (METRICS_FIELD_WIDTH + METRICS_FIELD_GAP);
                Rect::new(x, boxes[0].1.y0, x + METRICS_FIELD_WIDTH, boxes[0].1.y1)
            };
            for (rect, group) in [
                (group_box(0.0), &self.groups.0),
                (group_box(4.0), &self.groups.1),
            ] {
                painter
                    .fill(rounded(rect, pal.control_radius), pal.field())
                    .draw();
                let line = DesignStroke::Hairline.px();
                let half = line / 2.0;
                painter
                    .stroke(
                        rounded(
                            Rect::new(
                                rect.x0 + half,
                                rect.y0 + half,
                                rect.x1 - half,
                                rect.y1 - half,
                            ),
                            (pal.control_radius - half).max(0.0),
                        ),
                        &Stroke::new(line),
                        pal.field_outline,
                    )
                    .draw();
                text_label::draw(
                    painter,
                    rect.center(),
                    &group_label(group),
                    TextSize::Body.px(),
                    pal.text,
                    Anchor::Middle,
                );
            }
            for (field, rect) in boxes {
                let focused = self.field == Some(field);
                let value = if focused {
                    self.field_buf.clone()
                } else {
                    match (field, bearings) {
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
                let baseline = rect.center().y;
                text_label::draw(
                    painter,
                    Point::new(
                        rect.x0 + crate::application::design::INPUT_HORIZONTAL_INSET,
                        baseline,
                    ),
                    &value,
                    TextSize::Body.px(),
                    pal.text,
                    Anchor::Start,
                );
                if focused {
                    // A caret, drawn by hand, because this is a text
                    // field drawn by hand.
                    let caret =
                        Rect::new(rect.x1 - 4.0, rect.y0 + 3.0, rect.x1 - 3.0, rect.y1 - 3.0);
                    painter.fill(caret, pal.outline).draw();
                }
            }
        }
    }
}
