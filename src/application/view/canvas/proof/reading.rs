// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reading and waterfall pages: configurable samples, common axes, and selectable specimens.

use super::{comparison_picker, mode_bar};
use crate::application::design::areas::proof as design;
use crate::application::design::recipes::{self, proof as page};
use crate::application::design::{Region, Space, column, row};
use crate::application::editor::tools::review::ReviewMode;
use crate::application::editor::tools::review_reading::{Sample, missing_coverage};
use crate::application::widgets::scroll_viewport::portal;
use crate::application::widgets::source_text_area::source_text_area;
use crate::application::workspace::Workspace;
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::style::Style;
use xilem::view::{FlexExt as _, sized_box};
use xilem::{AnyWidgetView, WidgetView};

pub(super) fn reading_view(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let appearance = app.palette.proof_page(app.review.dark);
    let pal = appearance.as_ref();
    let sample = app.review_sample_text();
    let family = app
        .font
        .font_info()
        .names
        .family_name
        .clone()
        .unwrap_or_else(|| "Untitled".into());
    let fonts = app.review_reading_fonts();
    let variations = app.review_variations();
    let waterfall = app.review.mode == ReviewMode::Waterfall;
    let sizes: Vec<f32> = if waterfall {
        vec![72.0, 48.0, 36.0, 24.0, 18.0, 12.0]
    } else {
        vec![crate::application::view::render::px32(
            app.review.reading.size,
        )]
    };
    let mut cards: Vec<Box<AnyWidgetView<Workspace>>> = Vec::new();
    for font in fonts {
        let mut contents: Vec<Box<AnyWidgetView<Workspace>>> =
            vec![page::caption(pal, font.label.clone()).boxed()];
        if let Some(status) = font.status {
            contents.push(page::band(recipes::prose(pal, status)).boxed());
        }
        if let Some(bytes) = font.bytes {
            let missing = missing_coverage(&bytes, &sample);
            if !missing.is_empty() {
                contents.push(page::band(recipes::prose(pal, missing)).boxed());
                cards.push(column(Region::List, contents).gap(Space::None).boxed());
                continue;
            }
            let variations = font.variations.as_ref().unwrap_or(&variations);
            let mut specimens: Vec<Box<AnyWidgetView<Workspace>>> = Vec::new();
            for size in &sizes {
                specimens.push(
                    page::text(
                        pal,
                        bytes.clone(),
                        sample.clone(),
                        *size,
                        variations.clone(),
                    )
                    .boxed(),
                );
            }
            contents.push(
                page::band(column(Region::List, specimens).gap(Length::px(design::UNIT))).boxed(),
            );
        }
        cards.push(column(Region::List, contents).gap(Space::None).boxed());
    }
    // Keep revisions together; reference fonts follow in pairs below them.
    let mut pairs: Vec<Box<AnyWidgetView<Workspace>>> = Vec::new();
    let mut cards = cards.into_iter();
    while let Some(left) = cards.next() {
        let pair = vec![Some(left), cards.next()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let pair: Vec<_> = pair
            .into_iter()
            .map(|card| {
                sized_box(card)
                    .dims(Dimensions::new(Dim::Fixed(Length::ZERO), Dim::Auto))
                    .flex(1.0)
            })
            .collect();
        pairs.push(
            row(Region::Inline, pair)
                .gap(Length::px(design::COLUMN_GAP))
                .boxed(),
        );
    }
    let comparison = app.review_comparison();
    let apply = comparison.proposal_task.as_ref().map(|task| {
        let task = task.clone();
        row(
            Region::Inline,
            (
                page::action(
                    pal,
                    "Apply glyph".into(),
                    false,
                    !comparison.can_apply_glyph,
                    Workspace::apply_review_glyph,
                ),
                page::action(
                    pal,
                    format!(
                        "Discard proposal ({} glyphs)",
                        comparison.proposal_glyph_count
                    ),
                    false,
                    false,
                    move |app| {
                        app.discard_proposal(&task);
                    },
                ),
            ),
        )
        .gap(Length::px(design::COMPACT_GAP))
    });
    page::surface(
        pal,
        column(
            Region::List,
            (
                page::header_band(page::heading_row(pal, family,
                    row(Region::Inline, (
                        reference_button(app, pal),
                        page::tab(pal, "Dark", app.review.dark, false, |app| {
                            app.review.dark = !app.review.dark;
                        }),
                    )).gap(Length::px(design::COMPACT_GAP)),
                )),
                page::rule(pal),
                mode_bar(app, pal),
                sized_box(portal(column(
                    Region::List,
                    (
                        comparison_picker(app, pal),
                        page::band(sample_controls(app, pal)),
                        (!app.font.axes.is_empty()).then(|| page::band(axis_controls(app, pal))),
                        reference_errors(app, pal).map(page::band),
                        apply.map(page::band),
                        comparison.proposal_task.as_ref().map(|_| page::band(recipes::prose(
                            pal,
                            format!("Apply glyph changes only {} in the active master.", comparison.glyph),
                        ))),
                        app.review.reading.error.clone().map(|error| page::band(recipes::prose(pal, error))),
                        sample.is_empty().then(|| page::band(recipes::prose(
                            pal,
                            "Choose Custom and enter text; unencoded glyphs can be inspected in Compare.".into(),
                        ))),
                        column(Region::List, pairs).gap(Length::px(design::SECTION_GAP)),
                    ),
                ).gap(Space::None)).constrain_horizontal(true))
                    .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
                    .flex(1.0),
            ),
        ).gap(Space::None).padding(page::inset()),
    )
}

fn sample_controls(
    app: &Workspace,
    pal: &crate::application::design::theme::Palette,
) -> impl WidgetView<Workspace> + use<> {
    let choices = [
        ("Glyph", Sample::Glyph),
        ("Spacing", Sample::Spacing),
        ("Alphabet", Sample::Alphabet),
        ("Custom", Sample::Custom),
    ];
    let title = choices
        .iter()
        .find(|(_, sample)| *sample == app.review.reading.sample)
        .expect("sample label")
        .0;
    let menu = app.review.reading.sample_menu_open.then(|| {
        let buttons: Vec<_> = choices
            .into_iter()
            .map(|(name, sample)| {
                page::action(
                    pal,
                    name.into(),
                    sample == app.review.reading.sample,
                    false,
                    move |app| {
                        app.select_review_sample(sample);
                    },
                )
            })
            .collect();
        column(Region::List, buttons).gap(Length::px(design::COMPACT_GAP))
    });
    let custom = (app.review.reading.sample == Sample::Custom).then(|| {
        recipes::field_box(
            pal,
            sized_box(
                portal(
                    source_text_area(
                        app.review.reading.custom.clone(),
                        |app: &mut Workspace, text: String| {
                            app.review.reading.custom = text.chars().take(4096).collect();
                        },
                    )
                    .text_color(pal.text),
                )
                .constrain_horizontal(true),
            )
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::Fixed(Length::px(design::SAMPLE_HEIGHT)),
            )),
        )
    });
    column(
        Region::List,
        (
            recipes::prose(pal, "Sample".into()),
            page::selector(
                pal,
                title.into(),
                app.review.reading.sample_menu_open,
                |app| {
                    app.review.reading.sample_menu_open = !app.review.reading.sample_menu_open;
                },
            ),
            menu,
            custom,
            (app.review.mode == ReviewMode::Text).then(|| {
                column(
                    Region::List,
                    (
                        page::field_enter(
                            pal,
                            "Size (px)",
                            app.review.reading.size_buf.clone(),
                            |app, value| {
                                app.review.reading.size_buf = value;
                            },
                            |app, text| {
                                if let Ok(size) = text.parse::<f64>()
                                    && size.is_finite()
                                {
                                    app.review.reading.size = size.clamp(8.0, 160.0);
                                }
                                app.review.reading.size_buf = app.review.reading.size.to_string();
                            },
                        ),
                        recipes::neutral_slider(
                            pal,
                            8.0,
                            160.0,
                            app.review.reading.size,
                            |app, size| {
                                app.review.reading.size = size.round();
                                app.review.reading.size_buf = app.review.reading.size.to_string();
                            },
                        ),
                    ),
                )
                .gap(Length::px(design::COMPACT_GAP))
            }),
        ),
    )
    .gap(Length::px(design::COMPACT_GAP))
}

fn axis_controls(
    app: &Workspace,
    pal: &crate::application::design::theme::Palette,
) -> impl WidgetView<Workspace> + use<> {
    let fields: Vec<_> = app
        .font
        .axes
        .iter()
        .enumerate()
        .map(|(index, axis)| {
            let value = app
                .review
                .reading
                .axes
                .get(&axis.tag)
                .copied()
                .unwrap_or_else(|| app.axis_values.get(index).copied().unwrap_or(axis.default));
            let tag = axis.tag.clone();
            let edit_tag = tag.clone();
            let slider_tag = tag.clone();
            let buffer = app
                .review
                .reading
                .axis_buffers
                .get(&tag)
                .cloned()
                .unwrap_or_else(|| value.to_string());
            let (min, max) = (axis.min, axis.max);
            column(
                Region::List,
                (
                    recipes::prose(pal, format!("{} ({min}–{max})", axis.name)),
                    page::field_enter(
                        pal,
                        "",
                        buffer,
                        move |app, text| {
                            app.review
                                .reading
                                .axis_buffers
                                .insert(edit_tag.clone(), text);
                        },
                        move |app, text| {
                            let value = text
                                .parse::<f64>()
                                .ok()
                                .filter(|v| v.is_finite())
                                .unwrap_or(value)
                                .clamp(min, max);
                            app.review.reading.axes.insert(tag.clone(), value);
                            app.review
                                .reading
                                .axis_buffers
                                .insert(tag.clone(), value.to_string());
                        },
                    ),
                    recipes::neutral_slider(pal, min, max, value, move |app, value| {
                        app.review.reading.axes.insert(slider_tag.clone(), value);
                        app.review
                            .reading
                            .axis_buffers
                            .insert(slider_tag.clone(), format!("{value:.1}"));
                    }),
                ),
            )
            .gap(Length::px(design::COMPACT_GAP))
        })
        .collect();
    column(Region::List, fields).gap(Length::px(design::COMPACT_GAP))
}

pub(super) fn reference_button(
    app: &Workspace,
    pal: &crate::application::design::theme::Palette,
) -> Option<impl WidgetView<Workspace> + use<>> {
    (!app.review.reading.references.faces.is_empty()).then(|| {
        page::tab(
            pal,
            "References",
            app.review.reading.show_references,
            false,
            |app| {
                app.review.reading.show_references = !app.review.reading.show_references;
            },
        )
    })
}

pub(super) fn reference_errors(
    app: &Workspace,
    pal: &crate::application::design::theme::Palette,
) -> Option<impl WidgetView<Workspace> + use<>> {
    let errors = &app.review.reading.references.errors;
    (!errors.is_empty()).then(|| recipes::prose(pal, errors.join("; ")))
}
