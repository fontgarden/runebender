// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Measure tool: snapping, the scan line, and painting the ruler and its tags.

use super::*;

impl EditorWidget {
    /// The label tool's view of the glyph: each letter's ink in that letter's color, the regions
    /// of the active letter, and the polygon being placed. Ink no letter owns keeps the plain
    /// outline, so what is left to label is what is not colored.
    /// The point the Measure tool puts a ruler end on: a point within reach, or else the
    /// pointer itself, in design space.
    pub(super) fn measure_snap(&self, at: Point) -> Point {
        let affine = self.glyph_affine();
        self.session
            .points()
            .into_iter()
            .map(|p| (p.point, (affine * p.point).distance(at)))
            .filter(|(_, distance)| *distance <= HIT_RADIUS_PX)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map_or_else(|| self.screen_to_glyph_design(at), |(point, _)| point)
    }

    /// The Measure tool's scan line through `at`: across the advance, or with Option held,
    /// from descender to ascender, stretched to reach any ink outside those.
    fn scan_line(&self, at: Point, ink: &kurbo::BezPath) -> Line {
        let bounds = if ink.elements().is_empty() {
            Rect::from_points(at, at)
        } else {
            ink.bounding_box()
        };
        if self.hover_vertical {
            let m = &self.session.metrics;
            Line::new(
                (at.x, m.descender.min(bounds.y0)),
                (at.x, m.ascender.max(bounds.y1)),
            )
        } else {
            Line::new(
                (bounds.x0.min(0.0), at.y),
                (bounds.x1.max(self.session.advance()), at.y),
            )
        }
    }

    /// The Measure tool, as in other font editors: the dragged ruler, or else a scan line
    /// through the pointer. Each is cut where it crosses the outline and every piece is
    /// labeled, ink in full text color and counters and margins muted.
    pub(super) fn paint_ruler(&self, painter: &mut Painter<'_>, affine: Affine) {
        let pal = &self.palette;
        let mut ink = self.session.outline();
        ink.extend(self.session.components.elements().iter().copied());
        let (line, scan) = match (self.ruler, self.hover) {
            (Some(ruler), _) => (ruler, false),
            (None, Some(hover)) if matches!(self.drag, Drag::None) => {
                (self.scan_line(hover, &ink), true)
            }
            _ => return,
        };
        let ruler = runebender::analysis::measure::ruler(&ink, line);
        let color = if scan {
            pal.role("metricQuiet")
        } else {
            pal.tool_feedback()
        };
        let (p0, p1) = (affine * line.p0, affine * line.p1);
        painter
            .stroke(Line::new(p0, p1), &Stroke::new(pal.stroke), color)
            .draw();
        for crossing in &ruler.crossings {
            painter
                .fill(Circle::new(affine * *crossing, 3.0), color)
                .draw();
        }
        // A ruler that crosses nothing is one piece, and its end label says it all.
        if scan || ruler.pieces.len() > 1 {
            for piece in &ruler.pieces {
                let (a, b) = (affine * piece.a, affine * piece.b);
                if a.distance(b) < 16.0 {
                    continue;
                }
                let text = self.view.label(piece.units());
                let ink = if piece.ink { pal.text } else { pal.text_muted };
                self.measure_tag(painter, a.midpoint(b), &text, ink, Anchor::Middle);
            }
        }
        if !scan && line.length() >= 1.0 {
            let d = line.p1 - line.p0;
            let angle = d.y.atan2(d.x).to_degrees();
            let text = format!(
                "{} · {angle:.0}°",
                self.view
                    .label(runebender::outline::glyph_paths::round_units(line.length()))
            );
            let away = (p1 - p0).normalize() * 14.0;
            let anchor = if away.x < 0.0 {
                Anchor::End
            } else {
                Anchor::Start
            };
            self.measure_tag(painter, p1 + away, &text, color, anchor);
        }
    }

    /// A measurement label on a backdrop, so it reads over ink and outline alike.
    fn measure_tag(
        &self,
        painter: &mut Painter<'_>,
        at: Point,
        text: &str,
        color: xilem::Color,
        anchor: Anchor,
    ) {
        let size = TextSize::Caption.px();
        let width = text_label::width(text, size);
        let x0 = match anchor {
            Anchor::Start => at.x,
            Anchor::Middle => at.x - width / 2.0,
            Anchor::End => at.x - width,
        };
        let half = f64::from(size) * 0.75;
        let backdrop = Rect::new(x0 - 3.0, at.y - half, x0 + width + 3.0, at.y + half);
        painter
            .fill(backdrop, self.palette.point_halo_fill())
            .draw();
        text_label::draw(painter, at, text, size, color, anchor);
    }
}
