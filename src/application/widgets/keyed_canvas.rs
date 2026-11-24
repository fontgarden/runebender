// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A canvas that records its scene again only when its key or its size changes.
//!
//! Xilem's `canvas` view cannot compare two draw closures, so it clears and records its
//! scene on every rebuild of the application and requests a repaint each time. It also
//! answers a size change with a rebuild of the whole application. An icon, a keyline or a
//! swatch draws the same thing until its inputs change, and the default editor layout holds
//! about forty of them.
//!
//! `keyed_canvas` takes those inputs as a key. The draw closure runs when the key differs
//! from the previous view's key, and when the widget's size differs from the size of the
//! last recording. The closure must draw only from the key, from values that the key
//! determines, and from the size it receives.

use std::marker::PhantomData;

use masonry::core::{ArcStr, MutateCtx};
use masonry::imaging::record::Scene;
use masonry::kurbo::Size;
use masonry::widgets::{self, CanvasSizeChanged};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

/// A canvas whose drawing depends only on `key` and on its size.
pub(crate) fn keyed_canvas<State, K, F>(key: K, draw: F) -> KeyedCanvas<State, K, F>
where
    State: 'static,
    K: PartialEq + 'static,
    F: Fn(&mut MutateCtx<'_>, &mut Scene, Size) + Send + Sync + 'static,
{
    KeyedCanvas {
        key,
        draw,
        alt_text: None,
        phantom: PhantomData,
    }
}

/// The view that [`keyed_canvas`] makes.
#[must_use = "View values do nothing unless provided to Xilem."]
pub(crate) struct KeyedCanvas<State, K, F> {
    key: K,
    draw: F,
    alt_text: Option<ArcStr>,
    phantom: PhantomData<fn() -> State>,
}

impl<State, K, F> KeyedCanvas<State, K, F> {
    /// Set the text that describes the drawing to a screen reader.
    pub(crate) fn alt_text(mut self, alt_text: impl Into<ArcStr>) -> Self {
        self.alt_text = Some(alt_text.into());
        self
    }
}

impl<State, K, F> KeyedCanvas<State, K, F>
where
    F: Fn(&mut MutateCtx<'_>, &mut Scene, Size) + Send + Sync + 'static,
{
    /// Record the scene at the widget's current size and remember that size.
    fn record(&self, drawn: &mut Option<Size>, element: &mut Mut<'_, Pod<widgets::Canvas>>) {
        *drawn = Some(element.widget.size());
        widgets::Canvas::update_scene(element, |ctx, scene, size| (self.draw)(ctx, scene, size));
    }
}

impl<State, K, F> ViewMarker for KeyedCanvas<State, K, F> {}

impl<State, Action, K, F> View<State, Action, ViewCtx> for KeyedCanvas<State, K, F>
where
    State: 'static,
    K: PartialEq + 'static,
    F: Fn(&mut MutateCtx<'_>, &mut Scene, Size) + Send + Sync + 'static,
{
    type Element = Pod<widgets::Canvas>;
    /// The size of the last recording; `None` before the first one.
    type ViewState = Option<Size>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        (
            ctx.with_action_widget(|ctx| {
                let widget = match &self.alt_text {
                    Some(alt_text) => widgets::Canvas::default().with_alt_text(alt_text.clone()),
                    None => widgets::Canvas::default(),
                };
                ctx.create_pod(widget)
            }),
            None,
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        drawn: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if self.key != prev.key || *drawn != Some(element.widget.size()) {
            self.record(drawn, &mut element);
        }
        if self.alt_text != prev.alt_text {
            widgets::Canvas::set_alt_text(&mut element, self.alt_text.clone());
        }
    }

    fn teardown(&self, _: &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        drawn: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<CanvasSizeChanged>() {
            // The new size needs a new recording, and nothing else in the application
            // depends on it, so the application is not rebuilt.
            Some(_) => {
                self.record(drawn, &mut element);
                MessageResult::Nop
            }
            None => MessageResult::Stale,
        }
    }
}
