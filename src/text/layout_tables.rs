// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Layout tables for the text line, built with the feature writers of fontc.
//!
//! This module runs the kerning and mark writers of `fontbe`, the back end of fontc, on the
//! values of one designspace location, and gives their lookups to `fea-rs` at the
//! `# Automatic Code` comments of the feature file.
//! The writers are the ones the export uses, so kerning, mark-to-base, mark-to-ligature,
//! mark-to-mark, cursive attachment, `abvm`, `blwm` and `dist` follow the same rules as the
//! exported font: script and direction splits, mark filtering sets, lookup flags, and the
//! policy that a feature block without a marker gets no generated lookups.
//!
//! The values are written at one "default" location of a font with no axes.
//! No outline, no variation data and no other master is read, so a glyph that does not
//! interpolate cannot stop the build.
//! A composite without anchors takes the anchors of its components, as the compiler snapshot
//! gives them to fontc, so the editor and the export attach marks in the same way.
//!
//! [`LayoutTables`] keeps three results between builds, each keyed on the inputs it reads:
//! the first pass over the feature file (parse, validation and GSUB), the mark lookups and the
//! kerning lookups.
//! An anchor edit therefore runs the mark writer again and not the kerning writer, and a
//! kerning edit the reverse.
//!
//! [`LayoutInputs::from_project`] reads the values of one master, and
//! [`LayoutInputs::at_location`] interpolates them at any designspace location, each glyph and
//! each anchor alone, with the variation model of the outlines.
//! A value that cannot be interpolated comes from the nearest master and is reported as a
//! [`LayoutDiagnostic`]; only feature code that does not compile stops a build.
//!
//! [`LayoutTables::build_live`] is the one function that the editor shapes its text line with.
//! A drag step calls it with the committed inputs and [`LayoutOverrides`], the values of the
//! gesture that are not committed yet; the commit calls it with the committed inputs alone.
//! Identical values give identical inputs, so the line cannot move at the moment of commit
//! (rule 4 of section 5.2 in `docs/ARABIC-TEXT-LAYOUT-PLAN.md`).
//! When the feature text does not compile, `build_live` builds from the last feature text that
//! compiled, with the current values, and reports the error with its line (rule 7).
//!
//! See items 0.1, 2.1 to 2.4 and 3.4 to 3.7 in `docs/ARABIC-TEXT-LAYOUT-PLAN.md`.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fea_rs::{
    DiagnosticSet, GlyphMap, Opts,
    compile::{FeatureBuilder, FeatureProvider, NopFeatureProvider},
    parse::SourceLoadError,
};
use fontbe::orchestration::{
    AnyWorkId, Context as BeContext, FeaFirstPassOutput, FeaRsKerns, FeaRsMarks,
};
use fontdrasil::{coords::NormalizedLocation, orchestration::Access, types::GlyphName};
use fontir::{
    ir::{
        AnchorBuilder, GdefCategories, Glyph, GlyphInstance, GlyphOrder, KernGroup, KernSide,
        KerningInstance, KerningLocations, StaticMetadata,
    },
    orchestration::{Context as IrContext, Flags},
};
use write_fonts::tables::{gdef::GlyphClassDef, layout::ClassDef};

use crate::font::LayerView;
use crate::font::model::designspace::CanonicalDesignspace;
use crate::font::project::Project;
use crate::font::var_model::{Location, VariationModel, locations_match, squared_distance};
use crate::font::variable::{GlyphSource, LayerId, SourceId};
use crate::text::layout_variation::LocationVariationInfo;
use crate::text::shape::{ShapingFont, ShapingGlyph};

/// The GDEF glyph class of a glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutCategory {
    /// A spacing glyph that marks attach to.
    Base,
    /// A ligature, whose marks attach to numbered anchors.
    Ligature,
    /// A combining mark.
    Mark,
    /// A part of a glyph that is not shown alone.
    Component,
}

impl LayoutCategory {
    fn class_def(self) -> GlyphClassDef {
        match self {
            Self::Base => GlyphClassDef::Base,
            Self::Ligature => GlyphClassDef::Ligature,
            Self::Mark => GlyphClassDef::Mark,
            Self::Component => GlyphClassDef::Component,
        }
    }
}

/// One glyph at the location of the layout.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutGlyph {
    /// The glyph name.
    pub name: String,
    /// Code points that map to the glyph.
    pub codepoints: Vec<u32>,
    /// Advance width in font units.
    pub advance: f64,
    /// The anchors the glyph offers, its own or those of its components: name, x, y.
    pub anchors: Vec<(String, f64, f64)>,
    /// The GDEF class, when the source gives or implies one.
    pub category: Option<LayoutCategory>,
}

/// Everything the layout tables are built from, at one designspace location.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutInputs {
    /// Units per em.
    pub units_per_em: f64,
    /// The glyph order. The index is the glyph id, so `.notdef` belongs first.
    pub glyphs: Vec<LayoutGlyph>,
    /// Kerning groups by UFO name (`public.kern1.*` and `public.kern2.*`).
    pub groups: BTreeMap<String, Vec<String>>,
    /// Kerning pairs by UFO glyph or group name, in logical order, in font units.
    pub kerning: Vec<(String, String, f64)>,
    /// The feature file, without `include` statements.
    pub features: String,
    /// The axes of the font, for variable values in the feature file such as
    /// `<anchor (wght=400:360 wght=700:400) 384>`; empty for a static font.
    pub axes: Vec<fontdrasil::types::Axis>,
    /// The normalized location of these values, at which variable feature values are read.
    pub location: NormalizedLocation,
}

/// The normalized location, by axis tag, of a location by axis name.
fn tagged_location(project: &Project, location: &Location) -> Result<NormalizedLocation, String> {
    let mut tagged = NormalizedLocation::new();
    for axis in &project.axes {
        let tag = axis.user.fontdrasil_axis()?.tag;
        let value = location.get(&axis.name).copied().unwrap_or(0.0);
        tagged.insert(tag, fontdrasil::coords::NormalizedCoord::new(value));
    }
    Ok(tagged)
}

impl LayoutInputs {
    /// Read the layout values of one source of `project`, as the fontc export reads them.
    ///
    /// Glyphs that the source does not export are left out, as fontc leaves them out.
    /// Kerning values are rounded to integers, as the compiler snapshot rounds them.
    pub fn from_project(project: &Project, source: SourceId) -> Result<Self, String> {
        let view = project
            .document_sources()
            .find(|view| view.id() == source)
            .ok_or("the source is not loaded")?;
        let source_layer = view.default_layer();
        let info = project
            .document_font_info(source)
            .ok_or("the source has no font info")?;
        let metadata = project
            .document_font_metadata(source)
            .ok_or("the source has no kerning metadata")?;

        let mut names = vec![".notdef"];
        names.extend(project.glyph_names().filter(|name| *name != ".notdef"));
        let mut glyphs = Vec::with_capacity(names.len());
        for name in names {
            let glyph_metadata = project.document_source_glyph_metadata(source, name);
            let Some(layer) = project.document_layer(name, &source_layer) else {
                if name == ".notdef" {
                    glyphs.push(LayoutGlyph {
                        name: name.to_string(),
                        codepoints: Vec::new(),
                        advance: 500.0,
                        anchors: Vec::new(),
                        category: Some(LayoutCategory::Base),
                    });
                }
                continue;
            };
            if glyph_metadata.is_some_and(|metadata| !metadata.exported()) {
                continue;
            }
            let category = crate::font::compiler::glyph_category_from_values(
                name,
                glyph_metadata.and_then(|metadata| metadata.category()),
                layer.codepoints(),
                layer.anchors().map(|anchor| anchor.name()),
            )?;
            // The Babelfont adapter of fontc maps every other category to Base.
            let category = match category {
                babelfont::GlyphCategory::Mark => LayoutCategory::Mark,
                babelfont::GlyphCategory::Ligature => LayoutCategory::Ligature,
                _ => LayoutCategory::Base,
            };
            glyphs.push(LayoutGlyph {
                name: name.to_string(),
                codepoints: layer.codepoints().map(u32::from).collect(),
                advance: layer.width(),
                // A composite without anchors takes those of its components, as the
                // compiler snapshot gives them to fontc.
                anchors: crate::font::composites::effective_document_anchors(layer, |name| {
                    project.document_layer(name, &source_layer)
                })
                .into_iter()
                .map(|(name, at)| (name, at.x, at.y))
                .collect(),
                category: Some(category),
            });
        }
        let features = project
            .document_feature_text(source)
            .unwrap_or_default()
            .lines()
            .filter(|line| line.trim() != crate::text::features::INCLUDE_LINE)
            .collect::<Vec<_>>()
            .join("\n");
        let axes = project
            .axes
            .iter()
            .map(|axis| axis.user.fontdrasil_axis())
            .collect::<Result<Vec<_>, _>>()?;
        let location = match project
            .document_sources()
            .position(|view| view.id() == source)
            .and_then(|index| project.master_locations().get(index))
        {
            Some(master) => tagged_location(project, master)?,
            None => NormalizedLocation::new(),
        };
        Ok(Self {
            axes,
            location,
            units_per_em: info.metrics.units_per_em.unwrap_or(1000.0),
            glyphs,
            groups: metadata
                .groups()
                .iter()
                .filter(|(name, _)| name.starts_with("public.kern"))
                .map(|(name, members)| (name.clone(), members.clone()))
                .collect(),
            kerning: metadata
                .kerning_pairs()
                .map(|(left, right, value)| {
                    (
                        left.as_raw_name().to_owned(),
                        right.as_raw_name().to_owned(),
                        value.round(),
                    )
                })
                .collect(),
            features,
        })
    }

    /// Read the layout values of `project` at a normalized designspace location.
    ///
    /// The glyph set, code points, categories, kerning groups and feature text are those of
    /// `source`, as [`Self::from_project`] reads them; the caller passes the active source.
    /// Advances, anchors and kerning values are interpolated with the variation model of the
    /// outlines, each glyph alone and each anchor by name alone, so one unfinished glyph
    /// cannot change another.
    /// A value that cannot be interpolated, because a master lacks the anchor, has it twice,
    /// or the glyph has no layer at the default location, takes the value of the master of
    /// that glyph nearest to `location`, and a [`LayoutDiagnostic`] names it.
    /// Contour compatibility is not read, so a glyph whose contours do not interpolate keeps
    /// interpolated layout values.
    /// A glyph with a layer exactly at `location` takes the values of that layer, so at the
    /// location of `source` the result equals [`Self::from_project`].
    /// Kerning values are rounded at each master and after interpolation, as the compiled
    /// font rounds them.
    pub fn at_location(
        project: &Project,
        location: &Location,
        source: SourceId,
    ) -> Result<(Self, Vec<LayoutDiagnostic>), String> {
        let mut inputs = Self::from_project(project, source)?;
        inputs.location = tagged_location(project, location)?;
        let mut diagnostics = Vec::new();
        let Some(designspace) = project.document_designspace() else {
            return Ok((inputs, diagnostics));
        };
        let mut labels: HashMap<LayerId, String> = HashMap::new();
        for descriptor in designspace.sources() {
            labels.insert(
                descriptor.default_layer.clone(),
                descriptor.display_name().to_owned(),
            );
        }
        for sparse in designspace.sparse_sources() {
            labels.insert(sparse.layer.clone(), sparse.layer.name.clone());
        }
        let label = |layer: &LayerId| {
            labels
                .get(layer)
                .cloned()
                .unwrap_or_else(|| layer.name.clone())
        };

        let mut weights = WeightCache::new(location);
        for glyph in &mut inputs.glyphs {
            let Ok(sources) = project.glyph_sources(&glyph.name) else {
                // `.notdef` without a layer keeps the value `from_project` made for it.
                continue;
            };
            let layers: Vec<(&GlyphSource, LayerView<'_>)> = sources
                .iter()
                .filter_map(|glyph_source| {
                    Some((
                        glyph_source,
                        project.document_layer(&glyph.name, &glyph_source.layer)?,
                    ))
                })
                .collect();
            if layers.is_empty() {
                continue;
            }
            let anchors_of = |layer: LayerView<'_>, at: &LayerId| {
                crate::font::composites::effective_document_anchors(layer, |name| {
                    project.document_layer(name, at)
                })
            };
            if let Some((exact, layer)) = layers
                .iter()
                .find(|(glyph_source, _)| locations_match(&glyph_source.location, location))
            {
                glyph.advance = layer.width();
                glyph.anchors = anchors_of(*layer, &exact.layer)
                    .into_iter()
                    .map(|(name, at)| (name, at.x, at.y))
                    .collect();
                continue;
            }
            let nearest = nearest_index(&layers, location, source);
            let nearest_label = label(&layers[nearest].0.layer);
            let glyph_weights = weights.get(layers.iter().map(|(glyph_source, _)| *glyph_source));
            let glyph_weights = match glyph_weights {
                Ok(found) => Some(found),
                Err(error) => {
                    diagnostics.push(LayoutDiagnostic {
                        glyph: glyph.name.clone(),
                        anchor: None,
                        reason: format!("{error}; the values of {nearest_label} are used"),
                    });
                    None
                }
            };
            let weighted = |values: &[f64]| -> f64 {
                glyph_weights
                    .as_ref()
                    .expect("weights are checked first")
                    .iter()
                    .zip(values)
                    .map(|(weight, value)| weight * value)
                    .sum()
            };

            let widths: Vec<f64> = layers.iter().map(|(_, layer)| layer.width()).collect();
            glyph.advance = match glyph_weights {
                Some(_) => weighted(&widths),
                None => widths[nearest],
            };

            let per_layer: Vec<Vec<(String, kurbo::Point)>> = layers
                .iter()
                .map(|(glyph_source, layer)| anchors_of(*layer, &glyph_source.layer))
                .collect();
            let mut names: Vec<&str> = Vec::new();
            for anchors in std::iter::once(&per_layer[nearest]).chain(&per_layer) {
                for (name, _) in anchors {
                    if !names.contains(&name.as_str()) {
                        names.push(name);
                    }
                }
            }
            let mut anchors = Vec::with_capacity(names.len());
            for name in names {
                let found: Vec<Vec<kurbo::Point>> = per_layer
                    .iter()
                    .map(|anchors| {
                        anchors
                            .iter()
                            .filter(|(candidate, _)| candidate == name)
                            .map(|(_, at)| *at)
                            .collect()
                    })
                    .collect();
                if glyph_weights.is_some() && found.iter().all(|points| points.len() == 1) {
                    let xs: Vec<f64> = found.iter().map(|points| points[0].x).collect();
                    let ys: Vec<f64> = found.iter().map(|points| points[0].y).collect();
                    anchors.push((name.to_owned(), weighted(&xs), weighted(&ys)));
                    continue;
                }
                if glyph_weights.is_some() {
                    let problem = found
                        .iter()
                        .zip(&layers)
                        .find(|(points, _)| points.len() != 1)
                        .map(|(points, (glyph_source, _))| {
                            let master = label(&glyph_source.layer);
                            if points.is_empty() {
                                format!("missing in {master}")
                            } else {
                                format!("more than once in {master}")
                            }
                        })
                        .unwrap_or_default();
                    let used = if found[nearest].is_empty() {
                        format!("{nearest_label} has none, so the glyph has none here")
                    } else {
                        format!("the value of {nearest_label} is used")
                    };
                    diagnostics.push(LayoutDiagnostic {
                        glyph: glyph.name.clone(),
                        anchor: Some(name.to_owned()),
                        reason: format!("the anchor is {problem}; {used}"),
                    });
                }
                for at in &found[nearest] {
                    anchors.push((name.to_owned(), at.x, at.y));
                }
            }
            glyph.anchors = anchors;
        }

        match kerning_at(project, designspace, location, source, &inputs) {
            Ok(kerning) => inputs.kerning = kerning,
            Err(error) => diagnostics.push(LayoutDiagnostic {
                glyph: String::new(),
                anchor: None,
                reason: format!("kerning: {error}; the kerning of the active source is used"),
            }),
        }
        Ok((inputs, diagnostics))
    }

    /// Keep only the glyphs for which `keep` is true, with the kerning and group members that
    /// refer to them.
    ///
    /// The feature text is kept as it is, so it must not name a removed glyph.
    /// `.notdef` is always kept.
    pub fn retain_glyphs(&mut self, keep: impl Fn(&LayoutGlyph) -> bool) {
        self.glyphs
            .retain(|glyph| glyph.name == ".notdef" || keep(glyph));
        let names: HashSet<String> = self.glyphs.iter().map(|g| g.name.clone()).collect();
        for members in self.groups.values_mut() {
            members.retain(|member| names.contains(member));
        }
        self.groups.retain(|_, members| !members.is_empty());
        let groups = &self.groups;
        self.kerning.retain(|(left, right, _)| {
            [left, right]
                .iter()
                .all(|side| names.contains(*side) || groups.contains_key(*side))
        });
    }
}

/// A layout value between masters that came from one master instead of the interpolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutDiagnostic {
    /// The glyph, or an empty string for a value of the whole font such as kerning.
    pub glyph: String,
    /// The anchor, when only one anchor of the glyph is affected.
    pub anchor: Option<String>,
    /// Why the value could not be interpolated, and which master supplied it.
    pub reason: String,
}

impl std::fmt::Display for LayoutDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.anchor {
            Some(anchor) => write!(f, "{} anchor {}: {}", self.glyph, anchor, self.reason),
            None if self.glyph.is_empty() => write!(f, "{}", self.reason),
            None => write!(f, "{}: {}", self.glyph, self.reason),
        }
    }
}

/// The layer nearest to `location`; on a tie, a layer of `preferred`, then the first.
fn nearest_index(
    layers: &[(&GlyphSource, LayerView<'_>)],
    location: &Location,
    preferred: SourceId,
) -> usize {
    let mut best = 0;
    let mut best_key = (f64::INFINITY, true);
    for (index, (glyph_source, _)) in layers.iter().enumerate() {
        let key = (
            squared_distance(&glyph_source.location, location),
            glyph_source.layer.source != preferred,
        );
        let closer = key.0 < best_key.0 - 1e-12;
        let tie_won = (key.0 - best_key.0).abs() <= 1e-12 && !key.1 && best_key.1;
        if closer || tie_won {
            best = index;
            best_key = key;
        }
    }
    best
}

/// The weights of each set of glyph layers at one location, computed once per set.
struct WeightCache<'a> {
    target: &'a Location,
    found: HashMap<Vec<LayerId>, Result<Vec<f64>, String>>,
}

impl<'a> WeightCache<'a> {
    fn new(target: &'a Location) -> Self {
        Self {
            target,
            found: HashMap::new(),
        }
    }

    fn get<'s>(
        &mut self,
        sources: impl Iterator<Item = &'s GlyphSource>,
    ) -> Result<Vec<f64>, String> {
        let sources: Vec<&GlyphSource> = sources.collect();
        let key: Vec<LayerId> = sources.iter().map(|source| source.layer.clone()).collect();
        let target = self.target;
        self.found
            .entry(key)
            .or_insert_with(|| {
                let locations: Vec<Location> = sources
                    .iter()
                    .map(|source| source.location.clone())
                    .collect();
                VariationModel::new(&locations)?.weights(target)
            })
            .clone()
    }
}

/// The kerning of one master, read with the UFO kerning value lookup.
#[derive(Debug, Clone)]
struct MasterKerning {
    pairs: HashMap<(String, String), f64>,
    first: HashMap<String, String>,
    second: HashMap<String, String>,
}

impl MasterKerning {
    fn read(project: &Project, source: SourceId) -> Option<Self> {
        let metadata = project.document_font_metadata(source)?;
        let mut first = HashMap::new();
        let mut second = HashMap::new();
        for (group, members) in metadata.groups() {
            let side = if group.starts_with("public.kern1.") {
                &mut first
            } else if group.starts_with("public.kern2.") {
                &mut second
            } else {
                continue;
            };
            for member in members {
                side.entry(member.clone()).or_insert_with(|| group.clone());
            }
        }
        Some(Self {
            pairs: metadata
                .kerning_pairs()
                .map(|(left, right, value)| {
                    (
                        (
                            left.as_raw_name().to_owned(),
                            right.as_raw_name().to_owned(),
                        ),
                        value.round(),
                    )
                })
                .collect(),
            first,
            second,
        })
    }

    /// The value of a pair of glyph or group names: the pair itself, then the groups of its
    /// glyphs, then zero, as fontc resolves a pair that one master does not define.
    fn value(&self, left: &str, right: &str) -> f64 {
        let get = |left: &str, right: &str| {
            self.pairs
                .get(&(left.to_owned(), right.to_owned()))
                .copied()
        };
        if let Some(value) = get(left, right) {
            return value;
        }
        let is_group = |side: &str| side.starts_with("public.kern");
        let first_group = if is_group(left) {
            Some(left)
        } else {
            self.first.get(left).map(String::as_str)
        };
        let second_group = if is_group(right) {
            Some(right)
        } else {
            self.second.get(right).map(String::as_str)
        };
        let first_glyph = (!is_group(left)).then_some(left);
        let second_glyph = (!is_group(right)).then_some(right);
        [
            (first_glyph, second_group),
            (first_group, second_glyph),
            (first_group, second_group),
        ]
        .into_iter()
        .find_map(|(left, right)| get(left?, right?))
        .unwrap_or(0.0)
    }
}

/// The kerning pairs of every master at `location`.
fn kerning_at(
    project: &Project,
    designspace: &CanonicalDesignspace,
    location: &Location,
    source: SourceId,
    inputs: &LayoutInputs,
) -> Result<Vec<(String, String, f64)>, String> {
    let masters: Vec<(SourceId, Location)> = designspace
        .sources()
        .iter()
        .map(|descriptor| {
            Ok((
                descriptor.id(),
                descriptor.location.to_normalized(designspace.axes())?,
            ))
        })
        .collect::<Result<_, String>>()?;
    let tables: Vec<MasterKerning> = masters
        .iter()
        .map(|(id, _)| MasterKerning::read(project, *id).ok_or("a source has no kerning metadata"))
        .collect::<Result<_, _>>()?;
    if let Some(index) = masters
        .iter()
        .position(|(_, at)| locations_match(at, location))
    {
        if masters[index].0 == source {
            return Ok(inputs.kerning.clone());
        }
        let mut pairs: Vec<_> = tables[index]
            .pairs
            .iter()
            .map(|((left, right), value)| (left.clone(), right.clone(), *value))
            .collect();
        pairs.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        return Ok(pairs);
    }
    let locations: Vec<Location> = masters.iter().map(|(_, at)| at.clone()).collect();
    let weights = VariationModel::new(&locations)?.weights(location)?;
    let names: BTreeSet<&(String, String)> =
        tables.iter().flat_map(|table| table.pairs.keys()).collect();
    Ok(names
        .into_iter()
        .map(|(left, right)| {
            let value: f64 = tables
                .iter()
                .zip(&weights)
                .map(|(table, weight)| weight * table.value(left, right))
                .sum();
            (left.clone(), right.clone(), value.round())
        })
        .collect())
}

/// The layout values that an unfinished edit gives one glyph, in place of its committed ones.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphOverride {
    /// The glyph name.
    pub name: String,
    /// Advance width in font units.
    pub advance: f64,
    /// The glyph's own anchors: name, x, y.
    /// `None` keeps the committed anchors, which a composite can take from its components.
    pub anchors: Option<Vec<(String, f64, f64)>>,
}

/// Values of a gesture in progress that replace committed layout values.
///
/// A gesture stages its edit in a draft that the project receives only at the end of the
/// gesture.
/// The text line applies the draft through these overrides, so it follows the pointer in the
/// same frame and shows the same layout that the commit gives.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayoutOverrides {
    /// Glyph values, by glyph name.
    pub glyphs: Vec<GlyphOverride>,
    /// Kerning pairs by UFO glyph or group name, in logical order, in font units.
    /// A pair that the inputs do not have is added.
    pub kerning: Vec<(String, String, f64)>,
}

impl LayoutOverrides {
    /// Whether no value is replaced.
    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty() && self.kerning.is_empty()
    }
}

impl LayoutInputs {
    /// The overrides that change a value of these inputs.
    ///
    /// Entries equal to the committed values, and glyphs that the inputs do not have, are left
    /// out, so a drag of contour points alone gives empty overrides and builds nothing.
    pub fn changes_in(&self, overrides: &LayoutOverrides) -> LayoutOverrides {
        let glyphs = overrides
            .glyphs
            .iter()
            .filter(|edit| {
                self.glyphs.iter().any(|glyph| {
                    glyph.name == edit.name
                        && (glyph.advance != edit.advance
                            || edit
                                .anchors
                                .as_ref()
                                .is_some_and(|anchors| *anchors != glyph.anchors))
                })
            })
            .cloned()
            .collect();
        let kerning = overrides
            .kerning
            .iter()
            .filter(|(left, right, value)| {
                let value = value.round();
                !self
                    .kerning
                    .iter()
                    .any(|(l, r, v)| l == left && r == right && *v == value)
            })
            .cloned()
            .collect();
        LayoutOverrides { glyphs, kerning }
    }

    /// These inputs with `overrides` in place of the committed values.
    ///
    /// Kerning values are rounded as [`Self::from_project`] rounds them, so an override and
    /// the commit of the same value give equal inputs.
    pub fn with_overrides(&self, overrides: &LayoutOverrides) -> Cow<'_, Self> {
        if overrides.is_empty() {
            return Cow::Borrowed(self);
        }
        let mut inputs = self.clone();
        for edit in &overrides.glyphs {
            if let Some(glyph) = inputs.glyphs.iter_mut().find(|g| g.name == edit.name) {
                glyph.advance = edit.advance;
                if let Some(anchors) = &edit.anchors {
                    glyph.anchors.clone_from(anchors);
                }
            }
        }
        for (left, right, value) in &overrides.kerning {
            let value = value.round();
            match inputs
                .kerning
                .iter_mut()
                .find(|(l, r, _)| l == left && r == right)
            {
                Some(pair) => pair.2 = value,
                None => inputs.kerning.push((left.clone(), right.clone(), value)),
            }
        }
        Cow::Owned(inputs)
    }
}

/// The anchors a composite with no anchors of its own takes from `components`, each base's
/// anchors given by `base_anchors`, by the rule of
/// [`effective_document_anchors`](crate::font::composites::effective_document_anchors).
fn inherited_anchors<'a>(
    components: &[(String, kurbo::Affine)],
    mut base_anchors: impl FnMut(&str) -> Option<&'a [(String, f64, f64)]>,
) -> Vec<(String, f64, f64)> {
    let mut anchors: Vec<(String, f64, f64)> = Vec::new();
    for (base, transform) in components {
        for (name, x, y) in base_anchors(base).unwrap_or_default() {
            if name.starts_with('_') {
                continue;
            }
            let at = *transform * kurbo::Point::new(*x, *y);
            anchors.retain(|(candidate, _, _)| candidate != name);
            anchors.push((name.clone(), at.x, at.y));
        }
    }
    anchors
}

/// What turns the values of a gesture on one master into overrides of the layout inputs at
/// the displayed location.
///
/// A gesture edits one layer of the active master.
/// Composites that take their anchors from the edited glyph take the dragged anchors too, as
/// the commit gives them.
/// Between masters, each glyph value moves by the weight of the master in that glyph's
/// interpolation times the change, which equals the interpolation of the edited master
/// values, and kerning is interpolated again from every master with the dragged pair in
/// place.
#[derive(Debug, Clone, Default)]
pub struct OverrideBasis {
    /// The committed values of the edited master; `None` when they are the located inputs.
    master: Option<Arc<LayoutInputs>>,
    /// The weight of the master in each glyph's values at the location.
    weights: HashMap<String, f64>,
    /// The weight of each master's kerning at the location, its kerning, and whether it is
    /// the edited master.
    kerning: Vec<(f64, MasterKerning, bool)>,
    /// Composites of the edited master without anchors of their own, with their components.
    composites: Vec<(String, Vec<(String, kurbo::Affine)>)>,
    /// The committed anchors of every glyph that a composite uses, exported or not.
    bases: HashMap<String, Vec<(String, f64, f64)>>,
}

impl OverrideBasis {
    /// The basis at the location of the master `source`.
    pub fn at_master(project: &Project, source: SourceId) -> Self {
        let mut basis = Self::default();
        basis.read_composites(project, source);
        basis
    }

    /// The basis at `location`, for a gesture on the master `source`.
    pub fn at_location(
        project: &Project,
        location: &Location,
        source: SourceId,
    ) -> Result<Self, String> {
        let mut basis = Self {
            master: Some(Arc::new(LayoutInputs::from_project(project, source)?)),
            ..Self::default()
        };
        basis.read_composites(project, source);
        let master_layer = project
            .document_source(source)
            .ok_or("the source is not loaded")?
            .default_layer();
        let mut weights = WeightCache::new(location);
        let names: Vec<String> = basis
            .master
            .as_ref()
            .map(|inputs| inputs.glyphs.iter().map(|g| g.name.clone()).collect())
            .unwrap_or_default();
        for name in names {
            let Ok(sources) = project.glyph_sources(&name) else {
                continue;
            };
            let sources: Vec<&GlyphSource> = sources
                .iter()
                .filter(|glyph_source| project.document_layer(&name, &glyph_source.layer).is_some())
                .collect();
            let is_master = |glyph_source: &&GlyphSource| glyph_source.layer == master_layer;
            let weight = if let Some(exact) = sources
                .iter()
                .find(|glyph_source| locations_match(&glyph_source.location, location))
            {
                if is_master(exact) { 1.0 } else { 0.0 }
            } else {
                match weights.get(sources.iter().copied()) {
                    Ok(found) => sources
                        .iter()
                        .zip(found)
                        .find(|(glyph_source, _)| is_master(glyph_source))
                        .map_or(0.0, |(_, weight)| weight),
                    // The nearest master supplies the values.
                    Err(_) => {
                        let nearest = sources
                            .iter()
                            .min_by(|a, b| {
                                squared_distance(&a.location, location)
                                    .total_cmp(&squared_distance(&b.location, location))
                            })
                            .is_some_and(is_master);
                        if nearest { 1.0 } else { 0.0 }
                    }
                }
            };
            basis.weights.insert(name, weight);
        }
        if let Some(designspace) = project.document_designspace() {
            let masters: Vec<(SourceId, Location)> = designspace
                .sources()
                .iter()
                .map(|descriptor| {
                    Ok((
                        descriptor.id(),
                        descriptor.location.to_normalized(designspace.axes())?,
                    ))
                })
                .collect::<Result<_, String>>()?;
            let locations: Vec<Location> = masters.iter().map(|(_, at)| at.clone()).collect();
            let master_weights = match masters
                .iter()
                .position(|(_, at)| locations_match(at, location))
            {
                Some(index) => (0..masters.len())
                    .map(|i| if i == index { 1.0 } else { 0.0 })
                    .collect(),
                None => VariationModel::new(&locations)?.weights(location)?,
            };
            for ((id, _), weight) in masters.iter().zip(master_weights) {
                let table =
                    MasterKerning::read(project, *id).ok_or("a source has no kerning metadata")?;
                basis.kerning.push((weight, table, *id == source));
            }
        }
        Ok(basis)
    }

    fn read_composites(&mut self, project: &Project, source: SourceId) {
        let Some(layer) = project
            .document_source(source)
            .map(|source| source.default_layer())
        else {
            return;
        };
        for name in project.glyph_names() {
            let Some(view) = project.document_layer(name, &layer) else {
                continue;
            };
            if view.anchors().next().is_some() || view.components().next().is_none() {
                continue;
            }
            let components: Vec<(String, kurbo::Affine)> = view
                .components()
                .map(|component| (component.reference().to_owned(), component.transform()))
                .collect();
            for (base, _) in &components {
                if !self.bases.contains_key(base)
                    && let Some(base_view) = project.document_layer(base, &layer)
                {
                    let anchors =
                        crate::font::composites::effective_document_anchors(base_view, |name| {
                            project.document_layer(name, &layer)
                        })
                        .into_iter()
                        .map(|(name, at)| (name, at.x, at.y))
                        .collect();
                    self.bases.insert(base.clone(), anchors);
                }
            }
            self.composites.push((name.to_owned(), components));
        }
    }

    /// `overrides`, values of the edited master, as overrides of `located`, the inputs at the
    /// displayed location, with the composites that inherit dragged anchors added.
    ///
    /// Entries that change nothing are left out; see [`LayoutInputs::changes_in`].
    pub fn locate(&self, located: &LayoutInputs, overrides: &LayoutOverrides) -> LayoutOverrides {
        if overrides.is_empty() {
            return LayoutOverrides::default();
        }
        let master = self.master.as_deref().unwrap_or(located);
        let mut glyphs = overrides.glyphs.clone();
        // Composites follow the anchors of their components, transitively.
        let mut dragged: HashMap<String, Vec<(String, f64, f64)>> = overrides
            .glyphs
            .iter()
            .filter_map(|glyph| Some((glyph.name.clone(), glyph.anchors.clone()?)))
            .collect();
        for _ in 0..8 {
            let mut changed = false;
            for (composite, components) in &self.composites {
                if !components
                    .iter()
                    .any(|(base, _)| dragged.contains_key(base))
                    || overrides
                        .glyphs
                        .iter()
                        .any(|glyph| glyph.name == *composite)
                {
                    continue;
                }
                let anchors = inherited_anchors(components, |base| {
                    dragged
                        .get(base)
                        .or_else(|| self.bases.get(base))
                        .map(Vec::as_slice)
                });
                if dragged.get(composite) != Some(&anchors) {
                    dragged.insert(composite.clone(), anchors);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for (name, anchors) in &dragged {
            if glyphs.iter().any(|glyph| glyph.name == *name) {
                continue;
            }
            if let Some(glyph) = master.glyphs.iter().find(|glyph| glyph.name == *name) {
                glyphs.push(GlyphOverride {
                    name: name.clone(),
                    advance: glyph.advance,
                    anchors: Some(anchors.clone()),
                });
            }
        }
        let mut located_overrides = LayoutOverrides {
            glyphs,
            kerning: overrides.kerning.clone(),
        };
        if self.master.is_some() {
            located_overrides.glyphs = located_overrides
                .glyphs
                .iter()
                .filter_map(|edit| self.located_glyph(master, located, edit))
                .collect();
            located_overrides.kerning = located_overrides
                .kerning
                .iter()
                .map(|(left, right, value)| {
                    let value: f64 = self
                        .kerning
                        .iter()
                        .map(|(weight, table, edited)| {
                            weight
                                * if *edited {
                                    value.round()
                                } else {
                                    table.value(left, right)
                                }
                        })
                        .sum();
                    (left.clone(), right.clone(), value.round())
                })
                .collect();
        }
        located.changes_in(&located_overrides)
    }

    /// One glyph override of the edited master, moved to the location by the master's weight.
    fn located_glyph(
        &self,
        master: &LayoutInputs,
        located: &LayoutInputs,
        edit: &GlyphOverride,
    ) -> Option<GlyphOverride> {
        let before = master.glyphs.iter().find(|glyph| glyph.name == edit.name)?;
        let here = located
            .glyphs
            .iter()
            .find(|glyph| glyph.name == edit.name)?;
        let weight = self.weights.get(&edit.name).copied().unwrap_or(0.0);
        let anchors = edit.anchors.as_ref().map(|anchors| {
            here.anchors
                .iter()
                .map(|(name, x, y)| {
                    let find = |list: &[(String, f64, f64)]| {
                        list.iter()
                            .find(|(candidate, _, _)| candidate == name)
                            .map(|(_, x, y)| (*x, *y))
                    };
                    match (find(anchors), find(&before.anchors)) {
                        (Some((ox, oy)), Some((mx, my))) => {
                            (name.clone(), x + weight * (ox - mx), y + weight * (oy - my))
                        }
                        _ => (name.clone(), *x, *y),
                    }
                })
                .collect()
        });
        Some(GlyphOverride {
            name: edit.name.clone(),
            advance: here.advance + weight * (edit.advance - before.advance),
            anchors,
        })
    }
}

/// What [`LayoutTables::build_live`] built.
#[derive(Debug)]
pub struct LiveBuild {
    /// The shaping font.
    pub font: ShapingFont,
    /// Why the current feature text does not compile, on one line with its line number.
    /// When this is set, `font` was built from the last feature text that compiled.
    pub feature_error: Option<String>,
}

/// How many times each step of a build ran, since the builder was made.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayoutBuildCounts {
    /// First passes over a feature text.
    pub first_passes: u64,
    /// Runs of the mark writer.
    pub marks: u64,
    /// Runs of the kerning writer.
    pub kerning: u64,
    /// Compiles of the feature text with the generated lookups.
    pub compiles: u64,
}

/// Time spent in each step of one build.
#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutTimings {
    /// Parse, validate and compile GSUB of the feature file; zero when cached.
    pub first_pass: Duration,
    /// Fill the fontc IR from the inputs; zero when both writers are cached.
    pub ir: Duration,
    /// Run the mark writer; zero when cached.
    pub marks: Duration,
    /// Run the three kerning works; zero when cached.
    pub kerning: Duration,
    /// Compile the feature file with the generated lookups.
    pub compile: Duration,
    /// Write the shaping font.
    pub assemble: Duration,
}

impl LayoutTimings {
    /// The sum of all steps.
    pub fn total(&self) -> Duration {
        self.first_pass + self.ir + self.marks + self.kerning + self.compile + self.assemble
    }
}

struct FirstPass {
    key: u64,
    glyph_map: GlyphMap,
    glyph_order: GlyphOrder,
    output: FeaFirstPassOutput,
}

/// A builder of shaping fonts that keeps the results of earlier builds.
#[derive(Default)]
pub struct LayoutTables {
    first_pass: Option<FirstPass>,
    marks: Option<(u64, Arc<FeaRsMarks>)>,
    kerning: Option<(u64, Arc<FeaRsKerns>)>,
    /// The first-pass key and error of a feature text that did not compile, so a drag with
    /// broken features does not parse them again on every step.
    failed: Option<(u64, String)>,
    /// The last feature text that compiled; see [`Self::build_live`].
    good_features: Option<String>,
    counts: LayoutBuildCounts,
}

impl std::fmt::Debug for LayoutTables {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LayoutTables")
            .field("first_pass", &self.first_pass.as_ref().map(|pass| pass.key))
            .field("marks", &self.marks.as_ref().map(|(key, _)| *key))
            .field("kerning", &self.kerning.as_ref().map(|(key, _)| *key))
            .field("failed", &self.failed)
            .field(
                "good_features",
                &self.good_features.as_ref().map(String::len),
            )
            .field("counts", &self.counts)
            .finish()
    }
}

/// The generated lookups in the order fontc gives them to `fea-rs`: kerning, then marks.
struct GeneratedFeatures<'a> {
    kerning: &'a FeaRsKerns,
    marks: &'a FeaRsMarks,
}

impl FeatureProvider for GeneratedFeatures<'_> {
    fn add_features(&self, builder: &mut FeatureBuilder<'_>) {
        self.kerning.add_features(builder);
        self.marks.add_features(builder);
    }
}

fn hash_glyph_identity(glyph: &LayoutGlyph, hasher: &mut impl Hasher) {
    glyph.name.hash(hasher);
    glyph.codepoints.hash(hasher);
    glyph.category.hash(hasher);
}

impl LayoutInputs {
    /// What resolves the variable values of the feature file at the location of these
    /// inputs; `None` for a static font.
    ///
    /// Only `fea-rs` sees the axes: the values are read at the location, so the tables have
    /// no variation data, and the IR given to the fontc writers stays static.
    fn variation_info(&self) -> Option<LocationVariationInfo> {
        (!self.axes.is_empty())
            .then(|| LocationVariationInfo::new(self.axes.clone(), self.location.clone()))
    }

    fn first_pass_key(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.features.hash(&mut hasher);
        // Variable feature values are read at the location, so the first pass depends on it.
        for axis in &self.axes {
            format!("{axis:?}").hash(&mut hasher);
        }
        for (tag, value) in self.location.iter() {
            (tag, value.to_f64().to_bits()).hash(&mut hasher);
        }
        for glyph in &self.glyphs {
            glyph.name.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn mark_key(&self, first_pass: u64) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        first_pass.hash(&mut hasher);
        for glyph in &self.glyphs {
            hash_glyph_identity(glyph, &mut hasher);
            for (name, x, y) in &glyph.anchors {
                (name, x.to_bits(), y.to_bits()).hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    fn kerning_key(&self, first_pass: u64) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        first_pass.hash(&mut hasher);
        for glyph in &self.glyphs {
            hash_glyph_identity(glyph, &mut hasher);
            // The kerning writer reads widths only to tell spacing marks from others.
            (glyph.advance == 0.0).hash(&mut hasher);
        }
        self.groups.hash(&mut hasher);
        for (left, right, value) in &self.kerning {
            (left, right, value.to_bits()).hash(&mut hasher);
        }
        hasher.finish()
    }
}

const FEA_ROOT: &str = "features.fea";

/// The first error of `diagnostics` on one line, with its line in `text`.
fn feature_error(diagnostics: &DiagnosticSet, text: &str) -> String {
    let Some(error) = diagnostics.diagnostics().iter().find(|d| d.is_error()) else {
        return format!("{FEA_ROOT}: the feature text does not compile");
    };
    // The feature text has no include, so every span is an offset into `text`.
    let offset = error.span().start.min(text.len());
    let line = 1 + text.as_bytes()[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count();
    let message = error.text().lines().next().unwrap_or_default();
    format!("{FEA_ROOT} line {line}: {message}")
}

fn first_pass(inputs: &LayoutInputs, key: u64) -> Result<FirstPass, String> {
    let glyph_map = GlyphMap::new(inputs.glyphs.iter().map(|glyph| glyph.name.as_str()))
        .map_err(|error| format!("glyph order: {error}"))?;
    let glyph_order: GlyphOrder = inputs
        .glyphs
        .iter()
        .map(|glyph| GlyphName::new(&glyph.name))
        .collect();
    let text: Arc<str> = Arc::from(inputs.features.as_str());
    let resolver = move |path: &Path| {
        if path == Path::new(FEA_ROOT) {
            Ok(text.clone())
        } else {
            Err(SourceLoadError::new(
                path.to_path_buf(),
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "include() is not supported in layout tables",
                ),
            ))
        }
    };
    let (ast, diagnostics) = fea_rs::parse::parse_root(
        PathBuf::from(FEA_ROOT),
        Some(&glyph_map),
        Box::new(resolver),
    )
    .map_err(|error| format!("{FEA_ROOT}: {error}"))?;
    if diagnostics.has_errors() {
        return Err(feature_error(&diagnostics, &inputs.features));
    }
    let variation = inputs.variation_info();
    let diagnostics = fea_rs::compile::validate(&ast, &glyph_map, variation.as_ref());
    if diagnostics.has_errors() {
        return Err(feature_error(&diagnostics, &inputs.features));
    }
    // fontbe compiles GSUB once here for the glyph closure of the mark and kerning writers.
    let (compilation, _) = fea_rs::compile::compile::<_, NopFeatureProvider>(
        &ast,
        &glyph_map,
        variation.as_ref(),
        None,
        Opts::new().compile_gpos(false),
    )
    .map_err(|diagnostics| feature_error(&diagnostics, &inputs.features))?;
    let output = FeaFirstPassOutput::new(ast, compilation).map_err(|error| error.to_string())?;
    Ok(FirstPass {
        key,
        glyph_map,
        glyph_order,
        output,
    })
}

/// Fill a fontc IR context with the values of `inputs` at the default location.
fn ir_context(
    inputs: &LayoutInputs,
    glyph_order: &GlyphOrder,
    with_anchors: bool,
    with_kerning: bool,
) -> Result<IrContext, String> {
    let ir = IrContext::new_root(Flags::default()).copy_for_work(Access::All, Access::All);
    let default = NormalizedLocation::new();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "units per em is clamped to the OpenType range first"
    )]
    let units_per_em = inputs.units_per_em.clamp(16.0, 16384.0) as u16;
    let static_metadata = StaticMetadata::new(
        units_per_em,
        HashMap::new(),
        Vec::new(),
        Vec::new(),
        HashSet::from([default.clone()]),
        None,
        0.0,
        None,
        false,
    )
    .map_err(|error| error.to_string())?;
    ir.static_metadata.set(static_metadata);
    ir.glyph_order.set(glyph_order.clone());
    ir.gdef_categories.set(GdefCategories {
        categories: inputs
            .glyphs
            .iter()
            .filter_map(|glyph| {
                glyph
                    .category
                    .map(|category| (GlyphName::new(&glyph.name), category.class_def()))
            })
            .collect(),
    });
    for glyph in &inputs.glyphs {
        let name = GlyphName::new(&glyph.name);
        let instance = GlyphInstance {
            width: glyph.advance,
            ..GlyphInstance::default()
        };
        let ir_glyph = Glyph::new(
            name.clone(),
            true,
            glyph.codepoints.iter().copied().collect(),
            HashMap::from([(default.clone(), instance)]),
        )
        .map_err(|error| error.to_string())?;
        ir.glyphs.set_unconditionally(ir_glyph);
        if with_anchors && !glyph.anchors.is_empty() {
            let mut anchors = AnchorBuilder::new(name);
            for (anchor, x, y) in &glyph.anchors {
                anchors
                    .add(anchor.as_str().into(), default.clone(), (*x, *y).into())
                    .map_err(|error| error.to_string())?;
            }
            let anchors = anchors.build().map_err(|error| error.to_string())?;
            ir.anchors.set_unconditionally(anchors);
        }
    }
    if with_kerning {
        ir.kerning_locations.set(KerningLocations {
            locations: BTreeSet::from([default.clone()]),
        });
        let mut groups: BTreeMap<KernGroup, BTreeSet<GlyphName>> = BTreeMap::new();
        for (name, members) in &inputs.groups {
            let key = if name.starts_with("public.kern1.") {
                KernGroup::Side1(name.as_str().into())
            } else if name.starts_with("public.kern2.") {
                KernGroup::Side2(name.as_str().into())
            } else {
                continue;
            };
            groups.insert(key, members.iter().map(GlyphName::new).collect());
        }
        // As the Babelfont adapter: a side that names a missing group or glyph drops the pair.
        let side = |raw: &str, first: bool| -> Option<KernSide> {
            if raw.starts_with("public.kern") {
                let key = if first {
                    KernGroup::Side1(raw.into())
                } else {
                    KernGroup::Side2(raw.into())
                };
                groups.contains_key(&key).then_some(KernSide::Group(key))
            } else {
                let name = GlyphName::new(raw);
                glyph_order.contains(&name).then_some(KernSide::Glyph(name))
            }
        };
        let mut kerns = BTreeMap::new();
        for (left, right, value) in &inputs.kerning {
            if let (Some(left), Some(right)) = (side(left, true), side(right, false)) {
                kerns.insert((left, right), (*value).into());
            }
        }
        ir.kerning_at.set_unconditionally(KerningInstance {
            location: default,
            kerns,
            groups,
        });
    }
    Ok(ir)
}

fn run(work: Box<fontbe::orchestration::BeWork>, context: &BeContext) -> Result<(), String> {
    work.exec(&context.copy_for_work(Access::<AnyWorkId>::All, Access::All))
        .map_err(|error| error.to_string())
}

impl LayoutTables {
    /// A builder with nothing cached.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many times each step has run; tests use it to prove that an edit rebuilt nothing.
    pub fn build_counts(&self) -> LayoutBuildCounts {
        self.counts
    }

    /// Build the shaping font of the text line from `committed` with `overrides` in place.
    ///
    /// This is the one layout function of the editor: a drag step passes the values of the
    /// gesture as `overrides`, and the commit passes none.
    /// When the feature text does not compile, the font is built from the last feature text
    /// that compiled, or from no feature text before one has, with the current values, so
    /// advances, anchors and kerning still update; `feature_error` says why.
    pub fn build_live(
        &mut self,
        committed: &LayoutInputs,
        overrides: &LayoutOverrides,
    ) -> Result<LiveBuild, String> {
        let inputs = committed.with_overrides(overrides);
        match self.build(&inputs) {
            Ok(font) => {
                if self.good_features.as_deref() != Some(inputs.features.as_str()) {
                    self.good_features = Some(inputs.features.clone());
                }
                Ok(LiveBuild {
                    font,
                    feature_error: None,
                })
            }
            Err(error) => {
                let good = self.good_features.clone().unwrap_or_default();
                if good == inputs.features {
                    return Err(error);
                }
                let mut retry = inputs.into_owned();
                retry.features = good;
                let font = self.build(&retry)?;
                Ok(LiveBuild {
                    font,
                    feature_error: Some(error),
                })
            }
        }
    }

    /// Build a shaping font from `inputs`, reusing what the previous build computed from the
    /// same values.
    pub fn build(&mut self, inputs: &LayoutInputs) -> Result<ShapingFont, String> {
        self.build_timed(inputs).map(|(font, _)| font)
    }

    /// Build as [`Self::build`] does, and report the time of each step.
    pub fn build_timed(
        &mut self,
        inputs: &LayoutInputs,
    ) -> Result<(ShapingFont, LayoutTimings), String> {
        let mut timings = LayoutTimings::default();
        if inputs.glyphs.is_empty() {
            return Err("no glyphs to shape with".into());
        }

        let start = Instant::now();
        let first_key = inputs.first_pass_key();
        if self
            .first_pass
            .as_ref()
            .is_none_or(|pass| pass.key != first_key)
        {
            if let Some((key, error)) = &self.failed
                && *key == first_key
            {
                return Err(error.clone());
            }
            // The cached results stay until a new first pass succeeds, so a feature text that
            // does not compile leaves those of the last one that did.
            let pass = first_pass(inputs, first_key).inspect_err(|error| {
                self.failed = Some((first_key, error.clone()));
            })?;
            self.first_pass = Some(pass);
            self.marks = None;
            self.kerning = None;
            self.counts.first_passes += 1;
            timings.first_pass = start.elapsed();
        }
        let pass = self
            .first_pass
            .as_ref()
            .expect("the first pass was just set");

        let mark_key = inputs.mark_key(first_key);
        let kerning_key = inputs.kerning_key(first_key);
        let marks_stale = self.marks.as_ref().is_none_or(|(key, _)| *key != mark_key);
        let kerning_stale = self
            .kerning
            .as_ref()
            .is_none_or(|(key, _)| *key != kerning_key);

        if marks_stale || kerning_stale {
            let start = Instant::now();
            let ir = ir_context(inputs, &pass.glyph_order, marks_stale, kerning_stale)?;
            let be_root = BeContext::new_root(Flags::default(), None, None, false, &ir);
            let be = be_root.copy_for_work(Access::All, Access::All);
            be.fea_ast.set(pass.output.clone());
            timings.ir = start.elapsed();

            if marks_stale {
                let start = Instant::now();
                run(fontbe::features::create_mark_work(), &be)?;
                self.marks = Some((mark_key, be.fea_rs_marks.get()));
                self.counts.marks += 1;
                timings.marks = start.elapsed();
            }
            if kerning_stale {
                let start = Instant::now();
                run(fontbe::features::create_gather_ir_kerning_work(), &be)?;
                for work in fontbe::features::create_kern_segment_work(&be.all_kerning_pairs.get())
                {
                    run(work, &be)?;
                }
                run(fontbe::features::create_kerns_work(), &be)?;
                self.kerning = Some((kerning_key, be.fea_rs_kerns.get()));
                self.counts.kerning += 1;
                timings.kerning = start.elapsed();
            }
        }

        let start = Instant::now();
        let provider = GeneratedFeatures {
            kerning: &self.kerning.as_ref().expect("kerning was built").1,
            marks: &self.marks.as_ref().expect("marks were built").1,
        };
        let variation = inputs.variation_info();
        let (mut compilation, _) = fea_rs::compile::compile(
            &pass.output.ast,
            &pass.glyph_map,
            variation.as_ref(),
            Some(&provider),
            Opts::new(),
        )
        .map_err(|diagnostics| feature_error(&diagnostics, &inputs.features))?;
        self.counts.compiles += 1;
        // As fontbe's feature compilation: without a GlyphClassDef in the feature file, GDEF
        // takes the glyph classes from the categories of the source.
        if compilation.gdef_classes.is_none() {
            let class_def: ClassDef = inputs
                .glyphs
                .iter()
                .enumerate()
                .filter_map(|(index, glyph)| {
                    let id = write_fonts::types::GlyphId16::new(u16::try_from(index).ok()?);
                    Some((id, glyph.category?.class_def() as u16))
                })
                .collect();
            if class_def.iter().next().is_some() {
                let gdef = compilation.gdef.get_or_insert_with(Default::default);
                gdef.glyph_class_def.set(class_def);
            }
        }
        timings.compile = start.elapsed();

        let start = Instant::now();
        let glyphs: Vec<ShapingGlyph> = inputs
            .glyphs
            .iter()
            .map(|glyph| ShapingGlyph {
                name: glyph.name.clone(),
                advance: glyph.advance,
                unicodes: glyph.codepoints.clone(),
            })
            .collect();
        let font = ShapingFont::from_tables(inputs.units_per_em, &glyphs, &compilation)?;
        timings.assemble = start.elapsed();
        Ok((font, timings))
    }
}

/// Build a shaping font from `inputs` with nothing cached.
pub fn build_shaping_font(inputs: &LayoutInputs) -> Result<ShapingFont, String> {
    LayoutTables::new().build(inputs)
}

#[cfg(test)]
mod live_tests {
    //! [`LayoutTables::build_live`] on the repository-owned Arabic fixture.

    use super::*;

    type Row = (String, f64, f64, f64);

    fn rows(font: &ShapingFont, text: &str, rtl: bool) -> Vec<Row> {
        font.shape(text, rtl)
            .unwrap()
            .iter()
            .map(|glyph| {
                (
                    font.glyph_name(glyph.glyph_id).unwrap_or("?").to_string(),
                    glyph.x_advance,
                    glyph.x_offset,
                    glyph.y_offset,
                )
            })
            .collect()
    }

    fn arabic() -> LayoutInputs {
        let project = Project::load(&crate::testing::fonts::arabic_regular_ufo()).unwrap();
        let source = project.document_sources().next().unwrap().id();
        LayoutInputs::from_project(&project, source).unwrap()
    }

    /// The overrides of a drag of the `bottom` anchor and the advance of `beh-ar`, and of the
    /// reh and alef group pair.
    fn drag(inputs: &LayoutInputs, step: f64) -> LayoutOverrides {
        let beh = inputs.glyphs.iter().find(|g| g.name == "beh-ar").unwrap();
        let anchors = beh
            .anchors
            .iter()
            .map(|(name, x, y)| {
                let dy = if name == "bottom" { -step } else { 0.0 };
                (name.clone(), *x + step, *y + dy)
            })
            .collect();
        LayoutOverrides {
            glyphs: vec![GlyphOverride {
                name: "beh-ar".into(),
                advance: beh.advance + step,
                anchors: Some(anchors),
            }],
            kerning: vec![(
                "public.kern1.reh".into(),
                "public.kern2.alef".into(),
                -60.0 - step,
            )],
        }
    }

    const LINES: [&str; 3] = ["بِ بِ", "را", "بلا"];

    #[test]
    fn drag_steps_equal_a_fresh_build_of_the_committed_values() {
        let committed = arabic();
        let mut tables = LayoutTables::new();
        tables
            .build_live(&committed, &LayoutOverrides::default())
            .unwrap();
        let mut last = None;
        for step in 1..=5 {
            let overrides = drag(&committed, f64::from(step) * 7.0);
            last = Some(tables.build_live(&committed, &overrides).unwrap().font);
        }
        let last = last.unwrap();
        // The commit of the last step's values, built with nothing cached.
        let edited = committed
            .with_overrides(&drag(&committed, 35.0))
            .into_owned();
        let fresh = build_shaping_font(&edited).unwrap();
        // The commit, built by the same builder as the drag steps with no overrides.
        let commit = tables
            .build_live(&edited, &LayoutOverrides::default())
            .unwrap()
            .font;
        for text in LINES {
            assert_eq!(rows(&last, text, true), rows(&fresh, text, true), "{text}");
            assert_eq!(rows(&commit, text, true), rows(&last, text, true), "{text}");
        }
        let before = build_shaping_font(&committed).unwrap();
        assert_ne!(
            rows(&before, LINES[0], true),
            rows(&last, LINES[0], true),
            "the kasras follow the dragged anchor"
        );
        assert_ne!(rows(&before, "را", true), rows(&last, "را", true));
    }

    #[test]
    fn overrides_equal_to_the_committed_values_change_nothing() {
        let committed = arabic();
        assert!(committed.changes_in(&drag(&committed, 0.0)).is_empty());
        assert_eq!(committed.changes_in(&drag(&committed, 3.0)).glyphs.len(), 1);
        let mut tables = LayoutTables::new();
        tables
            .build_live(&committed, &LayoutOverrides::default())
            .unwrap();
        let counts = tables.build_counts();
        // An advance alone runs neither writer: the kerning writer reads only zero widths.
        let mut advance = drag(&committed, 4.0);
        advance.glyphs[0].anchors = None;
        advance.kerning.clear();
        tables.build_live(&committed, &advance).unwrap();
        let after = tables.build_counts();
        assert_eq!(after.first_passes, counts.first_passes);
        assert_eq!(after.marks, counts.marks);
        assert_eq!(after.kerning, counts.kerning);
        assert_eq!(after.compiles, counts.compiles + 1);
    }

    #[test]
    fn a_feature_error_keeps_the_last_features_with_the_current_values() {
        let committed = arabic();
        let mut tables = LayoutTables::new();
        let good = tables
            .build_live(&committed, &LayoutOverrides::default())
            .unwrap();
        assert!(good.feature_error.is_none());
        assert_eq!(rows(&good.font, "لا", true)[0].0, "lam_alef-ar");

        // Line 3 of the feature text no longer parses.
        let mut broken = committed.clone();
        let mut lines: Vec<&str> = broken.features.lines().collect();
        lines.insert(2, "feature broken { sub ; } broken;");
        broken.features = lines.join("\n");
        let built = tables.build_live(&broken, &drag(&broken, 20.0)).unwrap();
        let error = built.feature_error.expect("the error is reported");
        assert!(error.starts_with("features.fea line 3:"), "{error}");
        assert!(!error.contains('\n'), "{error}");
        // Substitutions are those of the last features that compiled.
        assert_eq!(rows(&built.font, "لا", true)[0].0, "lam_alef-ar");
        // Anchors, advances and kerning are the current values.
        let edited = committed
            .with_overrides(&drag(&committed, 20.0))
            .into_owned();
        let expected = build_shaping_font(&edited).unwrap();
        for text in LINES {
            assert_eq!(rows(&built.font, text, true), rows(&expected, text, true));
        }
        // The broken text is parsed once, not on every drag step.
        let counts = tables.build_counts();
        tables.build_live(&broken, &drag(&broken, 21.0)).unwrap();
        assert_eq!(tables.build_counts().first_passes, counts.first_passes);

        // Fixing the text clears the error.
        let fixed = tables
            .build_live(&committed, &LayoutOverrides::default())
            .unwrap();
        assert!(fixed.feature_error.is_none());
    }
}

#[cfg(all(test, feature = "integration-fonts"))]
mod tests {
    use super::*;
    use crate::text::shape::ShapedGlyph;

    type Row = (String, f64, f64, f64);

    fn rows(font: &ShapingFont, text: &str, rtl: bool) -> Vec<Row> {
        let row = |glyph: &ShapedGlyph| {
            (
                font.glyph_name(glyph.glyph_id).unwrap_or("?").to_string(),
                glyph.x_advance,
                glyph.x_offset,
                glyph.y_offset,
            )
        };
        font.shape(text, rtl).unwrap().iter().map(row).collect()
    }

    fn virtua() -> (Project, SourceId) {
        let project = Project::load(&crate::testing::fonts::virtua_designspace()).unwrap();
        let source = project.document_sources().next().unwrap().id();
        (project, source)
    }

    /// Bidirectional runs of a mixed line, each shaped in its own direction.
    fn runs(text: &str) -> Vec<(String, bool)> {
        let info = unicode_bidi::BidiInfo::new(text, None);
        let mut out = Vec::new();
        for paragraph in &info.paragraphs {
            let (levels, ranges) = info.visual_runs(paragraph, paragraph.range.clone());
            for range in ranges {
                out.push((
                    text[range.clone()].to_string(),
                    levels[range.start].is_rtl(),
                ));
            }
        }
        out
    }

    const SAMPLES: [(&str, bool); 5] = [
        ("بِسْمِ ٱللَّهِ ٱلرَّحْمَٰنِ", true),
        ("فيرتوا غروتيسك لا لله", true),
        ("בְּרֵאשִׁית בָּרָא אֱלֹהִים", true),
        ("AVATAR Tölpel ǻ ế", false),
        ("To Wa. LT", false),
    ];
    const MIXED: &str = "AVATAR بِسْمِ ٱللَّهِ To בָּרָא Wa.";

    #[test]
    fn layout_tables_match_the_fontc_compile_with_kerning() {
        let (project, source) = virtua();
        let inputs = LayoutInputs::from_project(&project, source).unwrap();
        assert!(!inputs.kerning.is_empty());
        let live = build_shaping_font(&inputs).unwrap();
        let compiled = project.compile().unwrap();
        let fontc = ShapingFont::from_bytes((*compiled.bytes).clone()).unwrap();

        for (text, rtl) in SAMPLES {
            assert_eq!(rows(&live, text, rtl), rows(&fontc, text, rtl), "{text}");
        }
        let mixed = runs(MIXED);
        assert!(mixed.len() >= 3);
        for (text, rtl) in mixed {
            assert_eq!(rows(&live, &text, rtl), rows(&fontc, &text, rtl), "{text}");
        }
        // Kerning takes part: "AV" is closer than the sum of the advances.
        let kerned = rows(&live, "AV", false);
        let unkerned: Vec<Row> = live
            .shape_with_features("AV", false, &[("kern".into(), false)])
            .unwrap()
            .iter()
            .map(|glyph| ("".into(), glyph.x_advance, 0.0, 0.0))
            .collect();
        assert_ne!(kerned[0].1, unkerned[0].1);
    }

    #[test]
    fn a_cached_rebuild_equals_a_fresh_build_after_an_edit() {
        let (project, source) = virtua();
        let mut inputs = LayoutInputs::from_project(&project, source).unwrap();
        let mut tables = LayoutTables::new();
        tables.build(&inputs).unwrap();

        // Move one base anchor and change one pair.
        let beh = inputs
            .glyphs
            .iter_mut()
            .find(|glyph| glyph.name == "beh-ar.init")
            .unwrap();
        if let Some(anchor) = beh.anchors.first_mut() {
            anchor.2 += 37.0;
        }
        inputs.kerning[0].2 += 23.0;
        let (cached, timings) = tables.build_timed(&inputs).unwrap();
        assert_eq!(timings.first_pass, Duration::ZERO);
        let fresh = build_shaping_font(&inputs).unwrap();
        for (text, rtl) in SAMPLES {
            assert_eq!(rows(&cached, text, rtl), rows(&fresh, text, rtl), "{text}");
        }
    }

    #[expect(
        clippy::print_stdout,
        reason = "the timing probe reports to the terminal"
    )]
    fn print_timings(label: &str, edit: &str, read: Duration, timings: &LayoutTimings) {
        let ms = |duration: Duration| duration.as_secs_f64() * 1000.0;
        println!(
            "{label:<24} {edit:<10} read {:>7.2}  first {:>7.2}  ir {:>7.2}  marks {:>7.2}  \
             kern {:>7.2}  compile {:>7.2}  font {:>6.2}  build {:>7.2}  total {:>7.2} ms",
            ms(read),
            ms(timings.first_pass),
            ms(timings.ir),
            ms(timings.marks),
            ms(timings.kerning),
            ms(timings.compile),
            ms(timings.assemble),
            ms(timings.total()),
            ms(read + timings.total()),
        );
    }

    /// Print the cost of a cold build, a build with no change, and builds after an anchor edit,
    /// a kerning edit and both, each the fastest of 15 samples.
    #[expect(
        clippy::print_stdout,
        reason = "the timing probe reports to the terminal"
    )]
    fn measure(
        label: &str,
        project: &Project,
        source: SourceId,
        cut: Option<usize>,
        clear_features: bool,
    ) {
        let read_inputs = || {
            let start = Instant::now();
            let mut inputs = LayoutInputs::from_project(project, source).unwrap();
            if clear_features {
                inputs.features.clear();
            }
            if let Some(count) = cut {
                cut_glyphs(&mut inputs, count);
            }
            (inputs, start.elapsed())
        };
        let (base, read) = read_inputs();
        println!(
            "{label}: {} glyphs, {} glyphs with anchors, {} pairs, {} groups",
            base.glyphs.len(),
            base.glyphs.iter().filter(|g| !g.anchors.is_empty()).count(),
            base.kerning.len(),
            base.groups.len()
        );
        let mut tables = LayoutTables::new();
        let (_, cold) = tables.build_timed(&base).unwrap();
        print_timings(label, "cold", read, &cold);
        let (_, warm) = tables.build_timed(&base).unwrap();
        print_timings(label, "no change", Duration::ZERO, &warm);

        let anchored = base
            .glyphs
            .iter()
            .position(|glyph| !glyph.anchors.is_empty())
            .unwrap();
        // The fastest sample: on a loaded host it is the closest to the cost of the work.
        let fastest = |mut samples: Vec<(Duration, LayoutTimings)>| {
            samples.sort_by_key(|(read, timings)| *read + timings.total());
            samples[0]
        };
        for (edit, kind) in [("anchor", 0), ("kerning", 1), ("both", 2)] {
            let mut samples = Vec::new();
            for step in 0..15 {
                let (mut inputs, read) = read_inputs();
                let delta = f64::from(step + 1);
                if kind != 1 {
                    inputs.glyphs[anchored].anchors[0].1 += delta;
                }
                if kind != 0 && !inputs.kerning.is_empty() {
                    inputs.kerning[0].2 += delta;
                }
                let (_, timings) = tables.build_timed(&inputs).unwrap();
                samples.push((read, timings));
            }
            let (read, timings) = fastest(samples);
            print_timings(label, edit, read, &timings);
        }
        let mut samples = Vec::new();
        for _ in 0..15 {
            let (_, timings) = LayoutTables::new().build_timed(&base).unwrap();
            samples.push((Duration::ZERO, timings));
        }
        let (_, timings) = fastest(samples);
        print_timings(label, "uncached", read, &timings);
    }

    /// Keep about `count` glyphs: marks and bases with anchors, and the glyphs they kern with.
    fn cut_glyphs(inputs: &mut LayoutInputs, count: usize) {
        let mut keep: HashSet<String> = HashSet::new();
        let marks: Vec<&LayoutGlyph> = inputs
            .glyphs
            .iter()
            .filter(|g| g.anchors.iter().any(|(name, _, _)| name.starts_with('_')))
            .collect();
        let bases: Vec<&LayoutGlyph> = inputs
            .glyphs
            .iter()
            .filter(|g| {
                !g.anchors.is_empty() && !g.anchors.iter().any(|(name, _, _)| name.starts_with('_'))
            })
            .collect();
        for glyph in marks.iter().take(count / 4) {
            keep.insert(glyph.name.clone());
        }
        for glyph in bases.iter().take(count - keep.len().min(count)) {
            keep.insert(glyph.name.clone());
        }
        inputs.retain_glyphs(|glyph| keep.contains(&glyph.name));
    }

    #[test]
    #[ignore = "prints timings; run with --ignored --nocapture"]
    #[expect(
        clippy::print_stdout,
        reason = "the timing probe reports to the terminal"
    )]
    fn layout_tables_timings() {
        let (project, source) = virtua();
        measure("Virtua Grotesk", &project, source, None, false);
        measure("Virtua Grotesk, cut", &project, source, Some(150), true);

        let amiri = std::env::var("RUNEBENDER_AMIRI_DESIGNSPACE").ok();
        if let Some(path) = amiri {
            let start = Instant::now();
            let project = Project::load(Path::new(&path)).unwrap();
            println!(
                "Amiri load {:.0} ms",
                start.elapsed().as_secs_f64() * 1000.0
            );
            let source = project.document_sources().next().unwrap().id();
            measure("Amiri", &project, source, None, false);
            measure("Amiri, cut", &project, source, Some(150), false);
        }
    }
}

/// Values between masters, on the repository-owned Arabic fixture.
#[cfg(test)]
mod location_tests {
    use super::*;
    use crate::testing::fonts;

    /// One shaped glyph: name, advance, x offset and y offset.
    type Row = (String, f64, f64, f64);

    /// The strings of the fixture README, with their direction.
    const STRINGS: [(&str, bool); 15] = [
        ("بسم", true),
        ("لا", true),
        ("بلا", true),
        ("لَاُ", true),
        ("لأ", true),
        ("بَّ", true),
        ("بِ", true),
        ("يِ", true),
        ("سسس", true),
        ("را", true),
        ("رم", true),
        ("رَا", true),
        ("رُه", true),
        ("AV", false),
        ("To 012.", false),
    ];

    fn load() -> Project {
        Project::load(&fonts::arabic_designspace()).expect("the Arabic fixture loads")
    }

    fn sources(project: &Project) -> (SourceId, SourceId) {
        let mut ids = project.document_sources().map(|source| source.id());
        (ids.next().unwrap(), ids.next().unwrap())
    }

    fn weight(value: f64) -> Location {
        [("Weight".to_owned(), value)].into()
    }

    fn rows(font: &ShapingFont, text: &str, rtl: bool) -> Vec<Row> {
        font.shape_with_options(text, rtl, &[], None, None)
            .unwrap()
            .iter()
            .map(|glyph| {
                (
                    font.glyph_name(glyph.glyph_id).unwrap_or("?").to_string(),
                    glyph.x_advance,
                    glyph.x_offset,
                    glyph.y_offset,
                )
            })
            .collect()
    }

    fn glyph<'a>(inputs: &'a LayoutInputs, name: &str) -> &'a LayoutGlyph {
        inputs
            .glyphs
            .iter()
            .find(|glyph| glyph.name == name)
            .unwrap_or_else(|| panic!("{name} is in the inputs"))
    }

    #[test]
    fn at_a_master_the_values_are_those_of_the_master() {
        let project = load();
        let (regular, bold) = sources(&project);
        for (source, location) in [(regular, weight(0.0)), (bold, weight(1.0))] {
            let (inputs, diagnostics) = LayoutInputs::at_location(&project, &location, source)
                .expect("the inputs are read");
            assert_eq!(
                inputs,
                LayoutInputs::from_project(&project, source).unwrap()
            );
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        }
    }

    #[test]
    fn at_the_midpoint_each_glyph_is_the_mean_and_one_anchor_is_reported() {
        let project = load();
        let (regular, bold) = sources(&project);
        let low = LayoutInputs::from_project(&project, regular).unwrap();
        let high = LayoutInputs::from_project(&project, bold).unwrap();
        let (middle, diagnostics) =
            LayoutInputs::at_location(&project, &weight(0.5), regular).unwrap();

        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].glyph, "dal-ar");
        assert_eq!(diagnostics[0].anchor.as_deref(), Some("top"));
        assert!(
            diagnostics[0].reason.contains("Regular"),
            "{}",
            diagnostics[0]
        );

        let mut compared = 0;
        for glyph in &middle.glyphs {
            if glyph.name == "dal-ar" || glyph.name == ".notdef" {
                continue;
            }
            let (a, b) = (
                self::glyph(&low, &glyph.name),
                self::glyph(&high, &glyph.name),
            );
            assert_eq!(
                glyph.advance,
                (a.advance + b.advance) / 2.0,
                "{}",
                glyph.name
            );
            assert_eq!(glyph.anchors.len(), a.anchors.len(), "{}", glyph.name);
            for (name, x, y) in &glyph.anchors {
                let from = |inputs: &[(String, f64, f64)]| {
                    inputs
                        .iter()
                        .find(|(candidate, ..)| candidate == name)
                        .map(|(_, x, y)| (*x, *y))
                        .unwrap()
                };
                let (ax, ay) = from(&a.anchors);
                let (bx, by) = from(&b.anchors);
                assert_eq!(
                    (*x, *y),
                    ((ax + bx) / 2.0, (ay + by) / 2.0),
                    "{}",
                    glyph.name
                );
            }
            compared += 1;
        }
        assert!(compared > 40);
        // The contours of noon do not interpolate, but its layout values do.
        let noon = glyph(&middle, "noon-ar");
        assert_eq!(
            noon.advance,
            (glyph(&low, "noon-ar").advance + glyph(&high, "noon-ar").advance) / 2.0
        );
        // The anchor that only Bold has takes the value of the nearest master: on the tie at
        // the midpoint, the source passed in, Regular, which has no `top`.
        let dal = glyph(&middle, "dal-ar");
        assert!(dal.anchors.iter().all(|(name, ..)| name != "top"));
        let (nearer_bold, _) = LayoutInputs::at_location(&project, &weight(0.75), regular).unwrap();
        assert!(
            glyph(&nearer_bold, "dal-ar")
                .anchors
                .iter()
                .any(|(name, x, y)| name == "top" && (*x, *y) == (300.0, 400.0))
        );
        assert_eq!(middle.kerning.len(), low.kerning.len());

        // The tables build, and the strings of the README keep their attached marks.
        let font = build_shaping_font(&middle).expect("the midpoint builds");
        let fatha = rows(&font, "لَاُ", true);
        assert_eq!(fatha[1].0, "fatha-ar");
        assert!(fatha[1].2 > 0.0 && fatha[1].3 > 0.0, "{fatha:?}");
        let kasra = rows(&font, "بِ", true);
        assert_eq!(kasra[0].0, "kasra-ar");
        assert!(kasra[0].3 < 0.0, "{kasra:?}");
        let stacked = rows(&font, "بَّ", true);
        assert!(stacked[0].3 > stacked[1].3, "{stacked:?}");
        let cursive = rows(&font, "سسس", true);
        assert!(cursive[2].3 < cursive[1].3, "{cursive:?}");
    }

    /// Variable values in the feature file are read at the location of the inputs, as fontc
    /// reads them, and the tables stay static.
    #[test]
    fn variable_feature_values_are_read_at_the_location() {
        use skrifa::raw::TableProvider as _;
        const KASRA: &str = "kasra-ar' <0 -80 0 0>";
        const VARIABLE_KASRA: &str = "kasra-ar' <0 (wght=400:-80 wght=700:-120) 0 0>";
        // A cursive attachment in a feature of its own, with a variable exit anchor.
        const CURSIVE: &str = "\nfeature ss01 {\n    lookup variable_exit {\n        \
            pos cursive period <anchor 0 0> <anchor (wght=400:0 wght=700:0) \
            (wght=400:50 wght=700:90)>;\n    } variable_exit;\n} ss01;\n";
        let project = load();
        let (regular, _) = sources(&project);
        let original = project.document_feature_text(regular).unwrap().to_owned();
        assert!(original.contains(KASRA));
        let edited = format!("{}{CURSIVE}", original.replace(KASRA, VARIABLE_KASRA));

        let mut finished = load();
        for name in ["noon-ar", "dal-ar"] {
            finished.remove_document_glyph(name).unwrap();
        }
        assert!(finished.set_feature_text(edited.clone()));
        let compiled = finished
            .compile()
            .expect("the edited font compiles with fontc");

        let ss01 = [("ss01".to_string(), true)];
        let shaped = |font: &ShapingFont, text: &str, rtl: bool| -> Vec<Row> {
            font.shape_with_options(text, rtl, &ss01, None, None)
                .unwrap()
                .iter()
                .map(|glyph| {
                    (
                        font.glyph_name(glyph.glyph_id).unwrap_or("?").to_string(),
                        glyph.x_advance,
                        glyph.x_offset,
                        glyph.y_offset,
                    )
                })
                .collect()
        };
        let mut cursive = Vec::new();
        for (at, lowered) in [(0.0, -80.0), (1.0, -120.0), (0.5, -100.0)] {
            let (mut inputs, _) =
                LayoutInputs::at_location(&project, &weight(at), regular).unwrap();
            // The baseline lowers the kasra by nothing.
            inputs.features = original.replace(KASRA, "kasra-ar' <0 0 0 0>");
            let before = build_shaping_font(&inputs).expect("the baseline features build");
            inputs.features.clone_from(&edited);
            let bytes = build_shaping_font(&inputs)
                .expect("variable feature values build")
                .into_bytes();
            let font = skrifa::FontRef::new(&bytes).unwrap();
            assert!(font.fvar().is_err(), "the shaping font has no axes");
            // fea-rs gives each variable value a variation index into a GDEF store, but the
            // values are resolved at the location, so the store holds no region and no delta.
            if let Some(store) = font.gdef().ok().and_then(|gdef| gdef.item_var_store()) {
                let store = store.unwrap();
                let regions = store.variation_region_list().unwrap();
                assert_eq!(
                    regions.region_count(),
                    0,
                    "the tables have no variation data"
                );
            }
            let live = ShapingFont::from_bytes(bytes).unwrap();
            let kasra = |font: &ShapingFont| {
                shaped(font, "يِ", true)
                    .into_iter()
                    .find(|row| row.0 == "kasra-ar")
                    .unwrap()
                    .3
            };
            assert_eq!(kasra(&live) - kasra(&before), lowered, "at {at}");
            let fontc = ShapingFont::from_bytes(compiled.bytes.to_vec())
                .unwrap()
                .at_normalized(vec![at]);
            for (text, rtl) in STRINGS.iter().copied().chain([("..", false)]) {
                assert_eq!(
                    shaped(&live, text, rtl),
                    shaped(&fontc, text, rtl),
                    "{text} at {at}"
                );
            }
            cursive.push(shaped(&live, "..", false));
        }
        assert_ne!(
            cursive[0], cursive[1],
            "the variable anchor moves the attachment"
        );
    }

    /// The live tables between masters equal the fontc variable compile at the same location.
    ///
    /// The values are rounded in different places: the live path interpolates the source
    /// values and rounds the result, while the compiled font rounds the values of each master
    /// and the deltas, and the shaper rounds the result at the location.
    /// So a difference of one unit is allowed; on this fixture, at the three locations, there
    /// is no difference.
    #[test]
    fn between_masters_the_tables_match_the_fontc_variable_compile() {
        let project = load();
        let (regular, _) = sources(&project);
        let mut finished = load();
        for name in ["noon-ar", "dal-ar"] {
            finished.remove_document_glyph(name).unwrap();
        }
        let compiled = finished.compile().expect("the finished glyphs compile");
        for at in [0.3, 0.5, 0.77] {
            let fontc = ShapingFont::from_bytes(compiled.bytes.to_vec())
                .unwrap()
                .at_normalized(vec![at]);
            let (inputs, _) = LayoutInputs::at_location(&project, &weight(at), regular).unwrap();
            let live = build_shaping_font(&inputs).unwrap();
            for (text, rtl) in STRINGS {
                let (live, fontc) = (rows(&live, text, rtl), rows(&fontc, text, rtl));
                assert_eq!(live.len(), fontc.len(), "{text} at {at}");
                for (a, b) in live.iter().zip(&fontc) {
                    assert_eq!(a.0, b.0, "{text} at {at}");
                    for (x, y) in [(a.1, b.1), (a.2, b.2), (a.3, b.3)] {
                        assert!(
                            (x - y).abs() <= 1.0,
                            "{text} at {at}: live {live:?}, fontc {fontc:?}"
                        );
                    }
                }
            }
        }
    }
}
