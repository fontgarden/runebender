// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Positioning lookups generated from live anchors, for the layout tables of the text line.
//!
//! `fea-rs` compiles the feature file and asks a [`FeatureProvider`] for the lookups that a
//! font compiler writes from the sources: mark attachment today, kerning and cursive attachment
//! later.
//! `fea-rs` puts them at the `# Automatic Code` comment of each feature, so their order among
//! the hand-written lookups is the order of the exported font.
//! The values are those of one designspace location; no other master and no outline is read.
//!
//! This is the first part of the layout engine in `docs/ARABIC-TEXT-LAYOUT-PLAN.md`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use fea_rs::compile::{FeatureBuilder, FeatureProvider, PendingLookup};
use write_fonts::{
    tables::{
        gpos::builders::{AnchorBuilder, MarkToBaseBuilder, MarkToMarkBuilder},
        layout::LookupFlag,
    },
    types::{GlyphId16, Tag},
};

use crate::outline::glyph_paths::round_units;

/// One anchor of a glyph at the displayed location: name, x, y.
pub type AnchorValue = (String, f64, f64);

/// The generated lookups of one font state.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GeneratedLookups {
    /// For each anchor class, in name order: the marks, the bases and the marks that take marks.
    classes: Vec<MarkClass>,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct MarkClass {
    name: String,
    /// Glyphs that attach by `_name`.
    marks: Vec<(GlyphId16, i16, i16)>,
    /// Glyphs that are not marks and offer `name`.
    bases: Vec<(GlyphId16, i16, i16)>,
    /// Marks that offer `name` to another mark.
    mark_bases: Vec<(GlyphId16, i16, i16)>,
}

fn units(value: f64) -> i16 {
    i16::try_from(round_units(value)).unwrap_or(if value < 0.0 { i16::MIN } else { i16::MAX })
}

impl GeneratedLookups {
    /// Group the anchors of every glyph into mark classes.
    ///
    /// `glyph_order` gives the glyph ids; `anchors` gives the effective anchors of a glyph by
    /// name. A glyph with an anchor whose name starts with `_` is a mark.
    pub fn from_anchors(
        glyph_order: &[String],
        anchors: &HashMap<String, Vec<AnchorValue>>,
    ) -> Self {
        let mut classes: BTreeMap<&str, MarkClass> = BTreeMap::new();
        let mut entries: Vec<(GlyphId16, bool, &Vec<AnchorValue>)> = Vec::new();
        for (index, name) in glyph_order.iter().enumerate() {
            let (Ok(id), Some(list)) = (u16::try_from(index), anchors.get(name)) else {
                continue;
            };
            let is_mark = list.iter().any(|(anchor, _, _)| anchor.starts_with('_'));
            entries.push((GlyphId16::new(id), is_mark, list));
        }
        for (id, is_mark, list) in entries {
            // A mark attaches by the first `_name` anchor it has, as in fontc.
            let mut attached: BTreeSet<&str> = BTreeSet::new();
            for (anchor, x, y) in list {
                let point = (id, units(*x), units(*y));
                if let Some(base) = anchor.strip_prefix('_') {
                    if attached.insert(base) {
                        let entry = classes.entry(base).or_default();
                        entry.name = base.to_string();
                        entry.marks.push(point);
                    }
                } else {
                    let entry = classes.entry(anchor.as_str()).or_default();
                    entry.name = anchor.clone();
                    if is_mark {
                        entry.mark_bases.push(point);
                    } else {
                        entry.bases.push(point);
                    }
                }
            }
        }
        Self {
            classes: classes
                .into_values()
                .filter(|class| !class.marks.is_empty())
                .collect(),
        }
    }

    /// Whether no lookup would be written.
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }
}

impl FeatureProvider for GeneratedLookups {
    fn add_features(&self, builder: &mut FeatureBuilder<'_>) {
        let mut mark = Vec::new();
        let mut mkmk = Vec::new();
        for class in &self.classes {
            if !class.bases.is_empty() {
                let mut lookup = MarkToBaseBuilder::default();
                for (glyph, x, y) in &class.marks {
                    // A glyph is in one mark class for each lookup, so this cannot fail.
                    let _ = lookup.insert_mark(*glyph, &class.name, AnchorBuilder::new(*x, *y));
                }
                for (glyph, x, y) in &class.bases {
                    lookup.insert_base(*glyph, &class.name, AnchorBuilder::new(*x, *y));
                }
                mark.push(builder.add_lookup(PendingLookup::new(
                    vec![lookup],
                    LookupFlag::empty(),
                    None,
                )));
            }
            if !class.mark_bases.is_empty() {
                let mut lookup = MarkToMarkBuilder::default();
                for (glyph, x, y) in &class.marks {
                    let _ = lookup.insert_mark1(*glyph, &class.name, AnchorBuilder::new(*x, *y));
                }
                for (glyph, x, y) in &class.mark_bases {
                    lookup.insert_mark2(*glyph, &class.name, AnchorBuilder::new(*x, *y));
                }
                // A stack attaches through its own class: other marks are not seen.
                let filter = class
                    .marks
                    .iter()
                    .chain(&class.mark_bases)
                    .map(|(glyph, _, _)| *glyph)
                    .collect();
                mkmk.push(builder.add_lookup(PendingLookup::new(
                    vec![lookup],
                    LookupFlag::USE_MARK_FILTERING_SET,
                    Some(filter),
                )));
            }
        }
        if !mark.is_empty() {
            builder.add_to_default_language_systems(Tag::new(b"mark"), &mark);
        }
        if !mkmk.is_empty() {
            builder.add_to_default_language_systems(Tag::new(b"mkmk"), &mkmk);
        }
    }
}

#[cfg(all(test, feature = "integration-fonts"))]
mod tests {
    use super::*;
    use crate::font::project::Project;
    use crate::text::shape::{ShapedGlyph, ShapingFont, ShapingGlyph, ShapingSource};

    /// Shape with kerning off: kerning lookups are not generated yet.
    fn rows(font: &ShapingFont, text: &str, rtl: bool) -> Vec<(String, f64, f64, f64)> {
        let row = |glyph: &ShapedGlyph| {
            (
                font.glyph_name(glyph.glyph_id).unwrap_or("?").to_string(),
                glyph.x_advance,
                glyph.x_offset,
                glyph.y_offset,
            )
        };
        font.shape_with_options(text, rtl, &[("kern".into(), false)], None, None)
            .unwrap()
            .iter()
            .map(row)
            .collect()
    }

    #[test]
    fn marks_from_live_anchors_match_the_fontc_compile() {
        let project = Project::load(&crate::testing::fonts::virtua_designspace()).unwrap();
        let source = project.document_sources().next().unwrap();
        let (source, layer) = (source.id(), source.default_layer());

        let mut order = vec![".notdef".to_string()];
        order.extend(
            project
                .glyph_names()
                .filter(|name| *name != ".notdef")
                .map(str::to_string),
        );
        let mut anchors = HashMap::new();
        let mut glyphs = Vec::new();
        for name in &order {
            let view = project.document_layer(name, &layer);
            glyphs.push(ShapingGlyph {
                name: name.clone(),
                advance: view.map_or(500.0, |layer| layer.width()),
                unicodes: view
                    .map(|layer| layer.codepoints().map(u32::from).collect())
                    .unwrap_or_default(),
            });
            // A composite without anchors takes those of its components, in the editor and
            // in the compiler snapshot.
            let list =
                crate::text::features::anchors_project(&project, source, name).unwrap_or_default();
            if !list.is_empty() {
                anchors.insert(name.clone(), list);
            }
        }
        let generated = GeneratedLookups::from_anchors(&order, &anchors);
        assert!(!generated.is_empty());
        let features: String = project
            .document_feature_text(source)
            .unwrap_or_default()
            .lines()
            .filter(|line| line.trim() != crate::text::features::INCLUDE_LINE)
            .map(|line| format!("{line}\n"))
            .collect();
        // The feature file has `feature mark { # Automatic Code ... }` with manual lookups
        // after the comment, so the order of the generated lookups is under test too.
        assert!(features.contains("# Automatic Code"));
        let live = ShapingFont::build_with(
            &ShapingSource {
                units_per_em: 1000.0,
                glyphs,
                features,
            },
            &generated,
        )
        .unwrap();
        let compiled = project.compile().unwrap();
        let fontc = ShapingFont::from_bytes((*compiled.bytes).clone()).unwrap();

        for (text, rtl) in [
            ("بِسْمِ ٱللَّهِ ٱلرَّحْمَٰنِ", true),
            ("فيرتوا غروتيسك لا لله", true),
            ("בְּרֵאשִׁית בָּרָא אֱלֹהִים", true),
            ("AVATAR Tölpel ǻ ế", false),
        ] {
            assert_eq!(rows(&live, text, rtl), rows(&fontc, text, rtl), "{text}");
        }
        // `beh-ar.init` is a composite with no anchors of its own; the kasra attaches by the
        // anchor of its component, in the live tables and in the compiled font.
        let kasra = rows(&fontc, "بِس", true)
            .into_iter()
            .find(|row| row.0 == "kasra-ar")
            .expect("a kasra");
        assert!(kasra.2 != 0.0 || kasra.3 != 0.0, "{kasra:?}");
    }
}
