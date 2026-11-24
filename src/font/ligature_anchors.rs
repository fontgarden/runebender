// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Anchor names and the component anchors of ligatures.
//!
//! The mark writer of fontc reads the kind of an anchor from its name: `top` is a base
//! anchor, `_top` a mark anchor, `top_1` the `top` anchor of component 1 of a ligature,
//! `_1` a ligature component without anchors, `caret_1` a ligature caret, and `entry` and
//! `exit` cursive anchors.
//! [`classify_anchor`] applies the same rule, by calling `fontir::ir::AnchorKind::new`, so
//! views, tools and the compile agree.
//!
//! [`propose_ligature_anchors`] derives the numbered anchors of a ligature from the anchors
//! of the glyphs it replaces.
//! The advances of the components are scaled to the advance of the ligature and laid out
//! in visual order: right to left for Arabic and Hebrew, so component 1 is the rightmost.
//! Each base anchor of component N becomes `name_N` at the same place in that span, a
//! component without base anchors gets `_N`, and `caret_N` marks the boundary between
//! components N and N + 1.
//! The function writes nothing; [`write_ligature_anchor_proposal`] writes the result as a
//! proposal layer, which the designer installs or discards.
//!
//! The caller supplies the component glyphs, usually from
//! `text::arabic_features::ligature_input_names`, because the font module does not depend
//! on the text module.

use std::fmt;

use kurbo::Point;

use crate::font::composites::{effective_document_anchors, layer_anchors};
use crate::font::project::Project;
use crate::font::proposal::{
    AnchorPlacements, ProposalError, ProposalSummary, write_anchor_proposal_project,
};
use crate::font::variable::SourceId;

/// The proposal task of ligature anchors.
pub const TASK: &str = "ligature-anchors";

/// What an anchor does, by its name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AnchorRole {
    /// A base anchor that marks attach to, such as `top`.
    Base {
        /// The attachment group, such as `top`.
        group: String,
    },
    /// A mark anchor, such as `_top`.
    Mark {
        /// The attachment group, such as `top`.
        group: String,
    },
    /// A base anchor of one ligature component, such as `top_1`.
    Ligature {
        /// The attachment group, such as `top`.
        group: String,
        /// The component, from 1, in logical order.
        component: usize,
    },
    /// A ligature component without anchors, such as `_2`.
    EmptyComponent(usize),
    /// A ligature caret, such as `caret_1`.
    Caret(usize),
    /// A vertical ligature caret, such as `vcaret_1`.
    VerticalCaret(usize),
    /// The cursive entry anchor, `entry`.
    CursiveEntry,
    /// The cursive exit anchor, `exit`.
    CursiveExit,
}

/// Why fontc rejects an anchor name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnchorNameError {
    /// A component or caret index of 0, such as `top_0`.
    ZeroIndex,
    /// A numbered mark anchor, such as `_top_1`.
    NumberedMark,
    /// A mark anchor without a group, `_`.
    EmptyMarkGroup,
    /// Another reason that fontc gives.
    Other,
}

impl fmt::Display for AnchorNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroIndex => "component and caret numbers start at 1",
            Self::NumberedMark => "a mark anchor cannot have a component number",
            Self::EmptyMarkGroup => "a mark anchor needs a name after the underscore",
            Self::Other => "fontc does not accept this anchor name",
        })
    }
}

impl std::error::Error for AnchorNameError {}

/// The role of an anchor named `name`, by the rule of fontc.
pub fn classify_anchor(name: &str) -> Result<AnchorRole, AnchorNameError> {
    use fontir::error::BadAnchorReason;
    use fontir::ir::AnchorKind;

    let kind = AnchorKind::new(name).map_err(|reason| match reason {
        BadAnchorReason::ZeroIndex => AnchorNameError::ZeroIndex,
        BadAnchorReason::NumberedMarkAnchor => AnchorNameError::NumberedMark,
        BadAnchorReason::NilMarkGroup => AnchorNameError::EmptyMarkGroup,
        _ => AnchorNameError::Other,
    })?;
    Ok(match kind {
        AnchorKind::Base(group) => AnchorRole::Base {
            group: group.to_string(),
        },
        AnchorKind::Mark(group) => AnchorRole::Mark {
            group: group.to_string(),
        },
        AnchorKind::Ligature { group_name, index } => AnchorRole::Ligature {
            group: group_name.to_string(),
            component: index,
        },
        AnchorKind::ComponentMarker(index) => AnchorRole::EmptyComponent(index),
        AnchorKind::Caret(index) => AnchorRole::Caret(index),
        AnchorKind::VCaret(index) => AnchorRole::VerticalCaret(index),
        AnchorKind::CursiveEntry => AnchorRole::CursiveEntry,
        AnchorKind::CursiveExit => AnchorRole::CursiveExit,
    })
}

/// The visual order of the components of a ligature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LigatureDirection {
    /// Component 1 is the leftmost.
    LeftToRight,
    /// Component 1 is the rightmost, as in Arabic and Hebrew.
    RightToLeft,
}

impl fmt::Display for LigatureDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::LeftToRight => "left to right",
            Self::RightToLeft => "right to left",
        })
    }
}

/// One proposed anchor of a ligature.
#[derive(Clone, Debug, PartialEq)]
pub struct ProposedAnchor {
    /// The anchor name, such as `top_1`.
    pub name: String,
    /// The proposed position in the ligature, rounded to whole units.
    pub position: Point,
    /// The position of the anchor of that name that the ligature has now.
    pub existing: Option<Point>,
    /// How the position was derived.
    pub reason: String,
}

/// The proposed component anchors of one ligature.
#[derive(Clone, Debug, PartialEq)]
pub struct LigatureAnchors {
    /// The ligature glyph.
    pub ligature: String,
    /// The glyphs that the ligature replaces, in logical order.
    pub components: Vec<String>,
    /// The visual order of the components.
    pub direction: LigatureDirection,
    /// The proposed anchors: the anchors of each component in order, then the carets.
    pub anchors: Vec<ProposedAnchor>,
}

impl LigatureAnchors {
    /// The anchors to write: those the ligature lacks, or with `only_missing` false, also
    /// those at another position.
    pub fn changes(&self, only_missing: bool) -> Vec<(String, Point)> {
        self.anchors
            .iter()
            .filter(|anchor| match anchor.existing {
                None => true,
                Some(existing) => !only_missing && existing != anchor.position,
            })
            .map(|anchor| (anchor.name.clone(), anchor.position))
            .collect()
    }
}

/// The span of each component across the advance of the ligature, as left and right x.
fn component_spans(widths: &[f64], advance: f64, direction: LigatureDirection) -> Vec<(f64, f64)> {
    let total: f64 = widths.iter().sum();
    let scale = advance / total;
    let mut start = 0.0;
    widths
        .iter()
        .map(|width| {
            let span = width * scale;
            let edges = match direction {
                LigatureDirection::LeftToRight => (start, start + span),
                LigatureDirection::RightToLeft => (advance - start - span, advance - start),
            };
            start += span;
            edges
        })
        .collect()
}

/// The direction of a ligature from the code points of its components, or of the glyphs
/// they are forms of: right to left when one has a strong right-to-left character.
fn direction_of(
    project: &Project,
    layer: &crate::font::variable::LayerId,
    components: &[String],
) -> LigatureDirection {
    let right_to_left = components.iter().any(|name| {
        let base = name.split('.').next().unwrap_or(name);
        [name.as_str(), base].iter().any(|candidate| {
            project
                .document_layer(candidate, layer)
                .is_some_and(|view| {
                    view.codepoints().any(|character| {
                        matches!(
                            unicode_bidi::bidi_class(character),
                            unicode_bidi::BidiClass::R | unicode_bidi::BidiClass::AL
                        )
                    })
                })
        })
    });
    if right_to_left {
        LigatureDirection::RightToLeft
    } else {
        LigatureDirection::LeftToRight
    }
}

/// Propose the component anchors of `ligature` in the default layer of `source`.
///
/// `components` are the glyphs the ligature replaces, in logical order, in the positional
/// forms that the substitution reads, such as `lam-ar.init` and `alef-ar.fina` for
/// `lam_alef-ar`.
/// Every component anchor and caret is proposed, with the position the ligature has now
/// when it has one; [`LigatureAnchors::changes`] selects what to write.
/// Nothing in the project changes.
pub fn propose_ligature_anchors(
    project: &Project,
    source: SourceId,
    ligature: &str,
    components: &[String],
) -> Result<LigatureAnchors, String> {
    let layer = project
        .document_source(source)
        .ok_or("the source is not loaded")?
        .default_layer();
    let glyph = project
        .document_layer(ligature, &layer)
        .ok_or_else(|| format!("{ligature}: not in the source"))?;
    if components.len() < 2 {
        return Err(format!(
            "{ligature}: a ligature needs two or more components"
        ));
    }
    let advance = glyph.width();
    if advance <= 0.0 {
        return Err(format!("{ligature}: the ligature has no advance"));
    }
    let mut parts = Vec::with_capacity(components.len());
    for name in components {
        let view = project
            .document_layer(name, &layer)
            .ok_or_else(|| format!("{ligature}: component {name} is not in the source"))?;
        let anchors = effective_document_anchors(view, |base| project.document_layer(base, &layer));
        parts.push((name, view.width(), anchors));
    }
    let widths: Vec<f64> = parts.iter().map(|part| part.1).collect();
    let total: f64 = widths.iter().sum();
    if total <= 0.0 {
        return Err(format!("{ligature}: the components have no advance"));
    }
    let scale = advance / total;
    let direction = direction_of(project, &layer, components);
    let spans = component_spans(&widths, advance, direction);
    let existing = layer_anchors(glyph);
    let existing_at = |name: &str| {
        existing
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, point)| *point)
    };

    let mut anchors = Vec::new();
    for (index, ((name, _, component_anchors), (left, right))) in
        parts.iter().zip(&spans).enumerate()
    {
        let number = index + 1;
        let span =
            format!("component {number}, x {left:.0} to {right:.0} {direction}, scale {scale:.3}");
        let mut found = false;
        for (anchor, point) in component_anchors {
            let Ok(AnchorRole::Base { group }) = classify_anchor(anchor) else {
                continue;
            };
            found = true;
            let anchor_name = format!("{group}_{number}");
            let position = Point::new((left + point.x * scale).round(), point.y.round());
            anchors.push(ProposedAnchor {
                existing: existing_at(&anchor_name),
                reason: format!(
                    "`{anchor}` of {name} at ({}, {}), placed in {span}",
                    point.x, point.y
                ),
                name: anchor_name,
                position,
            });
        }
        if !found {
            let anchor_name = format!("_{number}");
            anchors.push(ProposedAnchor {
                existing: existing_at(&anchor_name),
                reason: format!(
                    "{name} has no base anchor, so `_{number}` marks {span} as present"
                ),
                name: anchor_name,
                position: Point::new(((left + right) * 0.5).round(), 0.0),
            });
        }
    }
    for index in 1..parts.len() {
        let (left, right) = spans[index - 1];
        let x = match direction {
            LigatureDirection::LeftToRight => right,
            LigatureDirection::RightToLeft => left,
        };
        let anchor_name = format!("caret_{index}");
        anchors.push(ProposedAnchor {
            existing: existing_at(&anchor_name),
            reason: format!(
                "the boundary between component {index} ({}) and component {} ({})",
                parts[index - 1].0,
                index + 1,
                parts[index].0
            ),
            name: anchor_name,
            position: Point::new(x.round(), 0.0),
        });
    }
    Ok(LigatureAnchors {
        ligature: ligature.to_owned(),
        components: components.to_vec(),
        direction,
        anchors,
    })
}

/// Write proposed ligature anchors as the [`TASK`] proposal layer of `source`.
///
/// Each ligature gets the anchors of [`LigatureAnchors::changes`]; its other anchors,
/// outlines and advance stay as they are.
/// The layer replaces an earlier proposal of the same task; the foreground changes only
/// when the designer installs the proposal.
pub fn write_ligature_anchor_proposal(
    project: &mut Project,
    source: SourceId,
    proposals: &[LigatureAnchors],
    only_missing: bool,
) -> Result<ProposalSummary, ProposalError> {
    let placements: Vec<AnchorPlacements> = proposals
        .iter()
        .map(|proposal| AnchorPlacements {
            glyph: proposal.ligature.clone(),
            anchors: proposal.changes(only_missing),
        })
        .filter(|placement| !placement.anchors.is_empty())
        .collect();
    write_anchor_proposal_project(
        project,
        source,
        TASK,
        &placements,
        "Ligature anchors from the anchors of the component letters",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::proposal::{find_project, install_project, preview_project};
    use crate::text::arabic_features::ligature_input_names;
    use crate::text::layout_tables::{LayoutInputs, build_shaping_font};

    #[test]
    fn anchor_names_are_classified_as_fontc_classifies_them() {
        let base = |group: &str| AnchorRole::Base {
            group: group.into(),
        };
        let cases = [
            ("top", Ok(base("top"))),
            ("top.dot", Ok(base("top.dot"))),
            (
                "_top",
                Ok(AnchorRole::Mark {
                    group: "top".into(),
                }),
            ),
            (
                "top_2",
                Ok(AnchorRole::Ligature {
                    group: "top".into(),
                    component: 2,
                }),
            ),
            ("_3", Ok(AnchorRole::EmptyComponent(3))),
            ("caret_1", Ok(AnchorRole::Caret(1))),
            ("vcaret_2", Ok(AnchorRole::VerticalCaret(2))),
            ("entry", Ok(AnchorRole::CursiveEntry)),
            ("exit", Ok(AnchorRole::CursiveExit)),
            ("top_0", Err(AnchorNameError::ZeroIndex)),
            ("caret_0", Err(AnchorNameError::ZeroIndex)),
            ("_top_1", Err(AnchorNameError::NumberedMark)),
            ("_", Err(AnchorNameError::EmptyMarkGroup)),
        ];
        for (name, expected) in cases {
            assert_eq!(classify_anchor(name), expected, "{name}");
        }
    }

    #[test]
    fn components_span_the_advance_in_visual_order() {
        let ltr = component_spans(&[200.0, 300.0], 1000.0, LigatureDirection::LeftToRight);
        assert_eq!(ltr, [(0.0, 400.0), (400.0, 1000.0)], "left to right");
        let rtl = component_spans(&[200.0, 300.0], 1000.0, LigatureDirection::RightToLeft);
        assert_eq!(rtl, [(600.0, 1000.0), (0.0, 600.0)], "right to left");
    }

    fn fixture() -> (Project, SourceId) {
        let project = Project::load(&crate::testing::fonts::arabic_regular_ufo()).unwrap();
        let source = project.document_sources().next().unwrap().id();
        (project, source)
    }

    fn inputs(project: &Project, ligature: &str) -> Vec<String> {
        ligature_input_names(ligature, |name| project.document_glyph(name).is_some()).unwrap()
    }

    fn anchor<'a>(proposal: &'a LigatureAnchors, name: &str) -> &'a ProposedAnchor {
        proposal
            .anchors
            .iter()
            .find(|anchor| anchor.name == name)
            .unwrap_or_else(|| panic!("{name} in {proposal:?}"))
    }

    #[test]
    fn the_fixture_ligature_gets_its_anchors_on_the_correct_sides() {
        let (project, source) = fixture();
        let components = inputs(&project, "lam_alef-ar");
        assert_eq!(
            components,
            ["lam-ar.init", "alef-ar.fina"],
            "positional forms"
        );
        let proposal =
            propose_ligature_anchors(&project, source, "lam_alef-ar", &components).unwrap();
        assert_eq!(proposal.direction, LigatureDirection::RightToLeft, "Arabic");
        let names: Vec<&str> = proposal.anchors.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            ["top_1", "bottom_1", "top_2", "bottom_2", "caret_1"],
            "component order, then carets"
        );
        // The advances 240 and 200 scale to 520: lam spans x 236 to 520, alef 0 to 236.
        assert_eq!(
            anchor(&proposal, "top_1").position,
            Point::new(355.0, 760.0),
            "lam"
        );
        assert_eq!(
            anchor(&proposal, "top_2").position,
            Point::new(118.0, 760.0),
            "alef"
        );
        assert_eq!(
            anchor(&proposal, "bottom_1").position,
            Point::new(355.0, -140.0),
            "lam below"
        );
        assert_eq!(
            anchor(&proposal, "caret_1").position,
            Point::new(236.0, 0.0),
            "caret"
        );
        assert_eq!(
            anchor(&proposal, "top_1").existing,
            Some(Point::new(380.0, 760.0)),
            "the anchor the fixture has"
        );
        assert_eq!(
            anchor(&proposal, "caret_1").existing,
            Some(Point::new(260.0, 0.0)),
            "the caret the fixture has"
        );
        assert!(
            anchor(&proposal, "top_1").reason.contains("lam-ar.init"),
            "{}",
            anchor(&proposal, "top_1").reason
        );
        assert!(
            proposal.changes(true).is_empty(),
            "the fixture has every anchor, the caret included"
        );
        assert_eq!(proposal.changes(false).len(), 5, "every anchor differs");
    }

    #[test]
    fn a_component_without_base_anchors_gets_an_empty_component_anchor() {
        let (mut project, source) = fixture();
        let layer = project.document_source(source).unwrap().default_layer();
        let ids: Vec<_> = project
            .document_layer("alef-ar.fina", &layer)
            .unwrap()
            .anchors()
            .map(|anchor| anchor.id())
            .collect();
        project
            .edit_document_layer("alef-ar.fina", &layer, |draft| {
                for id in ids {
                    draft.remove_anchor(id)?;
                }
                Ok(())
            })
            .unwrap();
        let components = inputs(&project, "lam_alef-ar");
        let proposal =
            propose_ligature_anchors(&project, source, "lam_alef-ar", &components).unwrap();
        let empty = anchor(&proposal, "_2");
        assert_eq!(empty.position, Point::new(118.0, 0.0), "the middle of alef");
        assert!(
            proposal.anchors.iter().all(|a| a.name != "top_2"),
            "{proposal:?}"
        );
    }

    /// The absolute x of each glyph of `text`, shaped right to left, by glyph name.
    fn shaped_x(project: &Project, source: SourceId, text: &str) -> Vec<(String, f64, f64)> {
        let inputs = LayoutInputs::from_project(project, source).unwrap();
        let font = build_shaping_font(&inputs).unwrap();
        let mut pen = 0.0;
        font.shape(text, true)
            .unwrap()
            .iter()
            .map(|glyph| {
                let name = font.glyph_name(glyph.glyph_id).unwrap().to_owned();
                let x = pen + glyph.x_offset;
                pen += glyph.x_advance;
                (name, x, glyph.y_offset)
            })
            .collect()
    }

    #[test]
    fn proposed_anchors_attach_the_marks_of_lam_alef_to_their_components() {
        let (mut project, source) = fixture();
        let layer = project.document_source(source).unwrap().default_layer();
        let removed = ["top_1", "top_2", "bottom_1", "bottom_2", "caret_1"];
        let ids: Vec<_> = project
            .document_layer("lam_alef-ar", &layer)
            .unwrap()
            .anchors()
            .filter(|anchor| removed.contains(&anchor.name()))
            .map(|anchor| anchor.id())
            .collect();
        assert_eq!(ids.len(), 5, "the fixture has the five anchors");
        project
            .edit_document_layer("lam_alef-ar", &layer, |draft| {
                for id in ids {
                    draft.remove_anchor(id)?;
                }
                Ok(())
            })
            .unwrap();

        let components = inputs(&project, "lam_alef-ar");
        let proposal =
            propose_ligature_anchors(&project, source, "lam_alef-ar", &components).unwrap();
        let before = project.document_revision();
        let summary = write_ligature_anchor_proposal(
            &mut project,
            source,
            std::slice::from_ref(&proposal),
            true,
        )
        .unwrap();
        assert_eq!(summary.glyphs, ["lam_alef-ar"], "one proposed glyph");
        assert_eq!(summary.compatible, ["lam_alef-ar"], "the outline is kept");
        assert_ne!(
            project.document_revision(),
            before,
            "the proposal is a document change"
        );
        let foreground = project.document_layer("lam_alef-ar", &layer).unwrap();
        assert_eq!(
            foreground.anchors().count(),
            0,
            "the foreground is unchanged"
        );
        let proposed = preview_project(&project, source, TASK, "lam_alef-ar").unwrap();
        let mut names: Vec<&str> = proposed.anchors().map(|a| a.name()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            ["bottom_1", "bottom_2", "caret_1", "top_1", "top_2"],
            "the proposal layer"
        );

        install_project(&mut project, source, TASK, None, true).unwrap();
        assert!(find_project(&project, source, TASK).is_err(), "installed");

        // Fatha on lam and damma on alef.
        let glyphs = shaped_x(&project, source, "لَاُ");
        let x = |name: &str| {
            glyphs
                .iter()
                .find(|glyph| glyph.0 == name)
                .unwrap_or_else(|| panic!("{name} in {glyphs:?}"))
                .1
        };
        let ligature = x("lam_alef-ar");
        let advance = project
            .document_layer("lam_alef-ar", &layer)
            .unwrap()
            .width();
        let fatha = x("fatha-ar") - ligature;
        let damma = x("damma-ar") - ligature;
        assert_eq!(
            fatha,
            anchor(&proposal, "top_1").position.x,
            "fatha on top_1"
        );
        assert_eq!(
            damma,
            anchor(&proposal, "top_2").position.x,
            "damma on top_2"
        );
        assert!(fatha > advance * 0.5, "lam is on the right: {fatha}");
        assert!(damma < advance * 0.5, "alef is on the left: {damma}");
        for (name, _, y) in &glyphs {
            if name != "lam_alef-ar" {
                assert_eq!(*y, 760.0, "{name} sits on the top anchors");
            }
        }
    }
}
