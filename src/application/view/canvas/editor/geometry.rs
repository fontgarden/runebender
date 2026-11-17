// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Coordinate transforms, hit tests and view fitting of the editor widget.
//! The frame and corner handles of the layer picture are also here.

use super::*;

impl EditorWidget {
    pub(super) fn screen_points(&self) -> Vec<(PointId, Point, bool, bool, bool)> {
        let affine = self.glyph_affine();
        self.session
            .points()
            .into_iter()
            .map(|p| (p.id, affine * p.point, p.on_curve, p.smooth, p.start))
            .collect()
    }

    /// Closed contours expose their first on-curve point and outgoing direction.
    /// Open paths and contours without an on-curve point have no start marker.
    pub(super) fn start_markers(&self) -> Vec<(PointId, Point, Point)> {
        let affine = self.glyph_affine();
        self.session
            .start_markers()
            .into_iter()
            .map(|(id, from, to)| (id, affine * from, affine * to))
            .collect()
    }

    pub(super) fn hit_point(&self, at: Point) -> Option<PointId> {
        self.screen_points()
            .into_iter()
            .filter(|(_, sp, _, _, _)| sp.distance(at) <= HIT_RADIUS_PX)
            .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
            .map(|(id, _, _, _, _)| id)
    }

    /// The active sort's position in the composed line, or the origin when
    /// this tab has no text composition.
    pub(super) fn active_sort_origin(&self) -> Point {
        self.text
            .as_ref()
            .and_then(crate::application::editor::tools::text::TextState::active_origin)
            .unwrap_or(Point::ORIGIN)
    }

    /// Design-to-screen transform for the active sort: the root composite when a component
    /// is open in place.
    pub(super) fn sort_affine(&self) -> Affine {
        self.session.viewport.affine() * Affine::translate(self.active_sort_origin().to_vec2())
    }

    /// Design-to-screen transform for the glyph currently being edited.
    pub(super) fn glyph_affine(&self) -> Affine {
        self.sort_affine() * self.session.host_transform()
    }

    /// Convert a screen point into the active sort's glyph-local design space.
    pub(super) fn screen_to_glyph_design(&self, at: Point) -> Point {
        self.glyph_affine().inverse() * at
    }

    /// Convert a screen point into the active sort's design space.
    fn screen_to_sort_design(&self, at: Point) -> Point {
        self.sort_affine().inverse() * at
    }

    /// What a double click at `at` does to components, if anything: open the component under
    /// it, move to another part of the composite, or leave the open component.
    pub(super) fn component_double_click(&self, at: Point) -> Option<EditorEvent> {
        if let Some(id) = self.session.component_at(self.screen_to_glyph_design(at)) {
            return Some(EditorEvent::EnterComponent(id));
        }
        self.session.host.as_ref()?;
        if self
            .session
            .outline_contains(self.screen_to_glyph_design(at))
        {
            return None;
        }
        if let Some(owner) = self.session.part_at(self.screen_to_sort_design(at)) {
            return Some(EditorEvent::EnterPart { owner, point: None });
        }
        // Empty space inside the sort climbs out; another sort is the text line's business.
        let design = self.screen_to_sort_design(at);
        let m = &self.session.metrics;
        let inside = (0.0..=self.session.frame_advance()).contains(&design.x)
            && (m.descender..=m.upm.max(m.ascender)).contains(&design.y);
        (inside || self.text.is_none()).then_some(EditorEvent::ExitComponent)
    }

    /// The point of another part of the composite under `at`, when View > Components
    /// shows their points.
    pub(super) fn hit_part_point(&self, at: Point) -> Option<(PartOwner, PointId)> {
        if !self.view.components {
            return None;
        }
        let affine = self.sort_affine();
        self.session
            .parts
            .iter()
            .filter(|part| part.owner != PartOwner::Fixed)
            .flat_map(|part| {
                part.points
                    .iter()
                    .map(move |point| (part.owner, point.id, affine * point.point))
            })
            .filter(|(_, _, sp)| sp.distance(at) <= HIT_RADIUS_PX)
            .min_by(|a, b| a.2.distance(at).total_cmp(&b.2.distance(at)))
            .map(|(owner, id, _)| (owner, id))
    }

    pub(super) fn fit(&mut self) {
        if self.tool == Tool::Interpolation && self.fit_interpolation() {
            return;
        }
        // A neural canvas has no advance or metrics box to frame, and its
        // ink sits wherever the calligraphy was traced. Frame the ink.
        if self.session.neural && self.fit_ink() {
            return;
        }
        let m = self.session.metrics;
        self.session.viewport.fit_to_canvas(
            self.size.width,
            self.size.height,
            self.session.advance(),
            m.ascender,
            m.descender,
            0.62,
        );
        self.session.fitted = true;
    }

    /// Center the outline's bounds in the canvas. Returns false when the
    /// layer has no outline, so the caller can fall back to the metrics box.
    fn fit_ink(&mut self) -> bool {
        let Some(ink) = self
            .session
            .segment_bounds()
            .into_iter()
            .reduce(|a, b| a.union(b))
        else {
            return false;
        };
        let (width, height) = (ink.width().max(1.0), ink.height().max(1.0));
        let zoom = ((self.size.width * 0.7) / width).min((self.size.height * 0.7) / height);
        let viewport = &mut self.session.viewport;
        viewport.zoom = zoom.max(0.001);
        let center = ink.center();
        viewport.offset = kurbo::Vec2::new(
            self.size.width / 2.0 - center.x * viewport.zoom,
            self.size.height / 2.0 + center.y * viewport.zoom,
        );
        self.session.fitted = true;
        true
    }

    /// Frame the whole text line rather than one glyph.
    ///
    /// The editor's normal fit is one advance wide, which shows about two
    /// letters of a word. Typing is for judging spacing, so the view has
    /// to hold the line.
    pub(super) fn fit_text(&mut self) {
        let Some(text) = &self.text else {
            return;
        };
        let m = self.session.metrics;
        let width: f64 = text
            .placed()
            .iter()
            .map(|sort| sort.origin.x + sort.advance)
            .fold(self.session.advance(), f64::max);
        // Width first, and only then height. `fit_to_canvas` sizes to the
        // em, which is right for one glyph and wrong for a word: a line
        // of fifteen letters would fit vertically and run off both sides.
        let width = width.max(m.upm);
        // Text lines use the full sort box, not merely ascender to
        // descender. Virtua's upm is above its ascender; fitting the shorter
        // range made the top metric cross and caret cap collide with the
        // title bar while the glyphs looked too large.
        let sort_top = m.upm.max(m.ascender);
        let design_height = (sort_top - m.descender).max(1.0);
        // Leave the same breathing room as the mature editors: roughly ten
        // percent at each horizontal edge, and reserve the floating metrics
        // card instead of centering the run behind it.
        let available_height =
            (self.size.height - PANEL_HEIGHT - METRICS_CARD_BOTTOM).max(design_height * 0.001);
        let zoom = ((self.size.width * 0.8) / width).min((available_height * 0.8) / design_height);
        self.session.viewport.zoom = zoom.max(0.001);
        let center_y = (sort_top + m.descender) / 2.0;
        self.session.viewport.offset = kurbo::Vec2::new(
            (self.size.width - width * self.session.viewport.zoom) / 2.0,
            available_height / 2.0 + center_y * self.session.viewport.zoom,
        );
        self.session.fitted = true;
    }

    /// The layer's picture as shown: its transform and its size in pixels. None while it is
    /// hidden, missing or (unless `locked_too`) locked.
    pub(super) fn image_frame(&self, locked_too: bool) -> Option<(Affine, f64, f64)> {
        if !locked_too && self.session.image_locked() {
            return None;
        }
        let shown = self.underlay.image.as_ref()?;
        let transform = self.session.layer_image()?.transform();
        Some((
            transform,
            f64::from(shown.data.width),
            f64::from(shown.data.height),
        ))
    }

    /// The picture's corners in design space, each with the corner opposite it.
    pub(super) fn image_corners(transform: Affine, width: f64, height: f64) -> [(Point, Point); 4] {
        let corner = |x: f64, y: f64| transform * Point::new(x, y);
        let (a, b, c, d) = (
            corner(0.0, 0.0),
            corner(width, 0.0),
            corner(width, height),
            corner(0.0, height),
        );
        [(a, c), (b, d), (c, a), (d, b)]
    }

    /// What a Select press at `design` does to the picture: resize it from a corner handle
    /// when it is selected, or move it when the press lands on it.
    pub(super) fn image_press(&self, design: Point) -> Option<Drag> {
        let (transform, width, height) = self.image_frame(false)?;
        let reach = HIT_RADIUS_PX / self.session.viewport.zoom;
        if self.session.image_selected {
            for (corner, anchor) in Self::image_corners(transform, width, height) {
                if corner.distance(design) <= reach {
                    return Some(Drag::Image {
                        start: design,
                        origin: transform,
                        anchor: Some(anchor),
                        corner,
                    });
                }
            }
        }
        let bounds = (transform * Rect::new(0.0, 0.0, width, height).to_path(0.1)).bounding_box();
        bounds.contains(design).then_some(Drag::Image {
            start: design,
            origin: transform,
            anchor: None,
            corner: design,
        })
    }

    /// The transform an image drag gives the picture with the pointer at `design`.
    pub(super) fn image_drag_transform(
        start: Point,
        origin: Affine,
        anchor: Option<Point>,
        corner: Point,
        design: Point,
    ) -> Affine {
        let Some(anchor) = anchor else {
            return Affine::translate(design - start) * origin;
        };
        // Proportional: project the pointer onto the diagonal through the anchor.
        let diagonal = corner - anchor;
        let length = diagonal.hypot2();
        if length <= f64::EPSILON {
            return origin;
        }
        let scale = ((design - anchor).dot(diagonal) / length).max(0.02);
        Affine::translate(anchor.to_vec2())
            * Affine::scale(scale)
            * Affine::translate(-anchor.to_vec2())
            * origin
    }
}
