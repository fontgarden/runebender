// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Review-page controls using the editor's outlined buttons on a continuous paper surface.
//! The whole page uses a local appearance derived from the active editor palette.
//! Controls keep the shared geometry and interaction states in either appearance.

use super::{Marker, button, marker};
use crate::application::design::areas::proof as design;
use crate::application::design::theme::Palette;
use crate::application::design::{
    ControlKind, ControlSize, ControlStyle, Region, Space, Stroke, TextSize, column, row,
};
use crate::application::view::label;
use crate::application::workspace::Workspace;
use masonry::layout::{Dim, Length};
use masonry::properties::{Dimensions, LineBreaking, Padding};
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::{FlexExt as _, sized_box};

/// One continuous, opaque review page, including the unused space below its contents.
pub(crate) fn surface<V: WidgetView<Workspace>>(
    pal: &Palette,
    child: V,
) -> impl WidgetView<Workspace> + use<V> {
    sized_box(child)
        .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
        .background_color(pal.panel)
}

/// Account for the heading line's internal top space in the visible page margin.
pub(crate) fn inset() -> Padding {
    Padding::all(Length::px(design::PAGE_INSET))
}

/// Shared title/control alignment for the family and glyph-identity rows.
pub(crate) fn heading_row<V: WidgetView<Workspace>>(
    pal: &Palette,
    text: String,
    controls: V,
) -> impl WidgetView<Workspace> + use<V> {
    use skrifa::MetadataProvider;
    let font = skrifa::FontRef::new(crate::application::view::UI_FONT).expect("bundled UI font");
    let metrics = font.metrics(
        skrifa::instance::Size::unscaled(),
        skrifa::instance::LocationRef::default(),
    );
    let height = ControlSize::Control.px();
    let font_size = crate::application::view::render::px32(height)
        * f32::from(metrics.units_per_em)
        / metrics.cap_height.expect("UI cap height");
    xilem::view::flex_row((
        sized_box(crate::application::widgets::cap_height::cap_height(
            label(text)
                .text_size(font_size)
                .color(pal.text)
                .line_break_mode(LineBreaking::Clip),
            height,
        ))
        .dims(Dimensions::new(Dim::Fixed(Length::ZERO), Dim::Auto))
        .flex(1.0),
        controls,
    ))
    .cross_axis_alignment(masonry::properties::types::CrossAxisAlignment::End)
    .gap(Length::px(design::UNIT))
}

pub(crate) fn heading_band<V: WidgetView<Workspace>>(
    child: V,
) -> impl WidgetView<Workspace> + use<V> {
    sized_box(header_band(child)).padding(Padding::top(Length::px(design::UNIT)))
}

/// A content band owns its vertical spacing; adjacent bands never add hidden stack gaps.
pub(crate) fn band<V: WidgetView<Workspace>>(child: V) -> impl WidgetView<Workspace> + use<V> {
    sized_box(child)
        .dims(Dimensions::new(Dim::Stretch, Dim::Auto))
        .padding(Padding::vertical(Length::px(design::UNIT)))
}

/// The header already contains `HEADER_DESCENT` beneath its visible content.
/// Include that allowance in the same UNIT-to-rule gap used by every other band.
pub(crate) fn header_band<V: WidgetView<Workspace>>(
    child: V,
) -> impl WidgetView<Workspace> + use<V> {
    sized_box(child)
        .dims(Dimensions::new(Dim::Stretch, Dim::Auto))
        .padding(Padding::bottom(Length::px(design::UNIT)))
}

/// A specimen heading with the same rule-to-content distance as the controls above it.
pub(crate) fn caption(pal: &Palette, text: String) -> impl WidgetView<Workspace> + use<> {
    column(Region::List, (rule(pal), band(super::prose(pal, text)))).gap(Space::None)
}

/// A rule aligned with the page's common content margins.
pub(crate) fn rule(pal: &Palette) -> impl WidgetView<Workspace> + use<> {
    sized_box(label(""))
        .dims(Dimensions::new(
            Dim::Stretch,
            Dim::Fixed(Stroke::Hairline.length()),
        ))
        .background_color(pal.outline)
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
        .disabled(pal, disabled)
        .proof_selected(pal, selected);
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
        .disabled(pal, disabled)
        .proof_selected(pal, selected);
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

/// Native, selectable lettering from the exact compiled specimen font.
pub(crate) fn text(
    pal: &Palette,
    bytes: std::sync::Arc<Vec<u8>>,
    text: String,
    size: f32,
    variations: String,
) -> impl WidgetView<Workspace> + use<> {
    crate::application::widgets::proof_text::ProofText {
        bytes,
        text,
        size,
        variations,
        color: pal.proof_ink(),
    }
}

/// A proof-local field: stock control geometry with the page's own paper, ink and caret.
pub(crate) fn field_enter<F, G>(
    pal: &Palette,
    name: &'static str,
    value: String,
    on_change: F,
    on_enter: G,
) -> impl WidgetView<Workspace> + use<F, G>
where
    F: Fn(&mut Workspace, String) + Send + Sync + 'static,
    G: Fn(&mut Workspace, String) + Send + Sync + 'static,
{
    use crate::application::view::text_input;
    use crate::application::widgets::input_typography::input_typography;
    super::labeled_control(
        pal,
        name,
        sized_box(input_typography(
            text_input(value, on_change)
                .on_enter(on_enter)
                .text_color(pal.text)
                .caret_color(pal.text)
                .background_color(pal.field())
                .border_color(pal.field_outline),
            pal.text,
        ))
        .dims(Dimensions::new(
            Dim::Stretch,
            Dim::from(ControlSize::Control),
        )),
    )
}
