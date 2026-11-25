// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Real OpenType shaping for the editor's text buffer.
//!
//! The editor supplies a complete live font from `font::compiler`, including
//! variable outlines, advances and layout tables. The shaper receives the same
//! normalized coordinates used by the outline renderer.
//! `ShapingSource` also supports a lightweight single-master font for standalone
//! feature checks and fallback while a new document revision is compiling.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fea_rs::{
    Compiler, GlyphMap,
    compile::{NopFeatureProvider, NopVariationInfo},
    parse::{SourceLoadError, SourceResolver},
};
use harfrust::{Direction, FontRef, ShaperData, UnicodeBuffer};
use icu_properties::{CodePointMapData, props::Script};

use crate::outline::glyph_paths::round_units;
use write_fonts::{
    FontBuilder,
    tables::{
        cmap::Cmap,
        head::Head,
        hhea::Hhea,
        hmtx::{Hmtx, LongMetric},
        maxp::Maxp,
    },
    types::GlyphId,
};

/// One glyph as the shaper needs to see it.
#[derive(Debug, Clone)]
pub struct ShapingGlyph {
    /// The glyph name, used to build the post table and name lookups.
    pub name: String,
    /// Advance width in font units.
    pub advance: f64,
    /// Codepoints that map to this glyph. Usually zero or one.
    pub unicodes: Vec<u32>,
}

/// Everything needed to build a shaping font for one master.
#[derive(Debug, Clone)]
pub struct ShapingSource {
    /// Units per em of the master.
    pub units_per_em: f64,
    /// Glyph order. Index is the glyph id, so `.notdef` belongs first.
    pub glyphs: Vec<ShapingGlyph>,
    /// The master's features.fea, verbatim.
    pub features: String,
}

/// One glyph the shaper produced.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedGlyph {
    /// Index into the source's glyph order.
    pub glyph_id: u16,
    /// UTF-8 byte offset in the input. A ligature reports the
    /// cluster of its first character, so several glyphs can share one
    /// cluster and one glyph can stand for several characters.
    pub cluster: u32,
    /// Horizontal advance in font units, after positioning.
    pub x_advance: f64,
    /// Horizontal offset from the pen position in font units.
    pub x_offset: f64,
    /// Vertical offset from the baseline in font units.
    pub y_offset: f64,
}

/// A compiled shaping font, ready to shape with.
#[derive(Clone, Debug)]
pub struct ShapingFont {
    bytes: Vec<u8>,
    names: Vec<String>,
    normalized: Vec<f64>,
}

/// features.fea handed to fea-rs from memory: there is no filesystem in
/// wasm, and the feature text arrives from the host as a string.
struct InMemoryFea {
    root: PathBuf,
    text: Arc<str>,
}

impl SourceResolver for InMemoryFea {
    fn get_contents(&self, path: &Path) -> Result<Arc<str>, SourceLoadError> {
        if path == self.root {
            Ok(self.text.clone())
        } else {
            // `include()` would need the rest of the source tree, which we
            // do not have in the browser.
            Err(SourceLoadError::new(
                path.to_path_buf(),
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "include() is not supported while shaping from memory",
                ),
            ))
        }
    }
}

const FEA_ROOT: &str = "features.fea";

impl ShapingFont {
    /// Use a complete compiled font and its actual post-table glyph order.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        use skrifa::{MetadataProvider as _, raw::TableProvider as _};
        let font = skrifa::FontRef::new(&bytes).map_err(|error| error.to_string())?;
        let count = font.maxp().map_err(|error| error.to_string())?.num_glyphs();
        // Read the names in one pass. Asking for one name at a time scans the `post` table's
        // strings from the start for each glyph, which costs glyphs squared.
        let mut found: Vec<Option<String>> = vec![None; usize::from(count)];
        for (id, name) in font.glyph_names().iter() {
            if let Some(slot) = usize::try_from(id.to_u32())
                .ok()
                .and_then(|index| found.get_mut(index))
            {
                *slot = Some(name.to_string());
            }
        }
        let names = found
            .into_iter()
            .enumerate()
            .map(|(id, name)| {
                name.ok_or_else(|| format!("compiled font has no name for glyph {id}"))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            bytes,
            names,
            normalized: Vec::new(),
        })
    }

    /// Set normalized design coordinates in the font's axis order.
    /// Shaping uses the same location as the variable outline preview.
    pub fn at_normalized(mut self, normalized: Vec<f64>) -> Self {
        self.normalized = normalized;
        self
    }

    /// Names in compiled glyph-id order, including substitutions and `.notdef`.
    pub fn glyph_order(&self) -> &[String] {
        &self.names
    }
    /// The font as OpenType bytes: the layout tables, advances, character map and glyph names.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// The OpenType bytes of the font, borrowed.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The GDEF glyph class of a glyph by name: 1 base, 2 ligature, 3 mark, 4 component.
    /// `None` when the font has no such glyph or no GDEF glyph class table.
    pub fn glyph_class(&self, name: &str) -> Option<u16> {
        use skrifa::raw::TableProvider as _;
        let id = self.names.iter().position(|candidate| candidate == name)?;
        let id = skrifa::raw::types::GlyphId16::new(u16::try_from(id).ok()?);
        let font = skrifa::FontRef::new(&self.bytes).ok()?;
        let classes = font.gdef().ok()?.glyph_class_def()?.ok()?;
        Some(classes.get(id))
    }

    /// Compile a font for shaping.
    ///
    /// Fails when the feature file does not compile. That happens
    /// halfway through any edit, so callers are expected to keep
    /// working with whatever they had.
    pub fn build(source: &ShapingSource) -> Result<Self, String> {
        Self::build_with(source, &NopFeatureProvider)
    }

    /// Build as [`Self::build`] does, with positioning lookups from `provider` placed at the
    /// `# Automatic Code` comments of the feature file.
    pub fn build_with(
        source: &ShapingSource,
        provider: &impl fea_rs::compile::FeatureProvider,
    ) -> Result<Self, String> {
        if source.glyphs.is_empty() {
            return Err("no glyphs to shape with".into());
        }
        let glyph_map = GlyphMap::new(source.glyphs.iter().map(|g| g.name.as_str()))
            .map_err(|error| format!("glyph order: {error}"))?;

        let compilation = Compiler::<_, NopVariationInfo>::new(FEA_ROOT, &glyph_map)
            .with_feature_writer(provider)
            .with_resolver(InMemoryFea {
                root: PathBuf::from(FEA_ROOT),
                text: Arc::from(source.features.as_str()),
            })
            .compile()
            .map_err(|e| format!("features.fea: {e}"))?;
        Self::from_tables(source.units_per_em, &source.glyphs, &compilation)
    }

    /// Build a shaping font from layout tables that `fea-rs` has already compiled.
    ///
    /// `glyphs` gives the glyph order, advances and code points; `compilation` gives GSUB, GPOS
    /// and GDEF for the same glyph order.
    pub fn from_tables(
        units_per_em: f64,
        glyphs: &[ShapingGlyph],
        compilation: &fea_rs::compile::Compilation,
    ) -> Result<Self, String> {
        if glyphs.is_empty() {
            return Err("no glyphs to shape with".into());
        }
        let upem = units_per_em;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the range check above keeps it inside u16"
        )]
        let upem_u16 = if (16.0..=16384.0).contains(&upem) {
            upem as u16
        } else {
            1000
        };
        let names: Vec<String> = glyphs.iter().map(|g| g.name.clone()).collect();

        let mut builder = FontBuilder::new();

        // Header tables. Only the fields shaping reads are meaningful:
        // units per em, glyph count, and the advance widths.
        let mut head = Head {
            units_per_em: upem_u16,
            ..Default::default()
        };
        head.index_to_loc_format = 0;
        builder.add_table(&head).map_err(|e| e.to_string())?;

        let glyph_count =
            u16::try_from(glyphs.len()).map_err(|_| "more than 65535 glyphs".to_string())?;
        builder
            .add_table(&Maxp::new(glyph_count))
            .map_err(|e| e.to_string())?;

        let metrics: Vec<LongMetric> = glyphs
            .iter()
            .map(|glyph| LongMetric {
                advance: u16::try_from(round_units(glyph.advance.clamp(0.0, f64::from(u16::MAX))))
                    .unwrap_or(u16::MAX),
                side_bearing: 0,
            })
            .collect();
        let mut hhea = Hhea {
            number_of_h_metrics: glyph_count,
            ..Hhea::default()
        };
        let upem_i16 = i16::try_from(upem_u16).unwrap_or(1000);
        hhea.ascender = (upem_i16 / 5 * 4).into();
        hhea.descender = (-(upem_i16 / 5)).into();
        builder.add_table(&hhea).map_err(|e| e.to_string())?;
        builder
            .add_table(&Hmtx::new(metrics, Vec::new()))
            .map_err(|e| e.to_string())?;

        let mappings: Vec<(char, GlyphId)> = glyphs
            .iter()
            .enumerate()
            .flat_map(|(gid, glyph)| {
                glyph.unicodes.iter().filter_map(move |cp| {
                    let gid = u32::try_from(gid).ok()?;
                    char::from_u32(*cp).map(|ch| (ch, GlyphId::new(gid)))
                })
            })
            .collect();
        builder
            .add_table(&Cmap::from_mappings(mappings).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;

        if let Some(gsub) = compilation.gsub.as_ref() {
            builder.add_table(gsub).map_err(|e| e.to_string())?;
        }
        if let Some(gpos) = compilation.gpos.as_ref() {
            builder.add_table(gpos).map_err(|e| e.to_string())?;
        }
        if let Some(gdef) = compilation.gdef.as_ref() {
            builder.add_table(gdef).map_err(|e| e.to_string())?;
        }

        // The glyph names go into the font, so a reader of the bytes alone gets them back.
        builder
            .add_table(&write_fonts::tables::post::Post::new_v2(
                names.iter().map(String::as_str),
            ))
            .map_err(|e| e.to_string())?;

        Ok(Self {
            bytes: builder.build(),
            names,
            normalized: Vec::new(),
        })
    }

    /// Shape one run of text in one direction.
    pub fn shape(&self, text: &str, right_to_left: bool) -> Result<Vec<ShapedGlyph>, String> {
        self.shape_with_features(text, right_to_left, &[])
    }

    /// Shape with per-feature overrides: (tag, on) pairs handed to
    /// the shaper over the whole run; tags not listed keep the
    /// shaper's defaults.
    pub fn shape_with_features(
        &self,
        text: &str,
        right_to_left: bool,
        features: &[(String, bool)],
    ) -> Result<Vec<ShapedGlyph>, String> {
        self.shape_with_options(text, right_to_left, features, None, None)
    }

    /// Shape with explicit script and language overrides on top of
    /// the feature list. `script` is an ISO 15924 tag ("arab",
    /// "latn"); `language` a BCP 47 tag ("ur", "sd"). Either may be
    /// `None` to infer the script from Unicode or use the default language. Language-specific
    /// OpenType rules (languagesystem arab URD) only fire when the
    /// language is set here.
    pub fn shape_with_options(
        &self,
        text: &str,
        right_to_left: bool,
        features: &[(String, bool)],
        script: Option<&str>,
        language: Option<&str>,
    ) -> Result<Vec<ShapedGlyph>, String> {
        let font = FontRef::new(&self.bytes).map_err(|e| format!("shaping font: {e}"))?;
        let data = ShaperData::new(&font);
        let instance = harfrust::ShaperInstance::from_coords(
            &font,
            self.normalized.iter().map(|v| {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "OpenType variation coordinates have 14 fractional bits"
                )]
                harfrust::NormalizedCoord::from_f32(*v as f32)
            }),
        );
        let shaper = data.shaper(&font).instance(Some(&instance)).build();

        let ranges = script_runs(text, right_to_left, script);
        let script = script
            .map(str::trim)
            .filter(|tag| tag.len() == 4 && tag.is_ascii())
            .and_then(|tag| tag.parse::<harfrust::Script>().ok());
        let language = language.and_then(|tag| tag.trim().parse::<harfrust::Language>().ok());
        let overrides: Vec<harfrust::Feature> = features
            .iter()
            .filter(|(tag, _)| tag.len() == 4 && tag.is_ascii())
            .map(|(tag, on)| {
                let mut bytes = [b' '; 4];
                bytes.copy_from_slice(tag.as_bytes());
                harfrust::Feature::new(harfrust::Tag::new(&bytes), u32::from(*on), ..)
            })
            .collect();

        // A bidi run can contain several scripts reading in the same direction.
        // The API returns the glyphs in visual order, with clusters in the original text.
        u32::try_from(text.len()).map_err(|_| "text exceeds the shaper's cluster range")?;
        let mut glyphs = Vec::new();
        let mut buffer = UnicodeBuffer::new();
        for range in ranges {
            let offset = u32::try_from(range.start).expect("a range starts within the text");
            buffer.set_cluster_level(harfrust::BufferClusterLevel::MonotoneCharacters);
            buffer.push_str(&text[range]);
            buffer.set_direction(if right_to_left {
                Direction::RightToLeft
            } else {
                Direction::LeftToRight
            });
            if let Some(script) = script {
                buffer.set_script(script);
            }
            buffer.guess_segment_properties();
            if let Some(lang) = &language {
                buffer.set_language(lang.clone());
            }
            let shaped = shaper.shape(buffer, &overrides);
            glyphs.extend(
                shaped
                    .glyph_infos()
                    .iter()
                    .zip(shaped.glyph_positions())
                    .map(|(info, pos)| ShapedGlyph {
                        glyph_id: u16::try_from(info.glyph_id).unwrap_or(0),
                        cluster: offset + info.cluster,
                        x_advance: pos.x_advance as f64,
                        x_offset: pos.x_offset as f64,
                        y_offset: pos.y_offset as f64,
                    }),
            );
            buffer = shaped.clear();
        }
        Ok(glyphs)
    }

    /// Glyph name for a shaped glyph id.
    pub fn glyph_name(&self, glyph_id: u16) -> Option<&str> {
        self.names.get(glyph_id as usize).map(String::as_str)
    }
}

/// The UTF-8 ranges of the script runs within one directional span, in visual order.
///
/// Common and inherited characters stay with the preceding script, or with the following
/// script at the beginning of the run. In particular, a combining mark stays with its base.
/// A four-byte script override assigns the entire span to that script.
pub fn script_runs(
    text: &str,
    right_to_left: bool,
    script: Option<&str>,
) -> Vec<std::ops::Range<usize>> {
    if script.is_some_and(|tag| tag.trim().len() == 4 && tag.trim().is_ascii()) {
        return std::iter::once(0..text.len()).collect();
    }
    let scripts = CodePointMapData::<Script>::new();
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut active = None;
    for (offset, character) in text.char_indices() {
        let script = scripts.get(character);
        if matches!(script, Script::Common | Script::Inherited | Script::Unknown) {
            continue;
        }
        if active.is_some_and(|previous| previous != script) {
            ranges.push(start..offset);
            start = offset;
        }
        active = Some(script);
    }
    ranges.push(start..text.len());
    if right_to_left {
        ranges.reverse();
    }
    ranges
}

/// The host function that receives text shaping failures.
static SHAPING_FAILURE_REPORTER: std::sync::OnceLock<fn(&str)> = std::sync::OnceLock::new();

/// Install the host function that receives text shaping failures.
///
/// The library does not print; the native editor installs a reporter that writes to standard
/// error.
/// Only the first installation takes effect, and the return value says whether this one did.
pub fn set_shaping_failure_reporter(reporter: fn(&str)) -> bool {
    SHAPING_FAILURE_REPORTER.set(reporter).is_ok()
}

/// Pass a failed feature-file compile or outline read to the installed reporter.
///
/// The failure is not fatal: a feature file does not compile halfway through an edit, and typing
/// has to keep working.
/// The text buffer rebuilds its shaping font after each clone, so a message that repeats the
/// previous one is not reported again.
/// Without an installed reporter the failure is discarded.
pub fn log_shaping_failure(message: &str) {
    let Some(report) = SHAPING_FAILURE_REPORTER.get() else {
        return;
    };
    static LAST: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    let mut last = LAST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if record_new_failure(&mut last, message) {
        report(message);
    }
}

/// Store `message` as the last reported failure, and return whether it differs from the previous one.
fn record_new_failure(last: &mut Option<String>, message: &str) -> bool {
    if last.as_deref() == Some(message) {
        return false;
    }
    *last = Some(message.to_owned());
    true
}

#[cfg(test)]
mod failure_report_tests {
    use super::record_new_failure;

    #[test]
    fn a_repeated_failure_is_reported_once() {
        let mut last = None;
        assert!(record_new_failure(&mut last, "feature file: syntax error"));
        assert!(!record_new_failure(&mut last, "feature file: syntax error"));
        assert!(record_new_failure(&mut last, "feature file: unknown glyph"));
        assert!(record_new_failure(&mut last, "feature file: syntax error"));
    }
}

#[cfg(test)]
#[cfg(feature = "integration-fonts")]
mod tests {
    use super::*;

    /// Build a shaping source out of the pinned integration UFO: its glyph
    /// order, advances, codepoints and features.fea, exactly as the
    /// editor would hand them over.
    fn virtua_grotesk() -> ShapingSource {
        let ufo_dir = crate::testing::fonts::virtua_regular_ufo();
        let font = norad::Font::load(&ufo_dir).expect("test UFO loads");
        let features = std::fs::read_to_string(ufo_dir.join("features.fea"))
            .expect("test UFO has features.fea");

        // .notdef first, then the font's own glyph order.
        let order: Vec<String> = std::iter::once(".notdef".to_string())
            .chain(
                font.layers
                    .default_layer()
                    .iter()
                    .map(|glyph| glyph.name().to_string())
                    .filter(|name| name != ".notdef"),
            )
            .collect();

        let glyphs = order
            .iter()
            .map(|name| {
                let glyph = font.layers.default_layer().get_glyph(name.as_str());
                ShapingGlyph {
                    name: name.clone(),
                    advance: glyph.map(|g| g.width).unwrap_or(0.0),
                    unicodes: glyph
                        .map(|g| g.codepoints.iter().map(|c| c as u32).collect())
                        .unwrap_or_default(),
                }
            })
            .collect();

        ShapingSource {
            units_per_em: font
                .font_info
                .units_per_em
                .map(|upem| *upem)
                .unwrap_or(1000.0),
            glyphs,
            features,
        }
    }

    fn shaped_names(font: &ShapingFont, text: &str, rtl: bool) -> Vec<String> {
        font.shape(text, rtl)
            .expect("shaping succeeds")
            .iter()
            .map(|g| font.glyph_name(g.glyph_id).unwrap_or("?").to_string())
            .collect()
    }

    #[test]
    fn language_override_fires_language_specific_rules() {
        let mut src = virtua_grotesk();
        // A locl rule that only applies for Urdu: alef becomes the
        // lam_alef glyph (visually silly, unambiguous in a test).
        // languagesystem statements must precede every feature
        // block, so lift the originals out and re-emit them first.
        let (systems, rest): (Vec<&str>, Vec<&str>) = src
            .features
            .lines()
            .partition(|l| l.trim_start().starts_with("languagesystem"));
        src.features = format!(
            "{}\nlanguagesystem arab URD;\n{}\n\
             feature locl {{\n\
             script arab;\n\
             language URD;\n\
             sub alef-ar by lam_alef-ar;\n\
             }} locl;\n",
            systems.join("\n"),
            rest.join("\n")
        );
        let font = ShapingFont::build(&src).expect("shaping font builds");
        let urdu = font
            .shape_with_options("\u{0627}", true, &[], Some("arab"), Some("ur"))
            .expect("shaping succeeds");
        let names: Vec<_> = urdu
            .iter()
            .map(|g| font.glyph_name(g.glyph_id).unwrap_or("?"))
            .collect();
        assert_eq!(names, ["lam_alef-ar"]);
        // Without the language, the default rules leave alef alone.
        assert_eq!(shaped_names(&font, "\u{0627}", true), ["alef-ar"]);
    }

    #[test]
    fn lam_alef_shapes_to_the_ligature() {
        let font = ShapingFont::build(&virtua_grotesk()).expect("shaping font builds");
        // لا — the required ligature, which positional forms alone cannot
        // produce. It is one glyph, not two.
        assert_eq!(
            shaped_names(&font, "\u{0644}\u{0627}", true),
            ["lam_alef-ar"]
        );
    }

    #[test]
    fn arabic_positional_forms_come_from_the_font() {
        let font = ShapingFont::build(&virtua_grotesk()).expect("shaping font builds");
        // بب — the font's own init/fina features choose the forms, not
        // our joining table. Glyphs come back in visual order, so for RTL
        // the final form (leftmost) is first.
        assert_eq!(
            shaped_names(&font, "\u{0628}\u{0628}", true),
            ["beh-ar.fina", "beh-ar.init"]
        );
    }

    #[test]
    fn calt_fires_so_contextual_alternates_are_visible_in_the_editor() {
        // Arabic dot placement depends on what follows: a beh before alef
        // wants its dot centred. Virtua Grotesk writes that as a calt
        // rule (beh-ar.init before alef becomes beh-ar.init.ctr), and it
        // is only worth writing if the editor applies it. Otherwise the
        // font would look right everywhere except where it is drawn.
        let font = ShapingFont::build(&virtua_grotesk()).expect("shaping font builds");
        // با: visual order, so the alef comes back first.
        assert_eq!(
            shaped_names(&font, "\u{0628}\u{0627}", true),
            ["alef-ar.fina", "beh-ar.init.ctr"],
            "calt did not fire: contextual alternates would be invisible here"
        );
    }

    #[test]
    fn latin_shapes_one_glyph_per_character() {
        let font = ShapingFont::build(&virtua_grotesk()).expect("shaping font builds");
        assert_eq!(shaped_names(&font, "Ab", false), ["A", "b"]);
    }

    #[test]
    fn shaped_glyphs_carry_advances_and_clusters() {
        let font = ShapingFont::build(&virtua_grotesk()).expect("shaping font builds");
        let shaped = font
            .shape("\u{0644}\u{0627}", true)
            .expect("shaping succeeds");
        assert_eq!(shaped.len(), 1);
        // The ligature stands for both characters, and reports the
        // cluster of the first.
        assert_eq!(shaped[0].cluster, 0);
        assert!(shaped[0].x_advance > 0.0);
    }

    #[test]
    fn a_broken_feature_file_is_an_error_not_a_panic() {
        let mut source = virtua_grotesk();
        source.features = "feature liga { sub nonexistent by alsoMissing; } liga;".into();
        assert!(ShapingFont::build(&source).is_err());
    }
}
