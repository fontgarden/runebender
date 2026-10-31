// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Session-only, font-coordinate proof annotations; never edits font drawings.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, FromDynWidget, LayoutCtx, MeasureCtx, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

#[derive(Clone, Debug)]
pub(crate) struct Stroke {
    pub(crate) points: Vec<Point>,
    pub(crate) width: f64,
}
impl Stroke {
    pub(crate) fn clipped_path(
        &self,
        transform: kurbo::Affine,
        rect: kurbo::Rect,
    ) -> kurbo::BezPath {
        let mut path = kurbo::BezPath::new();
        for segment in self.points.windows(2) {
            let a = transform * segment[0];
            let b = transform * segment[1];
            let d = b - a;
            let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
            let mut visible = true;
            for (p, q) in [
                (-d.x, a.x - rect.x0),
                (d.x, rect.x1 - a.x),
                (-d.y, a.y - rect.y0),
                (d.y, rect.y1 - a.y),
            ] {
                if p.abs() < 1e-12 {
                    if q < 0.0 {
                        visible = false;
                        break;
                    }
                } else {
                    let t = q / p;
                    if p < 0.0 {
                        lo = lo.max(t);
                    } else {
                        hi = hi.min(t);
                    }
                    if lo > hi {
                        visible = false;
                        break;
                    }
                }
            }
            if visible {
                path.move_to(a + d * lo);
                path.line_to(a + d * hi);
            }
        }
        if self.points.len() == 1 {
            let p = transform * self.points[0];
            if rect.contains(p) {
                path.move_to(p);
                path.line_to(p + (0.01, 0.0));
            }
        }
        path
    }
    pub(crate) fn path(&self) -> kurbo::BezPath {
        let mut p = kurbo::BezPath::new();
        if let Some(first) = self.points.first() {
            p.move_to(*first);
            for point in self.points.iter().skip(1) {
                p.line_to(*point);
            }
            if self.points.len() == 1 {
                p.line_to((*first) + (0.01, 0.0));
            }
        }
        p
    }
}
#[derive(Default, Debug)]
pub(crate) struct Ink {
    pub(crate) strokes: std::collections::BTreeMap<String, Vec<Stroke>>,
    pub(crate) draft: Option<(String, Stroke)>,
    history: Vec<Vec<(String, Vec<Stroke>)>>,
    revision: u64,
}
impl Ink {
    pub(crate) fn finish(&mut self) {
        if let Some((key, stroke)) = self.draft.take() {
            let old = self.strokes.get(&key).cloned().unwrap_or_default();
            if old.len() < 128 && (self.strokes.contains_key(&key) || self.strokes.len() < 256) {
                self.remember(vec![(key.clone(), old)]);
                self.strokes.entry(key).or_default().push(stroke);
                self.revision = self.revision.wrapping_add(1);
            }
        }
    }
    pub(crate) fn can_undo(&self, prefix: &str) -> bool {
        self.history
            .iter()
            .any(|changes| changes.iter().any(|(key, _)| key.starts_with(prefix)))
    }
    fn remember(&mut self, changes: Vec<(String, Vec<Stroke>)>) {
        if !changes.is_empty() {
            self.history.push(changes);
            if self.history.len() > 16 {
                self.history.remove(0);
            }
        }
    }
    pub(crate) fn undo(&mut self, prefix: &str) {
        if let Some(i) = self
            .history
            .iter()
            .rposition(|changes| changes.iter().any(|(k, _)| k.starts_with(prefix)))
        {
            for (k, old) in self.history.remove(i) {
                self.strokes.insert(k, old);
            }
        }
        self.revision = self.revision.wrapping_add(1);
        self.draft = None;
    }
    pub(crate) fn clear(&mut self, prefix: &str) {
        let keys: Vec<_> = self
            .strokes
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        let changes = keys
            .into_iter()
            .map(|k| {
                let old = self.strokes.remove(&k).unwrap();
                (k, old)
            })
            .collect();
        self.remember(changes);
        self.revision = self.revision.wrapping_add(1);
        self.draft = None;
    }
}
#[derive(Clone, PartialEq)]
pub(crate) struct Mapping {
    pub(crate) keys: Vec<String>,
    pub(crate) bounds: kurbo::Rect,
    pub(crate) paired: bool,
    pub(crate) centered: bool,
}
impl Mapping {
    pub(crate) fn slots(&self, size: Size) -> Vec<(String, kurbo::Affine)> {
        let (l, r) = crate::application::view::canvas::proof::comparison_transforms(
            self.bounds,
            size,
            self.paired,
        );
        self.keys
            .iter()
            .enumerate()
            .map(|(i, k)| {
                (
                    k.clone(),
                    if self.centered {
                        l.then_translate(
                            (
                                (size.width + crate::application::design::areas::proof::COLUMN_GAP)
                                    / 4.0,
                                0.0,
                            )
                                .into(),
                        )
                    } else if i == 0 {
                        l
                    } else {
                        r
                    },
                )
            })
            .collect()
    }
}
pub(crate) fn prefix(app: &crate::application::workspace::Workspace) -> String {
    format!(
        "{}:{}:{}:",
        app.document_id,
        app.font
            .project
            .source_id(app.font.active())
            .map(|s| s.0)
            .unwrap_or(0),
        app.session.glyph_name
    )
}
pub(crate) fn key(
    app: &crate::application::workspace::Workspace,
    drawing: &crate::application::editor::tools::review::ReviewDrawing,
) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "{}{:x}",
        prefix(app),
        Sha256::digest(format!("{}:{}", drawing.advance, drawing.outline.to_svg()).as_bytes())
    )
}
pub(crate) fn card_key(
    app: &crate::application::workspace::Workspace,
    drawing: &crate::application::editor::tools::review::ReviewDrawing,
    caption: &str,
) -> String {
    format!("{}:{caption}", key(app, drawing))
}

pub(crate) struct ProofPencilWidget<W: Widget + FromDynWidget + ?Sized> {
    child: WidgetPod<W>,
    mapping: Mapping,
    ink: std::sync::Arc<std::sync::Mutex<Ink>>,
    enabled: bool,
    color: xilem::Color,
    size: Size,
}

impl<W: Widget + FromDynWidget + ?Sized> ProofPencilWidget<W> {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<W: Widget + FromDynWidget + ?Sized> Widget for ProofPencilWidget<W> {
    type Action = ();

    fn accepts_pointer_interaction(&self) -> bool {
        self.enabled
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        cross: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.child, axis, cross)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
        ctx.set_clip_path(size.to_rect());
    }

    fn post_paint(
        &mut self,
        _: &mut PaintCtx<'_>,
        _: &PropertiesRef<'_>,
        painter: &mut masonry::imaging::Painter<'_>,
    ) {
        let ink = self.ink.lock().unwrap();
        for (key, transform) in self.mapping.slots(self.size) {
            for stroke in ink.strokes.get(&key).into_iter().flatten().chain(
                ink.draft
                    .as_ref()
                    .filter(|(k, _)| *k == key)
                    .map(|(_, s)| s),
            ) {
                let path = stroke.path();
                painter
                    .stroke(
                        &(transform * path),
                        &kurbo::Stroke::new(stroke.width * transform.as_coeffs()[0].abs()),
                        self.color,
                    )
                    .draw();
            }
        }
    }
    fn paint(
        &mut self,
        _: &mut PaintCtx<'_>,
        _: &PropertiesRef<'_>,
        _: &mut masonry::imaging::Painter<'_>,
    ) {
    }
    fn on_pointer_event(
        &mut self,
        ctx: &mut masonry::core::EventCtx<'_>,
        _: &mut masonry::core::PropertiesMut<'_>,
        event: &masonry::core::PointerEvent,
    ) {
        use masonry::core::{PointerButton, PointerEvent};
        if !self.enabled {
            return;
        }
        let mut ink = self.ink.lock().unwrap();
        match event {
            PointerEvent::Down(e) if e.button == Some(PointerButton::Primary) => {
                let at = ctx.local_position(e.state.position);
                let slots = self.mapping.slots(self.size);
                let index = usize::from(slots.len() > 1 && at.x > self.size.width / 2.0);
                if let Some((key, transform)) = slots.get(index) {
                    if key.is_empty() {
                        return;
                    }
                    ink.draft = Some((
                        key.clone(),
                        Stroke {
                            points: vec![transform.inverse() * at],
                            width: 2.5 / transform.as_coeffs()[0].abs(),
                        },
                    ));
                    ctx.capture_pointer();
                    ctx.set_handled();
                    ctx.request_render();
                }
            }
            PointerEvent::Move(e) => {
                if let Some((key, stroke)) = &mut ink.draft
                    && let Some((_, transform)) = self
                        .mapping
                        .slots(self.size)
                        .into_iter()
                        .find(|(k, _)| k == key)
                {
                    let point = transform.inverse() * ctx.local_position(e.current.position);
                    if stroke.points.len() < 2048
                        && stroke
                            .points
                            .last()
                            .is_none_or(|p| p.distance(point) > stroke.width / 4.0)
                    {
                        stroke.points.push(point);
                    }
                    ctx.set_handled();
                    ctx.request_render();
                }
            }
            PointerEvent::Up(_) => {
                ink.finish();
                ctx.request_render();
            }
            PointerEvent::Cancel(_) => {
                ink.draft = None;
                ctx.request_render();
            }
            _ => {}
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

pub(crate) struct ProofPencil<V> {
    content: V,
    mapping: Mapping,
    ink: std::sync::Arc<std::sync::Mutex<Ink>>,
    enabled: bool,
    color: xilem::Color,
    revision: u64,
}

pub(crate) fn proof_pencil<V>(
    content: V,
    mapping: Mapping,
    ink: std::sync::Arc<std::sync::Mutex<Ink>>,
    enabled: bool,
    color: xilem::Color,
) -> ProofPencil<V> {
    let revision = ink.lock().unwrap().revision;
    ProofPencil {
        content,
        mapping,
        ink,
        enabled,
        color,
        revision,
    }
}

impl<V> ViewMarker for ProofPencil<V> {}

impl<State: 'static, V: WidgetView<State>> View<State, (), ViewCtx> for ProofPencil<V> {
    type Element = Pod<ProofPencilWidget<V::Widget>>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, state) = self.content.build(ctx, app);
        (
            ctx.create_pod(ProofPencilWidget {
                child: child.new_widget.to_pod(),
                mapping: self.mapping.clone(),
                ink: self.ink.clone(),
                enabled: self.enabled,
                color: self.color,
                size: Size::ZERO,
            }),
            state,
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut State,
    ) {
        if self.revision != prev.revision || self.color != prev.color {
            element.widget.color = self.color;
            element.ctx.request_render();
        }
        if self.mapping != prev.mapping || self.enabled != prev.enabled {
            element.widget.ink.lock().unwrap().draft = None;
            element.widget.mapping = self.mapping.clone();
            element.widget.enabled = self.enabled;
            element.ctx.request_layout();
        }
        self.content.rebuild(
            &prev.content,
            state,
            ctx,
            ProofPencilWidget::child_mut(&mut element),
            app,
        );
    }

    fn teardown(
        &self,
        state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        self.content
            .teardown(state, ctx, ProofPencilWidget::child_mut(&mut element));
    }

    fn message(
        &self,
        state: &mut Self::ViewState,
        ctx: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut State,
    ) -> MessageResult<()> {
        self.content
            .message(state, ctx, ProofPencilWidget::child_mut(&mut element), app)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{
        design::stock, view::render::panel_resize_tests_support::context, workspace::Workspace,
    };
    use masonry::core::PointerButton;
    use masonry_testing::TestHarness;
    use xilem::core::View;

    #[test]
    fn proof_pencil_pointer_undo_clear_and_resize() {
        let mut app = Workspace::untitled().unwrap();
        let revision = app.font.project.document_revision();
        let ink = std::sync::Arc::new(std::sync::Mutex::new(Ink::default()));
        let mapping = Mapping {
            keys: vec!["test:drawing".into()],
            bounds: kurbo::Rect::new(0.0, 0.0, 1024.0, 768.0),
            paired: false,
            centered: false,
        };
        fn make_view(
            mapping: Mapping,
            ink: std::sync::Arc<std::sync::Mutex<Ink>>,
        ) -> impl WidgetView<Workspace, Widget: Sized> {
            proof_pencil(
                xilem::view::canvas(|_: &mut Workspace, _, _, _: Size| {}),
                mapping,
                ink,
                true,
                crate::application::design::theme::Palette::load("gray").proof_pencil_ink(),
            )
        }
        let mut view = make_view(mapping.clone(), ink.clone());
        let mut ctx = context();
        let (pod, mut view_state) = view.build(&mut ctx, &mut app);
        let mut harness = TestHarness::create_with_size(
            stock::widget_properties(&app.palette),
            pod.new_widget,
            (600, 400),
        );
        harness.mouse_move((100.0, 100.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_move((200.0, 180.0));
        harness.mouse_button_release(Some(PointerButton::Primary));
        assert!(ink.lock().unwrap().draft.is_none());
        let state = ink.lock().unwrap();
        let stroke = &state.strokes["test:drawing"][0];
        assert!(stroke.points.len() >= 2);
        let original = stroke.points.clone();
        let transform = mapping.slots(Size::new(600.0, 400.0))[0].1;
        assert!((transform * stroke.points[0]).distance(Point::new(100.0, 100.0)) < 0.01);
        drop(state);
        let image = harness.render();
        assert!(
            image.pixels().any(|p| p[0] > 180 && p[1] < 80 && p[2] < 80),
            "red ink must reach native raster"
        );
        if let Some(path) = std::env::var_os("RUNEBENDER_PENCIL_RENDER") {
            image.save(path).unwrap();
        }
        let resized = mapping.slots(Size::new(900.0, 600.0))[0].1;
        assert!((resized.inverse() * (resized * original[0])).distance(original[0]) < 0.01);
        for (action, red_expected) in [("clear", false), ("undo", true), ("undo", false)] {
            {
                let mut state = ink.lock().unwrap();
                if action == "clear" {
                    state.clear("test:");
                } else {
                    state.undo("test:");
                }
            }
            let next = make_view(mapping.clone(), ink.clone());
            harness.edit_root_widget(|root| {
                next.rebuild(&view, &mut view_state, &mut ctx, root, &mut app);
            });
            view = next;
            let image = harness.render();
            assert_eq!(
                image.pixels().any(|p| p[0] > 180 && p[1] < 80 && p[2] < 80),
                red_expected,
                "{action} must immediately repaint ink"
            );
        }
        assert_eq!(app.font.project.document_revision(), revision);
    }
}
