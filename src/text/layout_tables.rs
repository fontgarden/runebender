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
//! This is the prototype of item 0.1 in `docs/ARABIC-TEXT-LAYOUT-PLAN.md`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fea_rs::{
    GlyphMap, Opts,
    compile::{FeatureBuilder, FeatureProvider, NopFeatureProvider, NopVariationInfo},
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

use crate::font::project::Project;
use crate::font::variable::SourceId;
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
        Ok(Self {
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
}

impl std::fmt::Debug for LayoutTables {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LayoutTables")
            .field("first_pass", &self.first_pass.as_ref().map(|pass| pass.key))
            .field("marks", &self.marks.as_ref().map(|(key, _)| *key))
            .field("kerning", &self.kerning.as_ref().map(|(key, _)| *key))
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
    fn first_pass_key(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.features.hash(&mut hasher);
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
    .map_err(|error| format!("features.fea: {error}"))?;
    if diagnostics.has_errors() {
        return Err(format!("features.fea: {}", diagnostics.display()));
    }
    let diagnostics = fea_rs::compile::validate::<NopVariationInfo>(&ast, &glyph_map, None);
    if diagnostics.has_errors() {
        return Err(format!("features.fea: {}", diagnostics.display()));
    }
    // fontbe compiles GSUB once here for the glyph closure of the mark and kerning writers.
    let (compilation, _) = fea_rs::compile::compile::<NopVariationInfo, NopFeatureProvider>(
        &ast,
        &glyph_map,
        None,
        None,
        Opts::new().compile_gpos(false),
    )
    .map_err(|diagnostics| format!("features.fea: {}", diagnostics.display()))?;
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
            self.first_pass = None;
            self.marks = None;
            self.kerning = None;
            self.first_pass = Some(first_pass(inputs, first_key)?);
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
                timings.kerning = start.elapsed();
            }
        }

        let start = Instant::now();
        let provider = GeneratedFeatures {
            kerning: &self.kerning.as_ref().expect("kerning was built").1,
            marks: &self.marks.as_ref().expect("marks were built").1,
        };
        let (mut compilation, _) = fea_rs::compile::compile::<NopVariationInfo, _>(
            &pass.output.ast,
            &pass.glyph_map,
            None,
            Some(&provider),
            Opts::new(),
        )
        .map_err(|diagnostics| format!("features.fea: {}", diagnostics.display()))?;
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
        let compiled = project.compile_layout().unwrap();
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
