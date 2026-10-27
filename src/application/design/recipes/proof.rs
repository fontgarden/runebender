// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Review-page controls using the editor's outlined buttons on a continuous paper surface.
//! The page uses the panel's paired paper/ink, so headings and controls never
//! inherit the dark window ground behind the editor canvas.

use super::{Marker, button, marker};
use crate::application::design::theme::Palette;
use crate::application::design::{
    ControlKind, ControlSize, ControlStyle, Region, Space, TextSize, row,
};
use crate::application::view::label;
use crate::application::workspace::Workspace;
use masonry::layout::{Dim, Length};
use masonry::properties::{Dimensions, LineBreaking};
use std::sync::OnceLock;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::{FlexExt as _, sized_box};

/// Proof appearance is independent of the editor theme and shared across review modes.
pub(crate) fn palette(dark: bool) -> &'static Palette {
    static LIGHT: OnceLock<Palette> = OnceLock::new();
    static DARK: OnceLock<Palette> = OnceLock::new();
    if dark {
        DARK.get_or_init(|| Palette::load("dark"))
    } else {
        LIGHT.get_or_init(|| Palette::load("light"))
    }
}

/// One continuous, opaque review page, including the unused space below its contents.
pub(crate) fn surface<V: WidgetView<Workspace>>(
    pal: &Palette,
    child: V,
) -> impl WidgetView<Workspace> + use<V> {
    sized_box(child)
        .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
        .background_color(pal.panel)
}

/// A rule aligned with the page's common content margins.
pub(crate) fn rule(pal: &Palette) -> impl WidgetView<Workspace> + use<> {
    sized_box(label(""))
        .dims(Dimensions::new(
            Dim::Stretch,
            Dim::Fixed(Length::px(pal.stroke_emphasis)),
        ))
        .background_color(pal.text)
}

/// A proof mode or revision using the same outlined toggle as other editor sections.
pub(crate) fn tab<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: &str,
    selected: bool,
    disabled: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let style = ControlStyle::new(pal, ControlKind::Toggle)
        .selected(pal, selected)
        .disabled(pal, disabled);
    sized_box(button(style, label(text.to_owned()).color(style.ink()), on_click).padding(Space::Md))
        .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Control)))
}

/// A wrapping action or choice. Long task names must not widen the proof page.
pub(crate) fn action<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    selected: bool,
    disabled: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let style = ControlStyle::new(pal, ControlKind::Toggle)
        .selected(pal, selected)
        .disabled(pal, disabled);
    button(
        style,
        label(text)
            .text_size(TextSize::Body.px())
            .color(style.ink())
            .line_break_mode(LineBreaking::WordWrap),
        on_click,
    )
    .padding(Space::Md)
}

/// An outlined comparison chooser aligned with the page's left margin.
pub(crate) fn selector<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    open: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let style = ControlStyle::new(pal, ControlKind::Secondary);
    button(
        style,
        row(
            Region::Inline,
            (
                sized_box(
                    label(text)
                        .color(style.ink())
                        .line_break_mode(LineBreaking::WordWrap),
                )
                .dims(Dimensions::new(Dim::Fixed(Length::ZERO), Dim::Auto))
                .flex(1.0),
                marker(
                    if open { Marker::Open } else { Marker::Closed },
                    style.ink(),
                ),
            ),
        ),
        on_click,
    )
    .padding(Space::Md)
}
