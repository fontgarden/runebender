// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The label tool overlay: region colors, painting and the caption.

use super::tools::label::LabelDrag;
use super::*;

/// One color per letter for the label tool, cycling through the theme's mark colors.
/// The colors of labeled letters: the theme's mark palette in its own order, cycling from
/// red through the rainbow and back. The same colors the glyph grid uses.
pub(crate) fn label_colors(pal: &Palette) -> Vec<xilem::Color> {
    let marks: Vec<xilem::Color> = pal
        .mark_list()
        .into_iter()
        .map(|(_, color)| color)
        .collect();
    if marks.is_empty() {
        vec![pal.tool_feedback()]
    } else {
        marks
    }
}

impl EditorWidget {
    pub(super) fn paint_labels(&self, painter: &mut Painter<'_>, affine: Affine) {
        use crate::application::editor::tools::label::{LabelHit, LabelSelection};
        let pal = &self.palette;
        let colors = label_colors(pal);
        let outline = affine * self.session.outline();
        let selected = self.session.label.sample;
        let active = self.session.label.active;
        let lit_letter = self.session.label.hover_letter;
        // The samples' loops show what can be picked while no sample is open. Inside a
        // sample, the tinted ink says what belongs to it, and the loop would only be clutter.
        if selected.is_none() {
            for sample in &self.session.neural_item().samples {
                painter
                    .stroke(
                        affine * sample.boundary_path(),
                        &Stroke::new(pal.stroke),
                        pal.text_muted.with_alpha(0.4),
                    )
                    .draw();
            }
        }
        // Labeled ink in its letter's color. Shared ink shows both colors, one over the other.
        for area in self.session.label_areas() {
            let color = colors[area.letter % colors.len()];
            let lit = Some(area.sample) == selected && lit_letter == Some(area.letter);
            let path = affine * area.area;
            painter.with_fill_clip(&path, |painter| {
                painter.fill(&outline, color.with_alpha(0.85)).draw();
                if lit {
                    painter.fill(&outline, pal.canvas.with_alpha(0.35)).draw();
                }
            });
        }
        // Every lasso polygon: a translucent fill in its letter's color and the same dark
        // outline as the drawing. The active letter's and the selected one carry colored
        // corner handles.
        let picked_corners = self.session.selected_corners();
        let shared = self.session.shared_corners();
        let picked_region = match &self.session.label.selected {
            Some(LabelSelection::Region(region)) => Some(*region),
            _ => None,
        };
        for polygon in self.session.label_polygons() {
            let color = colors[polygon.letter % colors.len()];
            let mut path = kurbo::BezPath::new();
            for (index, corner) in polygon.corners.iter().enumerate() {
                let at = affine * *corner;
                if index == 0 {
                    path.move_to(at);
                } else {
                    path.line_to(at);
                }
            }
            path.close_path();
            painter.fill(&path, color.with_alpha(0.14)).draw();
            painter
                .stroke(&path, &Stroke::new(pal.stroke), pal.role("pathStroke"))
                .draw();
            // Every shape shows its corners; the active letter's and the picked ones are
            // larger.
            let strong = polygon.letter == active
                || picked_region == Some(polygon.region)
                || picked_corners
                    .iter()
                    .any(|(region, _)| *region == polygon.region);
            for (index, corner) in polygon.corners.iter().enumerate() {
                if shared
                    .iter()
                    .any(|(at, _)| at.distance(*corner) <= LABEL_GRID / 2.0)
                {
                    continue;
                }
                let picked = picked_corners.contains(&(polygon.region, index));
                let radius = if picked {
                    5.0
                } else if strong {
                    3.5
                } else {
                    2.5
                };
                let handle = Circle::new(affine * *corner, radius);
                painter.fill(handle, color).draw();
                painter
                    .stroke(handle, &Stroke::new(pal.stroke), pal.role("pathStroke"))
                    .draw();
            }
            // A corner would go here: the edge under the pointer shows it.
            if let Some(LabelHit::Edge { region, at, .. }) = &self.label_hover
                && *region == polygon.region
            {
                let ghost = Circle::new(affine * *at, 3.5);
                painter.fill(ghost, color.with_alpha(0.5)).draw();
                painter
                    .stroke(ghost, &Stroke::new(pal.stroke), pal.role("pathStroke"))
                    .draw();
            }
        }
        // Where corners of different shapes meet: one node in wedges, one color each.
        for (at, letters) in &shared {
            let picked = picked_corners.iter().any(|(region, corner)| {
                self.session
                    .corner_point(*region, *corner)
                    .is_some_and(|p| p.distance(*at) <= LABEL_GRID / 2.0)
            });
            let center = affine * *at;
            let radius = if picked { 6.0 } else { 4.5 };
            // A full disc in the first color under the wedges, so the seams between wedges
            // and the rim show no ground.
            painter
                .fill(
                    Circle::new(center, radius),
                    colors[letters[0] % colors.len()],
                )
                .draw();
            let step = std::f64::consts::TAU / letters.len() as f64;
            for (slice, letter) in letters.iter().enumerate().skip(1) {
                let start = -std::f64::consts::FRAC_PI_2 + step * slice as f64;
                // From the center to the rim, around the arc, and back.
                let arc = kurbo::Arc::new(center, (radius, radius), start, step, 0.0);
                let mut wedge = kurbo::BezPath::new();
                wedge.move_to(center);
                wedge.line_to(center + kurbo::Vec2::new(start.cos(), start.sin()) * radius);
                wedge.extend(arc.append_iter(0.1));
                wedge.close_path();
                painter.fill(&wedge, colors[*letter % colors.len()]).draw();
            }
            painter
                .stroke(
                    Circle::new(center, radius),
                    &Stroke::new(pal.stroke),
                    pal.role("pathStroke"),
                )
                .draw();
        }
        if let Drag::Label(LabelDrag::Marquee { start, current }) = &self.drag {
            let rect = affine.transform_rect_bbox(Rect::from_points(*start, *current));
            painter
                .fill(rect, pal.role("selection").with_alpha(0.1))
                .draw();
            painter
                .stroke(rect, &Stroke::new(pal.stroke), pal.role("selection"))
                .draw();
        }
        // A freehand drag in progress: a cut is a straight line, a lasso a loop.
        if let Drag::Label(LabelDrag::Freehand { points }) = &self.drag
            && points.len() >= 2
        {
            let color = if selected.is_some() {
                colors[active % colors.len()]
            } else {
                pal.tool_feedback()
            };
            let mut path = kurbo::BezPath::new();
            path.move_to(points[0]);
            for point in &points[1..] {
                path.line_to(*point);
            }
            path.close_path();
            painter.fill(&path, color.with_alpha(0.12)).draw();
            painter.stroke(&path, &Stroke::new(1.5), color).draw();
        }
        // What a click does, in the corner of the canvas.
        if let Some(caption) = self.label_caption() {
            text_label::draw(
                painter,
                Point::new(PANEL_PAD + 2.0, self.size.height - PANEL_PAD - 10.0),
                &caption,
                12.0,
                pal.text_muted,
                Anchor::Start,
            );
        }
    }

    /// What a click would do where the label tool's pointer is.
    pub(super) fn label_caption(&self) -> Option<String> {
        use crate::application::editor::tools::label::LabelHit;
        if let Some(error) = &self.session.label.error {
            return Some(error.clone());
        }
        if self.session.label.sample.is_none() {
            return Some(
                "Click a word to label it. Drag a loop around writing for a new one".into(),
            );
        }
        let letter = self.session.active_letter().map(|(_, c)| c);
        let Some(letter) = letter else {
            return Some("Type the sample's text in the panel".into());
        };
        Some(match &self.label_hover {
            Some(LabelHit::Corner { .. }) => {
                "Drag: move the corner with any that meet it. Option-drag: only this one".into()
            }
            Some(LabelHit::Edge { .. }) => "Click: add a corner".into(),
            Some(LabelHit::Lasso { .. }) => "Click: pick this shape. Drag: move it".into(),
            _ => format!("Drag a loop around the ink of {letter}. Enter: next letter"),
        })
    }
}
