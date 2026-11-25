// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The tolerant snapshot of the background preview compile.
//!
//! A font in progress often has glyphs whose masters are not finished: a contour with one more
//! point in Bold, or an anchor that only one master has. fontc refuses the whole font for one
//! such glyph, so the preview would lose interpolated outlines, the proof view and the axes
//! panel. The preview snapshot therefore removes what fontc cannot compile from those glyphs
//! and records each change; export, feature checks and proofs that must match export keep the
//! complete snapshot and its precise error.
//!
//! The changes are those that the exact fontc failures need, and no more:
//!
//! - Contours that do not interpolate are removed with Babelfont's `DropIncompatiblePaths`.
//!   Its compatibility test compares the path count, the node count of each path and the node
//!   kinds of the master and sparse layers.
//! - An anchor that a master or sparse layer has but the default master does not is removed.
//!   fontc reports it as "no value at default location". An anchor that only a non-default
//!   master lacks compiles, so it is kept and the preview equals export for it.

use std::collections::BTreeSet;
use std::fmt;

use babelfont::filters::{DropIncompatiblePaths, FontFilter as _};
use babelfont::{Layer, LayerType};

/// What the preview snapshot removed from one glyph so that the font compiles.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PreviewChange {
    /// Contours that do not interpolate between the masters were removed from every layer.
    PathsDropped,
    /// This anchor has no value at the default master and was removed from every layer.
    AnchorDropped(String),
}

impl fmt::Display for PreviewChange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathsDropped => formatter.write_str("paths dropped"),
            Self::AnchorDropped(name) => write!(formatter, "anchor {name} dropped"),
        }
    }
}

/// One glyph that the compiled preview shows differently from its sources, and why.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PreviewException {
    /// The glyph name.
    pub glyph: String,
    /// What the preview snapshot removed from the glyph.
    pub change: PreviewChange,
}

impl fmt::Display for PreviewException {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.glyph, self.change)
    }
}

/// Whether fontc reads this layer as a master or a sparse source of its glyph.
///
/// This is Babelfont's rule for `Layer::should_interpolate`, which is private to that crate.
fn interpolates(layer: &Layer) -> bool {
    !layer.is_background
        && (!layer.smart_component_location.is_empty()
            || layer.location.is_some()
            || matches!(layer.master, LayerType::DefaultForMaster(_)))
}

/// Whether the paths of these layers interpolate, by the test of `DropIncompatiblePaths`.
fn paths_compatible(layers: &[&Layer]) -> bool {
    let Some((first, others)) = layers.split_first() else {
        return true;
    };
    others.iter().all(|other| {
        other.paths().count() == first.paths().count()
            && first.paths().zip(other.paths()).all(|(a, b)| {
                a.nodes.len() == b.nodes.len()
                    && a.nodes
                        .iter()
                        .zip(&b.nodes)
                        .all(|(a, b)| a.nodetype == b.nodetype)
            })
    })
}

fn has_compatible_paths(glyph: &babelfont::Glyph) -> bool {
    let layers: Vec<&Layer> = glyph
        .layers
        .iter()
        .filter(|layer| interpolates(layer))
        .collect();
    paths_compatible(&layers)
}

/// Remove contours that do not interpolate, and return the names of the changed glyphs.
///
/// The filter is applied to one glyph at a time. With a list of glyphs, the pinned
/// `DropIncompatiblePaths` stops at the first glyph whose masters differ in path count, so the
/// glyphs after it would keep their incompatible paths. With three or more masters it can also
/// queue one path twice and then remove a compatible path; a glyph that still does not
/// interpolate after the filter loses all of its paths.
fn drop_incompatible_paths(font: &mut babelfont::Font) -> Result<Vec<String>, String> {
    let names: Vec<String> = font
        .glyphs
        .iter()
        .filter(|glyph| !has_compatible_paths(glyph))
        .map(|glyph| glyph.name.to_string())
        .collect();
    for name in &names {
        DropIncompatiblePaths::new(vec![name.clone()])
            .apply(font)
            .map_err(|error| error.to_string())?;
        if let Some(glyph) = font
            .glyphs
            .iter_mut()
            .find(|glyph| glyph.name == name.as_str())
            && !has_compatible_paths(glyph)
        {
            for layer in &mut glyph.layers {
                layer
                    .shapes
                    .retain(|shape| !matches!(shape, babelfont::Shape::Path(_)));
            }
        }
    }
    Ok(names)
}

/// Remove anchors that have no value at the default master, and return each removal.
fn drop_anchors_without_default(font: &mut babelfont::Font) -> Vec<(String, String)> {
    let Some(default_master) = font.default_master().map(|master| master.id.clone()) else {
        return Vec::new();
    };
    let mut dropped = Vec::new();
    for glyph in font.glyphs.iter_mut() {
        let Some(default_layer) = glyph.layers.iter().find(|layer| {
            !layer.is_background
                && layer.location.is_none()
                && layer.smart_component_location.is_empty()
                && matches!(&layer.master, LayerType::DefaultForMaster(id) if *id == default_master)
        }) else {
            // fontc reports a glyph without a default layer by itself; that is not an anchor
            // failure.
            continue;
        };
        let at_default: BTreeSet<&str> = default_layer
            .anchors
            .iter()
            .map(|anchor| anchor.name.as_str())
            .collect();
        let missing: BTreeSet<String> = glyph
            .layers
            .iter()
            .filter(|layer| interpolates(layer))
            .flat_map(|layer| &layer.anchors)
            .filter(|anchor| !at_default.contains(anchor.name.as_str()))
            .map(|anchor| anchor.name.clone())
            .collect();
        if missing.is_empty() {
            continue;
        }
        for layer in &mut glyph.layers {
            layer
                .anchors
                .retain(|anchor| !missing.contains(&anchor.name));
        }
        let glyph_name = glyph.name.to_string();
        dropped.extend(
            missing
                .into_iter()
                .map(|anchor| (glyph_name.clone(), anchor)),
        );
    }
    dropped
}

/// Change a complete compiler snapshot into the preview snapshot, and list the changes in glyph
/// name order.
pub(super) fn make_tolerant(font: &mut babelfont::Font) -> Result<Vec<PreviewException>, String> {
    let mut exceptions: Vec<PreviewException> = drop_incompatible_paths(font)?
        .into_iter()
        .map(|glyph| PreviewException {
            glyph,
            change: PreviewChange::PathsDropped,
        })
        .collect();
    exceptions.extend(
        drop_anchors_without_default(font)
            .into_iter()
            .map(|(glyph, anchor)| PreviewException {
                glyph,
                change: PreviewChange::AnchorDropped(anchor),
            }),
    );
    exceptions.sort();
    Ok(exceptions)
}

#[cfg(test)]
mod tests {
    use super::{PreviewChange, PreviewException};
    use crate::font::project::Project;
    use crate::testing::fonts;

    const INCOMPATIBLE_GLYPH: &str = "noon-ar";
    const PARTIAL_ANCHOR_GLYPH: &str = "dal-ar";

    fn load() -> Project {
        Project::load(&fonts::arabic_designspace()).expect("the Arabic fixture loads")
    }

    #[test]
    fn the_preview_compiles_unfinished_glyphs_and_export_does_not() {
        let project = load();
        let error = project
            .compile()
            .expect_err("export refuses the unfinished glyphs");
        assert!(error.contains(PARTIAL_ANCHOR_GLYPH), "{error}");
        assert!(error.contains("no value at default location"), "{error}");
        assert!(project.preview_exceptions().is_empty());

        let working = project
            .compiled_preview()
            .expect("the preview compiles the unfinished glyphs");
        let reading = crate::font::compiler::proof::capture(&project)
            .unwrap()
            .compile_preview()
            .expect("a captured reading revision uses the same preview policy");
        assert_eq!(working.glyph_order, reading.glyph_order);
        assert_eq!(
            working.advances(&[]).unwrap(),
            reading.advances(&[]).unwrap()
        );
        let exceptions = project.preview_exceptions();
        assert_eq!(
            *exceptions,
            [
                PreviewException {
                    glyph: PARTIAL_ANCHOR_GLYPH.into(),
                    change: PreviewChange::AnchorDropped("top".into()),
                },
                PreviewException {
                    glyph: INCOMPATIBLE_GLYPH.into(),
                    change: PreviewChange::PathsDropped,
                },
            ]
        );
        assert_eq!(exceptions[0].to_string(), "dal-ar: anchor top dropped");
        assert_eq!(exceptions[1].to_string(), "noon-ar: paths dropped");

        // Export still names the precise failure after a preview.
        assert!(project.compile().is_err());
    }

    #[test]
    fn the_preview_keeps_the_outlines_of_finished_glyphs() {
        let project = load();
        let preview = project.compiled_preview().expect("the preview compiles");
        let mut finished = load();
        for glyph in [INCOMPATIBLE_GLYPH, PARTIAL_ANCHOR_GLYPH] {
            finished
                .remove_document_glyph(glyph)
                .expect("the unfinished glyph is removable");
        }
        let reference = finished.compile().expect("the finished glyphs compile");
        assert!(finished.preview_exceptions().is_empty());
        let mut compared = 0;
        for name in &reference.glyph_order {
            let midpoint = [0.5];
            assert_eq!(
                preview.outline(name, &midpoint).unwrap(),
                reference.outline(name, &midpoint).unwrap(),
                "{name}"
            );
            compared += 1;
        }
        assert!(compared > 40);
        // The glyph loses its first contour, which does not interpolate, and keeps the second.
        let noon = preview
            .outline(INCOMPATIBLE_GLYPH, &[0.5])
            .unwrap()
            .expect("the glyph stays in the preview");
        let contours = noon
            .elements()
            .iter()
            .filter(|element| matches!(element, kurbo::PathEl::MoveTo(_)))
            .count();
        assert_eq!(contours, 1);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_background_preview_keeps_its_exceptions() {
        let project = load();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let compiled = loop {
            if let Some(compiled) = project.request_preview().expect("the preview compiles") {
                break compiled;
            }
            assert!(std::time::Instant::now() < deadline, "the preview arrives");
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert!(compiled.glyph_ids.contains_key(INCOMPATIBLE_GLYPH));
        let glyphs: Vec<String> = project
            .preview_exceptions()
            .iter()
            .map(|exception| exception.glyph.clone())
            .collect();
        assert_eq!(glyphs, [PARTIAL_ANCHOR_GLYPH, INCOMPATIBLE_GLYPH]);
    }

    /// The pinned filter stops at the first glyph whose masters differ in path count; the
    /// preview applies it to each glyph, so the glyphs after that one are handled too.
    #[test]
    fn every_glyph_with_a_different_path_count_is_handled() {
        let mut project = load();
        let bold = project
            .document_sources()
            .nth(1)
            .expect("a second source")
            .default_layer();
        for glyph in ["alef-ar", "reh-ar"] {
            project
                .edit_document_layer(glyph, &bold, |draft| {
                    draft.clear_contours();
                    Ok(())
                })
                .expect("clear the contours");
        }
        project.compiled_preview().expect("the preview compiles");
        let exceptions = project.preview_exceptions();
        let glyphs: Vec<&str> = exceptions
            .iter()
            .filter(|exception| exception.change == PreviewChange::PathsDropped)
            .map(|exception| exception.glyph.as_str())
            .collect();
        assert_eq!(glyphs, ["alef-ar", INCOMPATIBLE_GLYPH, "reh-ar"]);
    }

    #[test]
    fn an_anchor_missing_only_outside_the_default_master_is_kept() {
        let mut project = load();
        project
            .remove_document_glyph(INCOMPATIBLE_GLYPH)
            .expect("the glyph is removable");
        // Give Regular, the default, the anchor that only Bold had: the glyph now compiles.
        let regular = project
            .document_sources()
            .next()
            .expect("a default source")
            .default_layer();
        let bold = project
            .document_sources()
            .nth(1)
            .expect("a second source")
            .default_layer();
        project
            .edit_document_layer(PARTIAL_ANCHOR_GLYPH, &regular, |draft| {
                draft.add_anchor("top".into(), kurbo::Point::new(100.0, 700.0))?;
                Ok(())
            })
            .expect("add the anchor");
        // Remove another anchor from Bold only: fontc compiles that.
        let bottom = project
            .document_layer("beh-ar", &bold)
            .expect("beh in Bold")
            .anchors()
            .find(|anchor| anchor.name() == "bottom")
            .expect("beh has a bottom anchor")
            .id();
        project
            .edit_document_layer("beh-ar", &bold, |draft| {
                draft.remove_anchor(bottom)?;
                Ok(())
            })
            .expect("remove the anchor");
        let export = project.compile().expect("export compiles");
        let preview = project.compiled_preview().expect("the preview compiles");
        assert!(project.preview_exceptions().is_empty());
        assert_eq!(preview.bytes, export.bytes);
    }
}
