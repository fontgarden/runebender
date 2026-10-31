// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Native glyph review, following the Virtua proof's quiet page hierarchy.
//!
//! Compare and Toggle read the same explicit comparison and fit both drawings
//! together. The pinned Before survives a return to editing; proposals are
//! chosen by identity and Apply is limited to the glyph actually reviewed.

use crate::application::design::areas::proof as design;
use crate::application::design::recipes::{self, proof as page};
use crate::application::design::theme::Palette;
use crate::application::design::{Region, Space, column, row};
use crate::application::editor::tools::review::{ReviewComparison, ReviewDrawing, ReviewMode};
use crate::application::view::label;
use crate::application::workspace::Workspace;
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Rect, Shape, Size};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::{FlexExt as _, FlexSpacer, canvas, sized_box};

pub(crate) fn proof_view(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    if matches!(app.review.mode, ReviewMode::Waterfall | ReviewMode::Text) {
        return reading::reading_view(app).boxed();
    }
    let appearance = app.palette.proof_page(app.review.dark);
    let pal = appearance.as_ref();
    let comparison = app.review_comparison();
    let family = app
        .font
        .font_info()
        .names
        .family_name
        .clone()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Untitled".into());
    let codepoint = app
        .font
        .glyphs
        .iter()
        .find(|entry| entry.name == comparison.glyph)
        .and_then(|entry| entry.codepoint);
    let has_pair = comparison.before.is_some();
    let toggle = app.review.mode == ReviewMode::Toggle;
    let show_before = app.review.show_before && has_pair;
    let mode_bar = mode_bar(app, pal);
    let revision_controls = toggle.then(|| {
        row(
            Region::Inline,
            (
                page::tab(
                    pal,
                    comparison.left_caption,
                    show_before,
                    !has_pair,
                    |app| app.show_review_revision(true),
                ),
                page::tab(pal, comparison.right_caption, !show_before, false, |app| {
                    app.show_review_revision(false);
                }),
                FlexSpacer::Flex(1.0),
                page::tab(
                    pal,
                    "Toggle",
                    false,
                    !has_pair,
                    Workspace::toggle_review_revision,
                ),
            ),
        )
        .gap(Length::px(design::COMPACT_GAP))
    });
    let actions = comparison.proposal_task.as_ref().map(|task| {
        let task = task.clone();
        let discard_label = if comparison.proposal_glyph_count > 1 {
            format!(
                "Discard proposal ({} glyphs)",
                comparison.proposal_glyph_count
            )
        } else {
            "Discard proposal".into()
        };
        row(
            Region::Inline,
            (
                sized_box(page::action(
                    pal,
                    "Apply glyph".into(),
                    false,
                    !comparison.can_apply_glyph,
                    Workspace::apply_review_glyph,
                ))
                .dims(Dimensions::new(Dim::Fixed(Length::ZERO), Dim::Auto))
                .flex(1.0),
                sized_box(page::action(pal, discard_label, false, false, move |app| {
                    app.discard_proposal(&task);
                }))
                .dims(Dimensions::new(Dim::Fixed(Length::ZERO), Dim::Auto))
                .flex(1.0),
            ),
        )
        .gap(Length::px(design::COMPACT_GAP))
    });
    let hint = comparison.message.clone().or_else(|| {
        comparison.proposal_task.as_ref().map(|_| {
            format!(
                "Apply glyph changes only {} in the active master.",
                comparison.glyph
            )
        })
    });
    let captions = if toggle || !has_pair {
        vec![if show_before {
            comparison.left_caption
        } else {
            comparison.right_caption
        }]
    } else {
        vec![comparison.left_caption, comparison.right_caption]
    };
    let captions: Vec<_> = captions
        .into_iter()
        .map(|caption| {
            sized_box(page::caption(pal, caption.into()))
                .dims(Dimensions::new(Dim::Fixed(Length::ZERO), Dim::Auto))
                .flex(1.0)
        })
        .collect();
    let show_references =
        app.review.reading.show_references && !app.review.reading.references.faces.is_empty();
    let specimens = if show_references {
        references::specimens(app, pal, comparison, codepoint).boxed()
    } else {
        column(
            Region::List,
            (
                row(Region::Inline, captions).gap(Length::px(design::COLUMN_GAP)),
                specimen(app, pal, comparison, app.review.mode, show_before).flex(1.0),
            ),
        )
        .gap(Space::None)
        .boxed()
    };
    page::surface(
        pal,
        column(
            Region::List,
            (
                page::header_band(page::heading_row(
                    pal,
                    family,
                    row(
                        Region::Inline,
                        (
                            reading::reference_button(app, pal),
                            page::tab(pal, "Dark", app.review.dark, false, |app| {
                                app.review.dark = !app.review.dark;
                            }),
                        ),
                    )
                    .gap(Length::px(design::COMPACT_GAP)),
                )),
                page::rule(pal),
                mode_bar,
                page::band(
                    row(
                        Region::Inline,
                        (
                            page::tab(pal, "Red pencil", app.review.pencil, false, |app| {
                                app.review.pencil = !app.review.pencil;
                            }),
                            page::tab(pal, "Undo ink", false, false, |app| {
                                let prefix = crate::application::widgets::proof_pencil::prefix(app);
                                app.review.annotations.lock().unwrap().undo(&prefix);
                            }),
                            page::tab(pal, "Clear ink", false, false, |app| {
                                let prefix = crate::application::widgets::proof_pencil::prefix(app);
                                app.review.annotations.lock().unwrap().clear(&prefix);
                            }),
                        ),
                    )
                    .gap(Length::px(design::COMPACT_GAP)),
                ),
                (app.review.mode != ReviewMode::Compare).then(|| page::rule(pal)),
                comparison_picker(app, pal),
                revision_controls.map(page::band),
                actions.map(page::band),
                hint.map(|text| page::band(recipes::prose(pal, text))),
                reading::reference_errors(app, pal).map(page::band),
                specimens.flex(1.0),
            ),
        )
        .gap(Space::None)
        .padding(page::inset()),
    )
    .boxed()
}

fn mode_bar(app: &Workspace, pal: &Palette) -> impl WidgetView<Workspace> + use<> {
    let modes = [
        ("Compare", ReviewMode::Compare),
        ("Toggle", ReviewMode::Toggle),
        ("Waterfall", ReviewMode::Waterfall),
        ("Text", ReviewMode::Text),
    ];
    let tabs: Vec<_> = modes
        .into_iter()
        .map(|(name, mode)| {
            page::tab(pal, name, app.review.mode == mode, false, move |app| {
                app.select_review_mode(mode);
            })
        })
        .collect();
    let glyph = &app.session.glyph_name;
    let identity = app
        .font
        .glyphs
        .iter()
        .find(|entry| &entry.name == glyph)
        .and_then(|entry| entry.codepoint)
        .map_or_else(
            || "Unencoded glyph".into(),
            |c| format!("U+{:04X}", u32::from(c)),
        );
    column(
        Region::List,
        (
            page::heading_band(page::heading_row(
                pal,
                format!("{glyph}  {identity}"),
                row(Region::Inline, tabs).gap(Length::px(design::COMPACT_GAP)),
            )),
            // Compare's specimen captions supply the next rule without a revision toolbar.
            (app.review.mode != ReviewMode::Compare).then(|| page::rule(pal)),
        ),
    )
    .gap(Space::None)
}

mod reading;
pub(crate) mod references;

fn comparison_picker(app: &Workspace, pal: &Palette) -> Option<impl WidgetView<Workspace> + use<>> {
    let selected = app.ai.preview_task.clone();
    let tasks: Vec<_> = app
        .ai
        .proposals
        .iter()
        .filter(|proposal| proposal.glyphs.contains(&app.session.glyph_name))
        .map(|proposal| proposal.task.clone())
        .collect();
    // A comparison source is not a revision switch. Keep sources visible and omit
    // the redundant chooser entirely when only Before / Working is available.
    (!tasks.is_empty()).then(|| {
        let choices: Vec<_> = std::iter::once(None)
            .chain(tasks.into_iter().map(Some))
            .map(|task| {
                let active = task == selected;
                let title = task.as_ref().map_or_else(
                    || "Before / Working".into(),
                    |name| format!("Working / {name}"),
                );
                page::action(pal, title, active, false, move |app| {
                    app.select_review_proposal(task.clone());
                })
            })
            .collect();
        page::band(
            column(
                Region::List,
                (
                    label("Comparison source").color(pal.text),
                    column(Region::List, choices).gap(Length::px(design::COMPACT_GAP)),
                ),
            )
            .gap(Length::px(design::COMPACT_GAP)),
        )
    })
}

/// Draw both sides in one field, so layout cannot give them different fit constraints.
fn specimen(
    app: &Workspace,
    pal: &Palette,
    comparison: ReviewComparison,
    mode: ReviewMode,
    show_before: bool,
) -> impl WidgetView<Workspace> + use<> {
    let ink = pal.proof_ink();
    let bounds = comparison_bounds(&comparison.working, comparison.before.as_ref());
    let paired = comparison.before.is_some() && mode == ReviewMode::Compare;
    use crate::application::widgets::proof_pencil::{self, Mapping};
    let drawings = if paired {
        vec![comparison.before.as_ref().unwrap(), &comparison.working]
    } else if show_before {
        vec![comparison.before.as_ref().unwrap_or(&comparison.working)]
    } else {
        vec![&comparison.working]
    };
    let mapping = Mapping {
        keys: drawings
            .into_iter()
            .enumerate()
            .map(|(i, d)| {
                proof_pencil::card_key(
                    app,
                    d,
                    if (paired && i == 0) || (!paired && show_before) {
                        comparison.left_caption
                    } else {
                        comparison.right_caption
                    },
                )
            })
            .collect(),
        bounds,
        paired,
        centered: false,
    };
    sized_box(proof_pencil::proof_pencil(
        canvas(move |_: &mut Workspace, _, scene, size: Size| {
            let (left, right) = comparison_transforms(bounds, size, paired);
            let mut painter = Painter::new(scene);
            if paired {
                if let Some(before) = &comparison.before {
                    painter.fill(&(left * &*before.outline), ink).draw();
                }
                painter
                    .fill(&(right * &*comparison.working.outline), ink)
                    .draw();
            } else {
                let drawing = if show_before {
                    comparison.before.as_ref().unwrap_or(&comparison.working)
                } else {
                    &comparison.working
                };
                painter.fill(&(left * &*drawing.outline), ink).draw();
            }
        }),
        mapping,
        app.review.annotations.clone(),
        app.review.pencil,
        app.palette.proof_pencil_ink(),
    ))
    .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
}

/// Include sidebearings, overhangs and both revisions, even when Toggle hides one.
pub(crate) fn comparison_bounds(working: &ReviewDrawing, before: Option<&ReviewDrawing>) -> Rect {
    let drawings = [Some(working), before];
    let advance = drawings
        .iter()
        .flatten()
        .map(|drawing| drawing.advance)
        .fold(1.0, f64::max);
    let ink = drawings
        .into_iter()
        .flatten()
        .filter(|drawing| !drawing.outline.elements().is_empty())
        .map(|drawing| drawing.outline.bounding_box())
        .reduce(|a, b| a.union(b));
    ink.map_or(Rect::new(0.0, 0.0, advance, 1.0), |ink| {
        Rect::new(
            ink.x0.min(0.0),
            ink.y0.min(0.0),
            ink.x1.max(advance),
            ink.y1.max(1.0),
        )
    })
}

/// Fit one shared ink/advance box into equal fields, with a shared baseline.
pub(crate) fn comparison_transforms(bounds: Rect, size: Size, paired: bool) -> (Affine, Affine) {
    let gap = if paired {
        design::COLUMN_GAP.min(size.width.max(0.0))
    } else {
        0.0
    };
    let width = if paired {
        (size.width - gap) / 2.0
    } else {
        size.width
    };
    let inset = design::INK_INSET
        .min(width.max(0.0) / 2.0)
        .min(size.height.max(0.0) / 2.0);
    let scale = ((width - 2.0 * inset).max(0.0) / bounds.width().max(1.0))
        .min((size.height - 2.0 * inset).max(0.0) / bounds.height().max(1.0));
    let x = (width - bounds.width() * scale) / 2.0 - bounds.x0 * scale;
    let y = (size.height - bounds.height() * scale) / 2.0 + bounds.y1 * scale;
    let left = Affine::scale_non_uniform(scale, -scale).then_translate((x, y).into());
    (left, left.then_translate((width + gap, 0.0).into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::kurbo::{BezPath, Point};
    use std::sync::Arc;

    fn drawing(ink: Rect, advance: f64) -> ReviewDrawing {
        ReviewDrawing {
            outline: Arc::new(ink.to_path(0.1)),
            advance,
        }
    }

    #[test]
    fn unequal_revisions_share_scale_baseline_and_room_for_overhangs() {
        let working = drawing(Rect::new(-120.0, -220.0, 840.0, 900.0), 600.0);
        let before = drawing(Rect::new(0.0, 0.0, 300.0, 700.0), 700.0);
        let bounds = comparison_bounds(&working, Some(&before));
        for size in [
            Size::new(240.0, 160.0),
            Size::new(800.0, 500.0),
            Size::new(1400.0, 900.0),
        ] {
            let (a, b) = comparison_transforms(bounds, size, true);
            assert_eq!(a.as_coeffs()[..4], b.as_coeffs()[..4]);
            assert_eq!((a * Point::ORIGIN).y, (b * Point::ORIGIN).y);
            let width = (size.width - design::COLUMN_GAP) / 2.0;
            for (transform, start) in [(a, 0.0), (b, width + design::COLUMN_GAP)] {
                let fit = transform.transform_rect_bbox(bounds);
                assert!(fit.x0 >= start && fit.x1 <= start + width);
                assert!(fit.y0 >= 0.0 && fit.y1 <= size.height);
            }
        }
    }

    #[test]
    fn toggle_uses_both_revisions_for_fitting_even_when_one_is_hidden() {
        let before = drawing(Rect::new(0.0, 0.0, 200.0, 700.0), 300.0);
        let working = drawing(Rect::new(-100.0, -300.0, 1000.0, 900.0), 900.0);
        let size = Size::new(500.0, 400.0);
        let bounds = comparison_bounds(&working, Some(&before));
        let swapped = comparison_bounds(&before, Some(&working));
        assert_eq!(
            comparison_transforms(bounds, size, false),
            comparison_transforms(swapped, size, false)
        );
        assert_ne!(
            comparison_transforms(bounds, size, false),
            comparison_transforms(comparison_bounds(&before, None), size, false)
        );
    }

    #[test]
    fn empty_glyphs_and_a_collapsed_specimen_have_finite_transforms() {
        let empty = ReviewDrawing {
            outline: Arc::new(BezPath::new()),
            advance: 0.0,
        };
        let bounds = comparison_bounds(&empty, None);
        for size in [Size::ZERO, Size::new(12.0, 8.0), Size::new(800.0, 400.0)] {
            for paired in [true, false] {
                let (a, b) = comparison_transforms(bounds, size, paired);
                assert!(
                    a.as_coeffs()
                        .iter()
                        .chain(b.as_coeffs().iter())
                        .all(|v| v.is_finite())
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "proof_tests.rs"]
mod layout_tests;
