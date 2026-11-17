// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The paint pass of the editor widget, from the canvas background to the metrics card.
//! The functions that build the thickness image are also here.

use super::tools::label::LabelDrag;
use super::*;
use crate::application::design::point_marker::{self, PointKind};

/// Cells along the longer side of the thickness field's grid.
const THICKNESS_CELLS: usize = 256;

/// Cache key for the thickness image: the outline's geometry and the ramp's colors.
fn thickness_key(outline: &kurbo::BezPath, pal: &Palette) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for element in outline.elements() {
        std::mem::discriminant(element).hash(&mut hasher);
        let points: &[Point] = match element {
            kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => std::slice::from_ref(p),
            kurbo::PathEl::QuadTo(a, b) => &[*a, *b],
            kurbo::PathEl::CurveTo(a, b, c) => &[*a, *b, *c],
            kurbo::PathEl::ClosePath => &[],
        };
        for p in points {
            p.x.to_bits().hash(&mut hasher);
            p.y.to_bits().hash(&mut hasher);
        }
    }
    for t in [0.0, 0.5, 1.0] {
        for component in pal.thickness_fill(t).components {
            component.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Rasterize the thickness field of `outline` as a straight-alpha RGBA image,
/// with the affine that places its pixels in design space.
fn thickness_image(outline: &kurbo::BezPath, pal: &Palette) -> Option<(ImageData, Affine)> {
    let field = runebender::analysis::thickness::thickness_field(outline, THICKNESS_CELLS)?;
    let mut rgba = vec![0_u8; field.width * field.height * 4];
    for (k, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let coverage = field.coverage[k];
        if coverage <= 0.0 {
            continue;
        }
        let color = pal
            .thickness_fill(field.ramp(f64::from(field.thickness[k])))
            .multiply_alpha(coverage)
            .to_rgba8();
        pixel.copy_from_slice(&[color.r, color.g, color.b, color.a]);
    }
    let image = ImageData {
        data: Blob::new(Arc::new(rgba)),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: u32::try_from(field.width).ok()?,
        height: u32::try_from(field.height).ok()?,
    };
    Some((image, field.pixel_to_design()))
}

impl EditorWidget {
    /// The other parts' outlines, handles and points, for View > Components.
    ///
    /// They use the open glyph's markers without the grid window or start arrows, so the
    /// glyph being edited stays the most detailed thing on the canvas.
    fn paint_part_points(&self, painter: &mut Painter<'_>, sort_affine: Affine) {
        let pal = &self.palette;
        let widths = point_marker::widths(self.session.viewport.zoom);
        for part in self
            .session
            .parts
            .iter()
            .filter(|part| part.owner != PartOwner::Fixed)
        {
            painter
                .stroke(
                    &(sort_affine * part.outline.clone()),
                    &Stroke::new(pal.stroke),
                    pal.role("pathStroke"),
                )
                .draw();
            for line in &part.handles {
                painter
                    .stroke(
                        sort_affine * *line,
                        &Stroke::new(pal.stroke),
                        pal.handle_line,
                    )
                    .draw();
            }
            for point in &part.points {
                let kind = PointKind::of(point.on_curve, point.smooth);
                let ink = point_marker::ink(pal, kind.hue(pal), false, pal.canvas);
                let shape = point_marker_shape(
                    sort_affine * point.point,
                    kind.radius() * widths.scale,
                    kind.is_square(),
                );
                point_marker::paint(painter, &shape, ink, point_marker::halo(pal), widths);
            }
        }
    }

    /// Paint the canvas.
    /// [`Widget::paint`] calls this.
    /// Each step paints one layer over the steps before it.
    pub(super) fn paint_canvas(&mut self, painter: &mut Painter<'_>) {
        let pal = self.palette.clone();
        painter.fill_rect(self.size.to_rect(), pal.canvas);

        // Holding Space is both the temporary Hand tool and the standard font
        // editor preview: show the filled design without nodes, handles, grid,
        // metrics, analyses, underlays, or other editing chrome. GPUI and Web
        // use this same press/release lifecycle, so panning never changes the
        // persistent tool and releasing Space restores the normal drawing.
        if self.preview_mode {
            self.paint_preview(painter, &pal);
            return;
        }

        if self.tool == Tool::Interpolation {
            self.paint_interpolation(painter);
            return;
        }

        // A text composition outlives the Text tool. With Text active every
        // sort is a fill and the caret is visible; with an outline tool the
        // active sort is omitted here and the editable glyph chrome below is
        // drawn at that sort's origin. This is the GPUI/Web state model.
        if let Some(text) = &self.text
            && !self.paint_text_line(painter, &pal, text)
        {
            return;
        }

        let affine = self.glyph_affine();
        self.paint_underlay(painter, &pal, affine);
        self.paint_layer_ghosts(painter, &pal, affine);
        self.paint_design_grid(painter, &pal, affine);
        self.paint_metric_frame(painter, &pal);

        // Editing affordances only render on a master. Off a master the view
        // shows the read-only interpolated instance instead (web/Glyphs
        // behavior): swap the outline, don't ghost it behind an editable one.
        if self.interp.is_none() {
            self.paint_composite_parts(painter, &pal, affine);
            self.paint_curvature_comb(painter, &pal, affine);
            self.paint_outline(painter, &pal, affine);
            self.paint_points(painter, &pal, affine);
            self.paint_pen_previews(painter, &pal, affine);
            self.paint_anchors(painter, &pal, affine);
        } else if let Some(interp) = &self.interp {
            self.paint_instance(painter, &pal, affine, interp);
        }
        self.paint_continuity(painter, &pal, affine);
        self.paint_colorize(painter, &pal, affine);
        self.paint_measurements(painter, &pal, affine);
        self.paint_tool_overlays(painter, &pal, affine);
        self.paint_metrics(painter);
    }

    /// Paint the filled design alone, for the preview that Space shows.
    fn paint_preview(&self, painter: &mut Painter<'_>, pal: &Palette) {
        let view_affine = self.session.viewport.affine();
        if let Some(data) = &self.hoi {
            painter
                .fill(&*data.preview, pal.editor_ink())
                .transform(view_affine)
                .draw();
        } else if let Some(text) = &self.text {
            for sort in text.placed() {
                painter
                    .fill(&*sort.outline, pal.editor_ink())
                    .transform(view_affine * Affine::translate(sort.origin.to_vec2()))
                    .draw();
            }
        } else if let Some(interp) = &self.interp {
            painter
                .fill(&**interp, pal.editor_ink())
                .transform(view_affine)
                .draw();
        } else {
            painter
                .fill(self.session.outline(), pal.editor_ink())
                .transform(view_affine)
                .draw();
            if !self.session.components.elements().is_empty() {
                painter
                    .fill(&self.session.components, pal.editor_ink())
                    .transform(view_affine)
                    .draw();
            }
        }
    }

    /// Paint the sorts, metric marks and caret of the text composition.
    /// Returns whether the editable glyph chrome is painted after it, at the active sort.
    fn paint_text_line(
        &self,
        painter: &mut Painter<'_>,
        pal: &Palette,
        text: &crate::application::editor::tools::text::TextState,
    ) -> bool {
        let view_affine = self.session.viewport.affine();
        let m = &self.session.metrics;
        let ink = pal.editor_ink();
        let sort_top = m.upm.max(m.ascender);
        let sort_bottom = m.descender;
        let sort_height_px = ((sort_top - sort_bottom) * self.session.viewport.zoom).abs();
        let mark = (sort_height_px * 0.05).clamp(1.5, 24.0);
        let metric_ys = text_sort_metric_ys(m);
        let placed = text.placed();
        let rule = Stroke::new(DesignStroke::Hairline.px());
        for sort in &placed {
            let box_ = Rect::from_points(
                view_affine * Point::new(sort.origin.x, sort_bottom + sort.origin.y),
                view_affine * Point::new(sort.origin.x + sort.advance, sort_top + sort.origin.y),
            );
            if self.tool == Tool::Text && sort.selected {
                painter
                    .fill(box_, pal.role("selection").with_alpha(0.32))
                    .draw();
            }
            if mark >= 3.0 && !sort.active {
                let quiet = pal.role("metricQuiet");
                for x in [box_.x0, box_.x1] {
                    painter
                        .stroke(Line::new((x, box_.y0), (x, box_.y1)), &rule, quiet)
                        .draw();
                }
                for &y in &metric_ys {
                    let sy = (view_affine * Point::new(sort.origin.x, sort.origin.y + y)).y;
                    painter
                        .stroke(Line::new((box_.x0, sy), (box_.x1, sy)), &rule, quiet)
                        .draw();
                }
            }
            if !sort.active || self.tool == Tool::Text {
                painter
                    .fill(&*sort.outline, ink)
                    .transform(view_affine * Affine::translate(sort.origin.to_vec2()))
                    .draw();
            }
        }
        // Draw crosses over every sort's quiet rules, including shared
        // boundaries. The active sort's marks go last when Text owns it;
        // outline tools draw them with the active glyph frame below.
        if mark >= 3.0 {
            for sort in placed.iter().filter(|sort| !sort.active).chain(
                placed
                    .iter()
                    .filter(|sort| sort.active && self.tool == Tool::Text),
            ) {
                let x0 = (view_affine * sort.origin).x;
                let x1 = (view_affine * Point::new(sort.origin.x + sort.advance, sort.origin.y)).x;
                let ys = metric_ys
                    .iter()
                    .map(|&y| (view_affine * Point::new(sort.origin.x, sort.origin.y + y)).y);
                let top = (view_affine * Point::new(0.0, sort.origin.y + sort_top)).y;
                let bottom = (view_affine * Point::new(0.0, sort.origin.y + sort_bottom)).y;
                paint_metric_crosses(painter, [x0, x1], [top, bottom], ys, mark, pal.outline);
            }
        }
        if self.tool == Tool::Text && self.cursor_visible {
            // The caret follows GPUI and Web: a full sort-height rule with
            // inward triangular caps scaled from its on-screen height. It
            // uses the same single outline stroke as the sort corners.
            let caret = text.caret();
            let top = view_affine * Point::new(caret.x, caret.y + sort_top);
            let bottom = view_affine * Point::new(caret.x, caret.y + sort_bottom);
            let cursor = pal.outline;
            painter
                .stroke(
                    Line::new(top, bottom),
                    &Stroke::new(DesignStroke::Hairline.px()),
                    cursor,
                )
                .draw();
            let triangle_width = (sort_height_px * TEXT_CURSOR_CAP_FRACTION)
                .clamp(TEXT_CURSOR_CAP_MIN, TEXT_CURSOR_CAP_MAX);
            let triangle_height = triangle_width * (2.0 / 3.0);
            let mut top_cap = kurbo::BezPath::new();
            top_cap.move_to((top.x - triangle_width / 2.0, top.y));
            top_cap.line_to((top.x + triangle_width / 2.0, top.y));
            top_cap.line_to((top.x, top.y + triangle_height));
            top_cap.close_path();
            painter.fill(&top_cap, cursor).draw();
            let mut bottom_cap = kurbo::BezPath::new();
            bottom_cap.move_to((bottom.x - triangle_width / 2.0, bottom.y));
            bottom_cap.line_to((bottom.x + triangle_width / 2.0, bottom.y));
            bottom_cap.line_to((bottom.x, bottom.y - triangle_height));
            bottom_cap.close_path();
            painter.fill(&bottom_cap, cursor).draw();

            self.paint_metrics(painter);
            return false;
        }
        if self.tool == Tool::Text {
            self.paint_metrics(painter);
            return false;
        }
        if text.active_origin().is_none() {
            return false;
        }
        true
    }

    /// Paint the layer picture, the reference glyph, the background layer and the proposals under the glyph.
    fn paint_underlay(&self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        // Underlay, drawn first so everything else sits on top of it. The
        // reference glyph is a quiet fill (it is a shape to match), the
        // background layer is a quiet outline (it is a trace to follow).
        if let Some(image) = &self.underlay.image {
            let height = f64::from(image.data.height);
            let to_glyph = self.image_preview.map_or(image.to_glyph, |transform| {
                transform * Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, height])
            });
            painter.draw_image(&image.data, affine * to_glyph);
            if self.session.image_selected
                && let Some((transform, width, height)) = self.image_frame(true)
            {
                let locked = self.session.image_locked();
                let transform = self.image_preview.unwrap_or(transform);
                let frame = affine * transform * Rect::new(0.0, 0.0, width, height).to_path(0.1);
                // A locked picture shows that it is selected, but offers no handles.
                let selection = if locked {
                    pal.text_muted
                } else {
                    pal.role("selection")
                };
                painter
                    .stroke(&frame, &Stroke::new(pal.stroke), selection)
                    .draw();
                for (corner, _) in Self::image_corners(transform, width, height)
                    .into_iter()
                    .filter(|_| !locked)
                {
                    let at = affine * corner;
                    painter
                        .fill(Rect::from_center_size(at, (7.0, 7.0)), pal.canvas)
                        .draw();
                    painter
                        .stroke(
                            Rect::from_center_size(at, (7.0, 7.0)),
                            &Stroke::new(pal.stroke),
                            selection,
                        )
                        .draw();
                }
            }
        }
        if let Some(reference) = &self.underlay.reference {
            painter
                .fill(&**reference, pal.text_muted.with_alpha(0.18))
                .transform(affine)
                .draw();
        }
        if let Some(background) = &self.underlay.background {
            painter
                .stroke(
                    &(affine * (**background).clone()),
                    &Stroke::new(pal.stroke),
                    pal.text_muted.with_alpha(0.5),
                )
                .draw();
        }
        for mark in &self.underlay.mark_cloud {
            painter
                .fill(&**mark, pal.role("component").with_alpha(0.10))
                .transform(affine)
                .draw();
        }
        if let Some(proposal) = &self.underlay.proposal {
            let path = affine * (**proposal).clone();
            painter
                .fill(&path, pal.role("warning").with_alpha(0.12))
                .draw();
            painter
                .stroke(
                    &path,
                    &Stroke::new(1.75),
                    pal.role("warning").with_alpha(0.95),
                )
                .draw();
        }
        if let Some(candidate) = &self.underlay.brush_candidate {
            let path = affine * (**candidate).clone();
            painter
                .fill(&path, pal.role("warning").with_alpha(0.35))
                .draw();
            painter
                .stroke(
                    &path,
                    &Stroke::new(pal.stroke_emphasis),
                    pal.role("warning"),
                )
                .draw();
        }
    }

    /// Paint the outlines of the other layers and masters whose eye is on.
    fn paint_layer_ghosts(&self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        // Other layers and masters whose eye is on, as outlines behind the edited layer.
        for ghost in self.ghosts.iter() {
            painter
                .stroke(
                    &(affine * (**ghost).clone()),
                    &Stroke::new(pal.stroke),
                    pal.layer_outline(),
                )
                .draw();
        }
    }

    /// Paint the design grid, as dots or lines according to View > Grid.
    fn paint_design_grid(&self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        // 8-unit intersection once the zoom passes 0.8x, and a finer
        // 2-unit dot past 8x. Dot sizes share the point-marker scale,
        // keeping the colored space inside markers proportional at every zoom.
        let zoom = self.session.viewport.zoom;
        let (mid, close) = grid_alphas(zoom);
        if mid > 0.0 {
            let inv = affine.inverse();
            let a = inv * Point::new(0.0, 0.0);
            let b = inv * Point::new(self.size.width, self.size.height);
            let (min_x, max_x) = (a.x.min(b.x), a.x.max(b.x));
            let (min_y, max_y) = (a.y.min(b.y), a.y.max(b.y));
            let mut level = |spacing: f64, skip_every: i64, size: f64, alpha: f64| {
                let mut marks = kurbo::BezPath::new();
                let (ix0, ix1) = (
                    grid_index((min_x / spacing).floor()),
                    grid_index((max_x / spacing).ceil()),
                );
                let (iy0, iy1) = (
                    grid_index((min_y / spacing).floor()),
                    grid_index((max_y / spacing).ceil()),
                );
                if self.view.grid_lines {
                    for ix in ix0..=ix1 {
                        if skip_every == 0 || ix % skip_every != 0 {
                            let x = (affine * Point::new(ix as f64 * spacing, 0.0)).x;
                            marks.move_to(Point::new(x, 0.0));
                            marks.line_to(Point::new(x, self.size.height));
                        }
                    }
                    for iy in iy0..=iy1 {
                        if skip_every == 0 || iy % skip_every != 0 {
                            let y = (affine * Point::new(0.0, iy as f64 * spacing)).y;
                            marks.move_to(Point::new(0.0, y));
                            marks.line_to(Point::new(self.size.width, y));
                        }
                    }
                }
                let color = pal
                    .role("designGridCoarse")
                    .with_alpha(crate::application::view::render::px32(alpha));
                if self.view.grid_lines {
                    painter.stroke(&marks, &Stroke::new(0.5), color).draw();
                    return;
                }
                // The dots are the same for every pan offset, so they are built once for
                // a zoom and canvas size and then only moved.
                let [scale_x, _, _, scale_y, origin_x, origin_y] = affine.as_coeffs();
                let (period_x, period_y) = (spacing * scale_x.abs(), spacing * scale_y.abs());
                let key: GridDotsKey = (
                    period_x.to_bits(),
                    period_y.to_bits(),
                    size.to_bits(),
                    self.size.width.to_bits(),
                    self.size.height.to_bits(),
                    skip_every,
                );
                let mut cache = self.grid_dots.borrow_mut();
                let slot = &mut cache[usize::from(skip_every > 0)];
                if slot.as_ref().is_none_or(|(built, _)| *built != key) {
                    let field = grid_dots_field(
                        period_x,
                        period_y,
                        size,
                        self.size.width,
                        self.size.height,
                        skip_every,
                    );
                    *slot = Some((key, field));
                }
                if let Some((_, field)) = slot {
                    let offset = (
                        grid_dots_offset(origin_x, period_x, skip_every),
                        grid_dots_offset(origin_y, period_y, skip_every),
                    );
                    painter
                        .fill(&*field, color)
                        .transform(Affine::translate(offset))
                        .draw();
                }
            };
            let (coarse, fine) = point_grid_dot_sizes(zoom);
            let spacing = runebender::outline::point_ops::DOT_GRID_SPACING;
            level(spacing, 0, coarse, mid);
            if close > 0.0 {
                level(spacing / 4.0, 4, fine, close);
            }
        }
    }

    /// Paint the metric lines and crosses of the glyph frame.
    fn paint_metric_frame(&self, painter: &mut Painter<'_>, pal: &Palette) {
        let m = &self.session.metrics;
        // Inside a component, the frame is the composite's: its sort, its advance.
        let frame_affine = self.sort_affine();
        // Metrics belong to this glyph's advance, not the whole workspace.
        // Include the full em even when its top lies above the ascender.
        let frame = pal.metrics_line();
        let rule = DesignStroke::Hairline.px();
        let box_top = m.upm.max(m.ascender);
        let x0 = (frame_affine * Point::new(0.0, 0.0)).x;
        let x1 = (frame_affine * Point::new(self.session.frame_advance(), 0.0)).x;
        let levels = text_sort_metric_ys(m);
        // A neural item is not set in a box: no advance, ascender, or descender.
        // One baseline at y = 0 runs across the whole canvas as the anchor
        // between positive and negative heights.
        let neural = self.session.neural;
        if neural {
            let sy = (frame_affine * Point::new(0.0, 0.0)).y;
            painter.fill_rect(horizontal_rule_rect(0.0, self.size.width, sy, rule), frame);
        }
        for &y in levels.iter().filter(|_| !neural) {
            let sy = (frame_affine * Point::new(0.0, y)).y;
            painter.fill_rect(horizontal_rule_rect(x0, x1, sy, rule), frame);
        }
        let top = (frame_affine * Point::new(0.0, box_top)).y;
        let bottom = (frame_affine * Point::new(0.0, m.descender)).y;
        for x in [x0, x1].into_iter().filter(|_| !neural) {
            painter.fill_rect(vertical_rule_rect(x, top, bottom, rule), frame);
        }
        let mark =
            (((box_top - m.descender) * self.session.viewport.zoom).abs() * 0.05).clamp(1.5, 24.0);
        if !neural {
            paint_metric_crosses(
                painter,
                [x0, x1],
                [top, bottom],
                levels
                    .iter()
                    .map(|&y| (frame_affine * Point::new(0.0, y)).y),
                mark,
                pal.outline,
            );
        }
    }

    /// Paint the rest of the composite around an open component, and the components of the glyph.
    fn paint_composite_parts(&self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        // Inside a component, the rest of the composite sits around it: its own contours
        // in the edit fill, its other components in the component tint.
        let sort_affine = self.sort_affine();
        if self.session.host.is_some() {
            for part in self.session.parts.iter() {
                let fill = if part.owner == PartOwner::Root {
                    pal.outline_fill()
                } else {
                    pal.component_fill()
                };
                let outline = sort_affine * part.outline.clone();
                painter.fill(&outline, fill).draw();
                // The outline without its points, as the open glyph has with them.
                painter
                    .stroke(&outline, &Stroke::new(pal.stroke), pal.role("pathStroke"))
                    .draw();
            }
        }
        if !self.session.components.elements().is_empty() {
            let components = affine * self.session.components.clone();
            painter.fill(&components, pal.component_fill()).draw();
            painter
                .stroke(
                    &components,
                    &Stroke::new(pal.stroke),
                    pal.role("pathStroke"),
                )
                .draw();
            if let Some(selected) = self.session.selected_component_path() {
                painter
                    .stroke(
                        &(affine * selected.clone()),
                        &Stroke::new(pal.stroke_emphasis),
                        pal.role("selection"),
                    )
                    .draw();
            }
        }
    }

    /// Paint the curvature comb under the editable outline.
    fn paint_curvature_comb(&self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        // Curvature comb: paint the coloured strip before the editable
        // outline, handles, and points. This is the GPUI layer order: the
        // comb remains vivid, while every editing affordance stays clear
        // and selectable above it.
        if self.view.comb {
            let strips = self.session.curvature_comb();
            let maxk = strips
                .iter()
                .flatten()
                .map(|sample| sample.kappa.abs())
                .fold(0.0, f64::max);
            for strip in strips {
                for pair in strip.windows(2) {
                    let mut quad = kurbo::BezPath::new();
                    quad.move_to(affine * pair[0].on);
                    quad.line_to(affine * pair[1].on);
                    quad.line_to(affine * pair[1].outer);
                    quad.line_to(affine * pair[0].outer);
                    quad.close_path();
                    let k = if maxk > 1e-12 {
                        (pair[0].kappa.abs() + pair[1].kappa.abs()) * 0.5 / maxk
                    } else {
                        0.0
                    };
                    painter
                        .stroke(&quad, &Stroke::new(pal.stroke_emphasis), pal.comb_keyline())
                        .draw();
                    painter.fill(&quad, pal.comb_fill(k)).draw();
                }
            }
        }
    }

    /// Paint the editable outline: its fill, the thickness field and its stroke.
    fn paint_outline(&mut self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        let outline = affine * self.session.outline();
        // An open component keeps its tint, so it still reads as a component.
        let fill = if self.session.host.is_some() {
            pal.component_fill()
        } else {
            // GPUI keeps the edit fill at 70% opacity so the glyph reads
            // as a shape without hiding the design grid and metrics.
            pal.outline_fill()
        };
        painter.fill(&outline, fill).draw();
        // Thickness: the local thickness field of the ink as an image,
        // colored on the warm ramp. Painted over the fill and under the
        // outline stroke, so the band replaces the ink and the edge stays
        // crisp. The field is cached by outline, so only an edit recomputes it.
        if self.view.thickness && self.interp.is_none() {
            let design_outline = self.session.outline();
            let key = thickness_key(&design_outline, pal);
            let image = match &self.thickness_image {
                Some((cached, image, placement)) if *cached == key => {
                    Some((image.clone(), *placement))
                }
                _ => {
                    let image = thickness_image(&design_outline, pal);
                    self.thickness_image = image
                        .clone()
                        .map(|(image, placement)| (key, image, placement));
                    image
                }
            };
            if let Some((image, pixel_to_design)) = image {
                painter.draw_image(&image, affine * pixel_to_design);
            }
        }
        painter
            .stroke(&outline, &Stroke::new(pal.stroke), pal.role("pathStroke"))
            .draw();
    }

    /// Paint the handles and point markers of the open glyph and, for View > Components, of the other parts.
    fn paint_points(&self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        // Labeling reads the ink as a whole; points and handles would only hide it.
        let point_chrome = self.tool != Tool::Label;
        let handle = Stroke::new(pal.stroke);
        if point_chrome && self.view.components {
            self.paint_part_points(painter, self.sort_affine());
        }
        if point_chrome {
            for line in self.session.handle_lines() {
                painter
                    .stroke(affine * line, &handle, pal.handle_line)
                    .draw();
            }
        }

        let widths = point_marker::widths(self.session.viewport.zoom);
        let marker_scale = widths.scale;
        if point_chrome {
            let start_markers = self.start_markers();
            for (id, sp, on_curve, smooth, _) in self.screen_points() {
                let selected = self.session.selection.contains(&id);
                let kind = PointKind::of(on_curve, smooth);
                let hue = kind.hue(pal);
                let ink = point_marker::ink(pal, hue, selected, pal.canvas);
                let square = kind.is_square();
                let r = (kind.radius() + if selected { POINT_SELECTED_GROW } else { 0.0 })
                    * marker_scale;
                let shape = start_markers
                    .iter()
                    .find(|(start_id, _, _)| *start_id == id)
                    .and_then(|(_, from, to)| direction_marker_shape(*from, *to, r, smooth))
                    .unwrap_or_else(|| point_marker_shape(sp, r, square));
                point_marker::paint_ground(
                    painter,
                    &shape,
                    ink.interior,
                    point_marker::halo(pal),
                    widths,
                );

                // A point is also a window onto the design grid. GPUI
                // redraws the dots or line chords which fall inside the
                // marker after its interior and before its ring. This makes
                // exact grid alignment readable without weakening the point.
                let (coarse_alpha, fine_alpha) = grid_alphas(self.session.viewport.zoom);
                let (coarse_dot, fine_dot) = point_grid_dot_sizes(self.session.viewport.zoom);
                let grid_color = if selected || pal.points_filled {
                    ink.ring
                } else {
                    hue
                };
                for (spacing, alpha, dot_size, line_width) in [
                    (8.0, coarse_alpha, coarse_dot, POINT_GRID_COARSE_LINE_WIDTH),
                    (2.0, fine_alpha, fine_dot, POINT_GRID_FINE_LINE_WIDTH),
                ] {
                    if alpha <= 0.0 {
                        continue;
                    }
                    let marks = point_grid_marks(
                        affine,
                        sp,
                        r,
                        square,
                        spacing,
                        dot_size,
                        self.view.grid_lines,
                    );
                    if marks.is_empty() {
                        continue;
                    }
                    let color =
                        grid_color.with_alpha(crate::application::view::render::px32(alpha));
                    if self.view.grid_lines {
                        painter
                            .stroke(&marks, &Stroke::new(line_width), color)
                            .draw();
                    } else {
                        painter.fill(&marks, color).draw();
                    }
                }
                point_marker::paint_ring(painter, &shape, ink.ring, widths);
            }
        }
    }

    /// Paint the handles of the ordinary pen that are not committed, and the point a pen click would add.
    fn paint_pen_previews(&self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        let widths = point_marker::widths(self.session.viewport.zoom);
        let marker_scale = widths.scale;
        // The ordinary pen's initial incoming and current outgoing
        // handles remain session-local until close or the next segment
        // commits them. Paint their nodes separately so smooth points
        // retain the same visible feedback as committed off-curves.
        for (_, handle) in self.session.pen_handle_previews() {
            let kind = PointKind::OffCurve;
            let ink = point_marker::ink(pal, kind.hue(pal), false, pal.canvas);
            let shape = point_marker_shape(affine * handle, kind.radius() * marker_scale, false);
            point_marker::paint(painter, &shape, ink, point_marker::halo(pal), widths);
        }

        // The point a pen click would add to the segment under the
        // cursor: a faint corner on a line, a faint smooth point on a curve.
        if let Some((position, curve)) = self.insert_preview.filter(|_| self.tool == Tool::Pen) {
            let kind = PointKind::of(true, curve);
            let ink = point_marker::ink(pal, kind.hue(pal), false, pal.canvas);
            let shape = point_marker_shape(
                affine * position,
                kind.radius() * marker_scale,
                kind.is_square(),
            );
            painter.fill(&shape, ink.interior.with_alpha(0.5)).draw();
            point_marker::paint_ring(painter, &shape, ink.ring.with_alpha(0.6), widths);
        }
    }

    /// Paint the anchors of the open glyph.
    fn paint_anchors(&self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        let widths = point_marker::widths(self.session.viewport.zoom);
        let marker_scale = widths.scale;
        // Anchors use the same point construction, with solid pink inside
        // the shared dark keyline. Selection retains the node palette.
        let anchor_color = pal.mark("pink").unwrap_or_else(|| pal.role("danger"));
        for (anchor, position) in self.session.anchor_points() {
            let p = affine * position;
            let selected = self.session.selected_anchors.contains(&anchor);
            let ink = point_marker::Ink {
                ring: point_marker::outline(pal),
                interior: if selected {
                    pal.role("pointSelected")
                } else {
                    anchor_color
                },
            };
            let r = (POINT_CURVE_RADIUS + if selected { POINT_SELECTED_GROW } else { 0.0 })
                * marker_scale
                * ANCHOR_DIAMOND_SCALE;
            let diamond = kurbo::BezPath::from_vec(vec![
                kurbo::PathEl::MoveTo(Point::new(p.x, p.y - r)),
                kurbo::PathEl::LineTo(Point::new(p.x + r, p.y)),
                kurbo::PathEl::LineTo(Point::new(p.x, p.y + r)),
                kurbo::PathEl::LineTo(Point::new(p.x - r, p.y)),
                kurbo::PathEl::ClosePath,
            ]);
            let halo = pal.point_halo.then(|| pal.role("halo"));
            point_marker::paint(painter, &diamond, ink, halo, widths);
        }
    }

    /// Paint the read-only interpolated instance in place of the editable outline.
    fn paint_instance(
        &self,
        painter: &mut Painter<'_>,
        pal: &Palette,
        affine: Affine,
        interp: &kurbo::BezPath,
    ) {
        // Read-only interpolated instance in warm amber, filled and
        // stroked, standing in for the editable outline.
        let path = affine * interp.clone();
        painter
            .fill(&path, pal.role("warning").with_alpha(0.14))
            .draw();
        painter
            .stroke(
                &path,
                &Stroke::new(1.75),
                pal.role("warning").with_alpha(0.95),
            )
            .draw();
    }

    /// Paint the continuity rings.
    fn paint_continuity(&self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        // Continuity rings match GPUI and preserve the underlying point shape.
        if self.view.continuity && self.interp.is_none() {
            use runebender::analysis::curve::GLevel;
            const CONTINUITY_RADIUS: f64 = 4.5 * 1.9;
            let color = pal.mark("green").unwrap_or(pal.text);
            let outline = pal.point_outline.unwrap_or(pal.text);
            for node in self.session.continuity() {
                if matches!(node.level, GLevel::Corner) {
                    continue;
                }
                let ring = Circle::new(affine * node.at, CONTINUITY_RADIUS);
                painter.stroke(ring, &Stroke::new(3.0), outline).draw();
                painter.stroke(ring, &Stroke::new(1.5), color).draw();
            }
        }
    }

    /// Paint the outline and its handles in colors by segment length.
    fn paint_colorize(&self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        // Colorize: tint the outline and its handles by segment length,
        // the web editor's mode for spotting odd measurements.
        if self.view.colorize && self.interp.is_none() {
            for stroke in self.session.colored_strokes() {
                let color = pal.popcount(stroke.popcount);
                let width = if stroke.wide { 2.0 } else { 1.0 };
                painter
                    .stroke(&(affine * stroke.path), &Stroke::new(width), color)
                    .draw();
            }
        }
    }

    /// Paint the measure overlay: segment and handle lengths, segment sizes and side bearings.
    fn paint_measurements(&self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        // Measure overlay: segment and handle lengths, and side bearings.
        if self.view.measures() && self.interp.is_none() {
            let zoom = self.session.viewport.zoom;
            for m in self.session.measurements() {
                use runebender::analysis::measure::MeasureKind;
                let wanted = match m.kind {
                    MeasureKind::Handle => self.view.handles,
                    MeasureKind::Segment => self.view.segments,
                    MeasureKind::Horizontal | MeasureKind::Vertical => self.view.spans,
                };
                if !wanted {
                    continue;
                }
                let a = affine * m.a;
                let b = affine * m.b;
                let color = match m.kind {
                    MeasureKind::Handle => pal.role("pointOffcurve"),
                    MeasureKind::Segment => pal.tool_feedback(),
                    _ => pal.role("selection"),
                };
                painter
                    .stroke(Line::new(a, b), &Stroke::new(pal.stroke), color)
                    .draw();
                let mid = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                let text = self.view.label(m.length);
                text_label::draw(painter, mid, &text, 11.0, color, Anchor::Middle);
            }
            if self.view.sizes {
                for bounds in self.session.segment_bounds() {
                    if bounds.width() < 1.0 && bounds.height() < 1.0 {
                        continue;
                    }
                    let a = affine * Point::new(bounds.x0, bounds.y0);
                    let b = affine * Point::new(bounds.x1, bounds.y1);
                    let screen = Rect::from_points(a, b);
                    painter
                        .stroke(screen, &Stroke::new(pal.stroke), pal.role("metricQuiet"))
                        .draw();
                    let label = format!("{:.0}×{:.0}", bounds.width(), bounds.height());
                    text_label::draw(
                        painter,
                        screen.center(),
                        &label,
                        11.0,
                        pal.text,
                        Anchor::Middle,
                    );
                }
            }
            if let Some(sb) = self.session.side_bearings().filter(|_| self.view.bearings) {
                let quiet = pal.role("metricQuiet");
                let y = (affine
                    * Point::new(0.0, sb.y_left.min(sb.y_right) - 40.0 / zoom.max(0.001)))
                .y;
                let l = affine * Point::new(0.0, 0.0);
                let ink_l = affine * Point::new(sb.min_x, 0.0);
                let ink_r = affine * Point::new(sb.max_x, 0.0);
                let adv = affine * Point::new(sb.advance, 0.0);
                painter
                    .stroke(
                        Line::new((l.x, y), (ink_l.x, y)),
                        &Stroke::new(pal.stroke),
                        quiet,
                    )
                    .draw();
                painter
                    .stroke(
                        Line::new((ink_r.x, y), (adv.x, y)),
                        &Stroke::new(pal.stroke),
                        quiet,
                    )
                    .draw();
                let (lsb, rsb) = (self.view.label(sb.lsb), self.view.label(sb.rsb));
                text_label::draw(
                    painter,
                    Point::new((l.x + ink_l.x) / 2.0, y - 8.0),
                    &lsb,
                    11.0,
                    quiet,
                    Anchor::Middle,
                );
                text_label::draw(
                    painter,
                    Point::new((ink_r.x + adv.x) / 2.0, y - 8.0),
                    &rsb,
                    11.0,
                    quiet,
                    Anchor::Middle,
                );
            }
        }
    }

    /// Paint what the active tool and the current drag show over the glyph, in order.
    fn paint_tool_overlays(&mut self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        if self.tool == Tool::Measure && self.interp.is_none() {
            self.paint_ruler(painter, affine);
        }
        self.paint_metaballs(painter, pal, affine);
        self.paint_marquee(painter, pal);
        if self.tool == Tool::Label {
            self.paint_labels(painter, affine);
        }
        self.paint_freehand_path(painter, pal);
        self.paint_shape_preview(painter, pal, affine);
        self.paint_ai_shape(painter, affine, pal);
        self.paint_sketch(painter, pal);
    }

    /// Paint the metaballs and their reach, for the Select and Metaball tools.
    fn paint_metaballs(&self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        if matches!(self.tool, Tool::Select | Tool::Metaball)
            && let Ok(source) = self.session.metaball_data()
        {
            let widths = point_marker::widths(self.session.viewport.zoom);
            for group in source.groups {
                for ball in group.balls {
                    let center = affine * Point::new(ball.x, ball.y);
                    let selected = self
                        .session
                        .metaballs
                        .selected
                        .contains(&(group.id, ball.id));
                    let ink = if selected {
                        point_marker::ink(pal, pal.canvas, true, pal.canvas)
                    } else {
                        point_marker::Ink {
                            ring: pal.editor_control_ink(),
                            interior: pal.canvas,
                        }
                    };
                    let marker_ring = ink.ring;
                    let support = Circle::new(
                        center,
                        ball.radius * ball.reach * self.session.viewport.zoom,
                    );
                    if selected {
                        let keyline = pal.mark("orange").unwrap_or(marker_ring);
                        painter
                            .stroke(
                                support,
                                &Stroke::new(DesignStroke::Hairline.px() * 3.0),
                                keyline,
                            )
                            .draw();
                        painter
                            .stroke(
                                support,
                                &Stroke::new(DesignStroke::Hairline.px()),
                                pal.role("pointSelected"),
                            )
                            .draw();
                    } else {
                        painter
                            .stroke(
                                support,
                                &Stroke::new(DesignStroke::Hairline.px()),
                                marker_ring.with_alpha(0.35),
                            )
                            .draw();
                    }
                    let radius = (POINT_CURVE_RADIUS
                        + if selected { POINT_SELECTED_GROW } else { 0.0 })
                        * widths.scale;
                    let marker = Circle::new(center, radius);
                    point_marker::paint(painter, &marker, ink, point_marker::halo(pal), widths);
                }
            }
        }
    }

    /// Paint the marquee rectangle of a selection drag.
    fn paint_marquee(&self, painter: &mut Painter<'_>, pal: &Palette) {
        // Marquee rectangle.
        if let Drag::Marquee { start, current, .. } = &self.drag {
            let rect = Rect::from_points(*start, *current);
            painter
                .fill(rect, pal.role("selection").with_alpha(0.15))
                .draw();
            painter
                .stroke(
                    rect,
                    &Stroke::new(pal.stroke),
                    pal.role("selection").with_alpha(0.8),
                )
                .draw();
        }
    }

    /// Paint the freehand path of a lasso or label drag.
    fn paint_freehand_path(&self, painter: &mut Painter<'_>, pal: &Palette) {
        if let Drag::Lasso { points, .. } | Drag::Label(LabelDrag::Freehand { points }) = &self.drag
            && let Some(first) = points.first()
        {
            let mut path = kurbo::BezPath::new();
            path.move_to(*first);
            for point in &points[1..] {
                path.line_to(*point);
            }
            if points.len() >= 3 {
                path.close_path();
                painter
                    .fill(&path, pal.role("selection").with_alpha(0.15))
                    .draw();
            }
            painter
                .stroke(
                    &path,
                    &Stroke::new(pal.stroke),
                    pal.role("selection").with_alpha(0.8),
                )
                .draw();
        }
    }

    /// Paint the preview of a rectangle, ellipse, knife or AI shapes drag.
    fn paint_shape_preview(&self, painter: &mut Painter<'_>, pal: &Palette, affine: Affine) {
        // Shape preview.
        if let Drag::Shape { start, current } = &self.drag {
            let p0 = affine * *start;
            let p1 = affine * *current;
            let accent = pal.tool_feedback();
            match self.tool {
                Tool::Knife => {
                    let danger = pal.role("danger");
                    painter
                        .stroke(Line::new(p0, p1), &Stroke::new(pal.stroke), danger)
                        .draw();
                    for hit in self.session.knife_hits(*start, *current) {
                        let sp = affine * hit;
                        painter.fill(Circle::new(sp, 3.5), danger).draw();
                    }
                }
                Tool::Ellipse => {
                    let c = ((p0.x + p1.x) / 2.0, (p0.y + p1.y) / 2.0);
                    let rr = ((p1.x - p0.x).abs() / 2.0, (p1.y - p0.y).abs() / 2.0);
                    let e = kurbo::Ellipse::new(c, rr, 0.0);
                    painter.stroke(e, &Stroke::new(pal.stroke), accent).draw();
                }
                _ => {
                    painter
                        .stroke(Rect::from_points(p0, p1), &Stroke::new(pal.stroke), accent)
                        .draw();
                }
            }
        }
    }

    /// Paint the session ink of the brush tool.
    fn paint_sketch(&mut self, painter: &mut Painter<'_>, pal: &Palette) {
        if self.tool == Tool::Sketch {
            let sketch = self
                .sketch
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if sketch.matches(&self.session.glyph_name, self.sketch_source) && sketch.has_ink() {
                let alpha: f32 = if self.underlay.brush_candidate.is_some() {
                    0.18
                } else {
                    0.55
                };
                let key = (sketch.revision(), alpha.to_bits());
                let image = match &self.sketch_image {
                    Some((revision, alpha_bits, image)) if (*revision, *alpha_bits) == key => {
                        image.clone()
                    }
                    _ => {
                        let image = ImageData {
                            data: Blob::new(Arc::new(
                                sketch.display_rgba(pal.tool_feedback().with_alpha(alpha)),
                            )),
                            format: ImageFormat::Rgba8,
                            alpha_type: ImageAlphaType::Alpha,
                            width: 512,
                            height: 512,
                        };
                        self.sketch_image = Some((key.0, key.1, image.clone()));
                        image
                    }
                };
                let pixel_to_glyph = Affine::new([2.0, 0.0, 0.0, -2.0, sketch.left(), 800.0]);
                painter.draw_image(&image, self.glyph_affine() * pixel_to_glyph);
            }
        }
    }
}
