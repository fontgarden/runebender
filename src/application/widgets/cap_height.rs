// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Trim a single-line heading to its cap height using the child's measured baseline.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, FromDynWidget, LayoutCtx, MeasureCtx, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

pub(crate) struct CapHeightWidget<W: Widget + FromDynWidget + ?Sized> {
    child: WidgetPod<W>,
    height: f64,
    natural_height: f64,
}

impl<W: Widget + FromDynWidget + ?Sized> CapHeightWidget<W> {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<W: Widget + FromDynWidget + ?Sized> Widget for CapHeightWidget<W> {
    type Action = ();

    fn accepts_pointer_interaction(&self) -> bool {
        false
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
        let measured = ctx.redirect_measurement(&mut self.child, axis, cross);
        if axis == Axis::Vertical {
            self.natural_height = measured.get();
            Length::px(self.height)
        } else {
            measured
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, Size::new(size.width, self.natural_height));
        let (baseline, _) = ctx.child_layout_baselines(&self.child);
        ctx.place_child(&mut self.child, Point::new(0.0, self.height - baseline));
        ctx.set_baselines(self.height, self.height);
    }

    fn paint(
        &mut self,
        _: &mut PaintCtx<'_>,
        _: &PropertiesRef<'_>,
        _: &mut masonry::imaging::Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

pub(crate) struct CapHeight<V> {
    content: V,
    height: f64,
}

pub(crate) fn cap_height<V>(content: V, height: f64) -> CapHeight<V> {
    CapHeight { content, height }
}

impl<V> ViewMarker for CapHeight<V> {}

impl<State: 'static, V: WidgetView<State>> View<State, (), ViewCtx> for CapHeight<V> {
    type Element = Pod<CapHeightWidget<V::Widget>>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, state) = self.content.build(ctx, app);
        (
            ctx.create_pod(CapHeightWidget {
                child: child.new_widget.to_pod(),
                height: self.height,
                natural_height: self.height,
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
        if self.height != prev.height {
            element.widget.height = self.height;
            element.ctx.request_layout();
        }
        self.content.rebuild(
            &prev.content,
            state,
            ctx,
            CapHeightWidget::child_mut(&mut element),
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
            .teardown(state, ctx, CapHeightWidget::child_mut(&mut element));
    }

    fn message(
        &self,
        state: &mut Self::ViewState,
        ctx: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut State,
    ) -> MessageResult<()> {
        self.content
            .message(state, ctx, CapHeightWidget::child_mut(&mut element), app)
    }
}
