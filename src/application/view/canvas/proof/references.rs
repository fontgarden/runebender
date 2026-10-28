// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Glyph comparison rows with a common em scale, including project-local reference faces.

use super::{comparison_bounds, comparison_transforms};
use crate::application::design::areas::proof as design;
use crate::application::design::recipes::{self, proof as page};
use crate::application::design::theme::Palette;
use crate::application::design::{Region, Space, column, row};
use crate::application::editor::tools::review::{ReviewComparison, ReviewDrawing, ReviewMode};
use crate::application::widgets::scroll_viewport::portal;
use crate::application::workspace::Workspace;
use masonry::imaging::Painter;
use masonry::kurbo::Size;
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use std::sync::Arc;
use xilem::style::Style;
use xilem::view::{FlexExt as _, canvas, sized_box};
use xilem::{AnyWidgetView, WidgetView};

struct Card {
    label: String,
    drawing: Option<ReviewDrawing>,
    error: Option<String>,
}

pub(super) fn specimens(
    app: &Workspace,
    pal: &Palette,
    comparison: ReviewComparison,
    character: Option<char>,
) -> impl WidgetView<Workspace> + use<> {
    let weight = app.review_reference_weight();
    let references: Vec<_> = app
        .review
        .reading
        .references
        .selected(weight)
        .into_iter()
        .map(|face| {
            let result = character
                .ok_or_else(|| "No Unicode character for this glyph".to_string())
                .and_then(|character| face.drawing(character, weight, app.font.units_per_em()));
            let label = if face.weight(weight) == weight {
                face.label.clone()
            } else {
                format!(
                    "{} · {} (nearest supplied)",
                    face.label,
                    face.weight(weight)
                )
            };
            match result {
                Ok((outline, advance)) => Card {
                    label,
                    drawing: Some(ReviewDrawing {
                        outline: Arc::new(outline),
                        advance,
                    }),
                    error: None,
                },
                Err(error) => Card {
                    label,
                    drawing: None,
                    error: Some(error),
                },
            }
        })
        .collect();
    // Include the hidden revision as well: Toggle never changes scale or reference positions.
    let bounds = references
        .iter()
        .filter_map(|card| card.drawing.as_ref())
        .fold(
            comparison_bounds(&comparison.working, comparison.before.as_ref()),
            |bounds, drawing| bounds.union(comparison_bounds(drawing, None)),
        );
    let main = if app.review.mode == ReviewMode::Toggle || comparison.before.is_none() {
        let show_before = app.review.show_before && comparison.before.is_some();
        vec![Card {
            label: if show_before {
                comparison.left_caption
            } else {
                comparison.right_caption
            }
            .into(),
            drawing: Some(if show_before {
                comparison.before.unwrap_or(comparison.working)
            } else {
                comparison.working
            }),
            error: None,
        }]
    } else {
        vec![
            Card {
                label: comparison.left_caption.into(),
                drawing: comparison.before,
                error: None,
            },
            Card {
                label: comparison.right_caption.into(),
                drawing: Some(comparison.working),
                error: None,
            },
        ]
    };
    let mut rows = vec![main];
    let mut references = references.into_iter();
    while let Some(left) = references.next() {
        rows.push(
            [Some(left), references.next()]
                .into_iter()
                .flatten()
                .collect(),
        );
    }
    let rows: Vec<Box<AnyWidgetView<Workspace>>> = rows
        .into_iter()
        .map(|cards| {
            let captions: Vec<_> = cards
                .iter()
                .map(|card| {
                    sized_box(
                        column(
                            Region::List,
                            (
                                page::caption(pal, card.label.clone()),
                                card.error.clone().map(|error| recipes::prose(pal, error)),
                            ),
                        )
                        .gap(Space::None),
                    )
                    .dims(Dimensions::new(Dim::Fixed(Length::ZERO), Dim::Auto))
                    .flex(1.0)
                })
                .collect();
            let ink = pal.proof_ink();
            let drawings: Vec<_> = cards.into_iter().map(|card| card.drawing).collect();
            column(
                Region::List,
                (
                    row(Region::Inline, captions).gap(Length::px(design::COLUMN_GAP)),
                    sized_box(canvas(move |_: &mut Workspace, _, scene, size: Size| {
                        let (left, right) = comparison_transforms(bounds, size, true);
                        let mut painter = Painter::new(scene);
                        for (index, drawing) in drawings.iter().enumerate() {
                            if let Some(drawing) = drawing {
                                let transform = if drawings.len() == 1 {
                                    left.then_translate(
                                        ((size.width + design::COLUMN_GAP) / 4.0, 0.0).into(),
                                    )
                                } else if index == 0 {
                                    left
                                } else {
                                    right
                                };
                                painter.fill(&(transform * &*drawing.outline), ink).draw();
                            }
                        }
                    }))
                    .dims(Dimensions::new(
                        Dim::Stretch,
                        Dim::Fixed(Length::px(design::REFERENCE_HEIGHT)),
                    )),
                ),
            )
            .gap(Space::None)
            .boxed()
        })
        .collect();
    sized_box(
        portal(column(Region::List, rows).gap(Length::px(design::SECTION_GAP)))
            .constrain_horizontal(true),
    )
    .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
}
