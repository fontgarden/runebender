// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! In-memory OpenType compilation of the live variable document.
//!
//! Preview and export share Babelfont's fontc source. No saved UFO, subprocess,
//! Python environment or repository build script participates in this pipeline.

#[cfg(test)]
mod arabic_fixture_tests;
mod hoi_export;
mod metadata;
mod preview_tolerance;
pub use hoi_export::{HoiBakeOptions, HoiBakeReport};
pub(crate) use metadata::glyph_category_from_values;
pub use preview_tolerance::{PreviewChange, PreviewException};
pub mod proof;
#[cfg(not(target_arch = "wasm32"))]
pub mod proof_jobs;

use std::path::PathBuf;
use std::sync::Arc;

use babelfont::convertors::fontir::{BabelfontIrSource, CompilationOptions};
use fontdrasil::coords::{DesignCoord, DesignLocation};

use super::project::Project;
use super::variable::{LayerId, SourceId};
use crate::text::shape::ShapingFont;

type CompileResult = Result<Arc<CompiledFont>, String>;

/// The revision and the designspace structure a compiled font was made from.
type CompileKey = (
    u64,
    Option<super::model::designspace::CanonicalCompilerStructure>,
);

#[derive(Debug, Default)]
pub(super) struct CompileCache {
    key: Option<CompileKey>,
    result: Option<Result<Arc<CompiledFont>, String>>,
    /// What the snapshot of `result` changed so that it compiles; see
    /// [`Project::preview_exceptions`].
    exceptions: Arc<[PreviewException]>,
    pending: Option<PreviewJob>,
    /// Set while a pointer gesture is in progress; see [`Project::preview_hold`].
    hold: Arc<std::sync::atomic::AtomicBool>,
    /// When the sources last changed while a preview was wanted; the snapshot and compile
    /// wait until the edits pause.
    #[cfg(not(target_arch = "wasm32"))]
    wanted: Option<std::time::Instant>,
}

/// How long the sources have to stay unchanged before a preview compile starts.
///
/// A drag ends with a commit, and the next one often follows within a second. Compiling
/// after every commit would take the snapshot on the UI thread each time and keep a compiler
/// thread busy while the next drag wants the frame budget.
#[cfg(not(target_arch = "wasm32"))]
const PREVIEW_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);

struct CompileSourceInput {
    id: SourceId,
    name: String,
    location: DesignLocation,
    path: PathBuf,
    default_layer: LayerId,
    is_default: bool,
}

fn canonical_design_location(
    axes: &[super::model::designspace::CanonicalAxis],
    location: &super::model::designspace::CanonicalLocation,
) -> Result<DesignLocation, String> {
    axes.iter()
        .map(|axis| {
            let tag = babelfont::Tag::new(
                axis.coordinates
                    .tag
                    .as_bytes()
                    .try_into()
                    .map_err(|_| "invalid axis tag")?,
            );
            Ok((tag, DesignCoord::new(location.design(axis))))
        })
        .collect()
}

/// One background compilation. Completion is polled by the host's event loop.
#[derive(Clone, Debug)]
pub struct PreviewJob {
    finished: Arc<std::sync::Mutex<Option<CompileResult>>>,
    /// What the snapshot of this job changed so that it compiles.
    exceptions: Arc<[PreviewException]>,
}

impl PreviewJob {
    /// Whether this worker has posted its result, including compile errors.
    pub fn is_finished(&self) -> bool {
        self.finished
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_some()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start(font: babelfont::Font, exceptions: Arc<[PreviewException]>) -> Self {
        let job = Self {
            finished: Arc::new(std::sync::Mutex::new(None)),
            exceptions,
        };
        let output = job.finished.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                CompiledFont::build(font)
            }))
            .map_err(|_| {
                "The font compiler stopped unexpectedly; check source compatibility".to_string()
            })
            .and_then(|result| result)
            .map(Arc::new);
            *output.lock().unwrap_or_else(|error| error.into_inner()) = Some(result);
        });
        job
    }
}

fn normalized_coords(normalized: &[f64]) -> Vec<skrifa::instance::NormalizedCoord> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "OpenType coordinates use 14 fractional bits"
    )]
    normalized
        .iter()
        .map(|value| skrifa::instance::NormalizedCoord::from_f32(*value as f32))
        .collect()
}

/// Draw the glyph with this index, unhinted and in font units; a glyph without an outline
/// gives an empty path.
fn draw_glyph(
    outlines: &skrifa::outline::OutlineGlyphCollection<'_>,
    id: u32,
    coords: &[skrifa::instance::NormalizedCoord],
) -> Result<kurbo::BezPath, String> {
    use skrifa::{
        instance::{LocationRef, Size},
        outline::{DrawSettings, OutlinePen},
    };
    #[derive(Default)]
    struct Pen(kurbo::BezPath);
    impl OutlinePen for Pen {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.move_to((f64::from(x), f64::from(y)));
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.line_to((f64::from(x), f64::from(y)));
        }
        fn quad_to(&mut self, x: f32, y: f32, x1: f32, y1: f32) {
            self.0
                .quad_to((f64::from(x), f64::from(y)), (f64::from(x1), f64::from(y1)));
        }
        fn curve_to(&mut self, x: f32, y: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
            self.0.curve_to(
                (f64::from(x), f64::from(y)),
                (f64::from(x1), f64::from(y1)),
                (f64::from(x2), f64::from(y2)),
            );
        }
        fn close(&mut self) {
            self.0.close_path();
        }
    }
    let mut pen = Pen::default();
    if let Some(glyph) = outlines.get(skrifa::GlyphId::new(id)) {
        glyph
            .draw(
                DrawSettings::unhinted(Size::unscaled(), LocationRef::new(coords)),
                &mut pen,
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(pen.0)
}

/// A compiled, immutable revision, shared by shaping and binary export.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledFont {
    /// Complete OpenType font, including variable outlines and positioning.
    pub bytes: Arc<Vec<u8>>,
    /// Glyph names in the compiler's final glyph order.
    pub glyph_order: Vec<String>,
    /// Axis tags in the compiled font's coordinate order.
    pub axis_tags: Vec<String>,
    /// Each glyph's index in `glyph_order`, for drawing one glyph by name.
    pub glyph_ids: std::collections::HashMap<String, u32>,
}

impl CompiledFont {
    /// Compiled advances, including HVAR, for encoded and unencoded glyphs.
    pub fn advances(&self, normalized: &[f64]) -> Result<Vec<(String, f64)>, String> {
        use skrifa::{
            MetadataProvider as _,
            instance::{LocationRef, Size},
        };
        let font = skrifa::FontRef::new(&self.bytes).map_err(|error| error.to_string())?;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "OpenType coordinates use 14 fractional bits"
        )]
        let coords: Vec<_> = normalized
            .iter()
            .map(|value| skrifa::instance::NormalizedCoord::from_f32(*value as f32))
            .collect();
        let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::new(&coords));
        self.glyph_order
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let id =
                    skrifa::GlyphId::new(u32::try_from(index).map_err(|_| "glyph id overflow")?);
                Ok((
                    name.clone(),
                    f64::from(metrics.advance_width(id).unwrap_or(0.0)),
                ))
            })
            .collect()
    }

    /// Draw one glyph's compiled outline at a normalized location.
    ///
    /// `Ok(None)` when the font has no glyph of that name.
    /// Text draws only the glyphs it shows, so this avoids drawing the whole font after
    /// every compile; use [`Self::outlines`] when every glyph is wanted.
    pub fn outline(
        &self,
        name: &str,
        normalized: &[f64],
    ) -> Result<Option<kurbo::BezPath>, String> {
        let Some(&id) = self.glyph_ids.get(name) else {
            return Ok(None);
        };
        // Reading the table directory again for each glyph is small beside drawing it.
        let font = skrifa::FontRef::new(&self.bytes).map_err(|error| error.to_string())?;
        use skrifa::MetadataProvider as _;
        let coords = normalized_coords(normalized);
        draw_glyph(&font.outline_glyphs(), id, &coords).map(Some)
    }

    /// Draw the compiled outlines at the same normalized location as shaping.
    pub fn outlines(
        &self,
        normalized: &[f64],
    ) -> Result<Vec<(String, Arc<kurbo::BezPath>)>, String> {
        use skrifa::MetadataProvider as _;
        let font = skrifa::FontRef::new(&self.bytes).map_err(|error| error.to_string())?;
        let coords = normalized_coords(normalized);
        let outlines = font.outline_glyphs();
        self.glyph_order
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let id = u32::try_from(index).map_err(|_| "glyph id overflow")?;
                Ok((name.clone(), Arc::new(draw_glyph(&outlines, id, &coords)?)))
            })
            .collect()
    }

    /// Compile the same snapshot used for preview or export.
    /// `head.modified` uses the captured creation time, not the export wall clock.
    pub fn build(font: babelfont::Font) -> Result<Self, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Self::build_inner(font)))
                .map_err(|_| {
                    "The font compiler stopped unexpectedly; check source compatibility".to_string()
                })?
        }
        #[cfg(target_arch = "wasm32")]
        Self::build_inner(font)
    }

    fn build_inner(mut font: babelfont::Font) -> Result<Self, String> {
        if font.masters.is_empty() || font.glyphs.iter().next().is_none() {
            return Err("the font needs a source and at least one glyph".into());
        }
        let options = CompilationOptions {
            dont_use_production_names: true,
            produce_varc_table: false,
            ..CompilationOptions::default()
        };
        // This snapshot already represents UFO semantics. The convenience compiler
        // applies Glyphs feature rewrites whose AST cannot parse fractional rule
        // conditions. Preserve raw FEA and pass the Babelfont IR source to fontc.
        use babelfont::filters::{FontFilter as _, RetainGlyphs};
        let features = std::mem::take(&mut font.features);
        let names = font
            .glyphs
            .iter()
            .filter(|g| g.exported)
            .map(|g| g.name.to_string())
            .collect();
        RetainGlyphs::new(names)
            .apply(&mut font)
            .map_err(|error| error.to_string())?;
        font.features = features;
        let source = BabelfontIrSource::new(font, options);
        let bytes = fontc::generate_font(Box::new(source), fontc::Options::default())
            .map_err(|error| one_line(&error.to_string()))?;
        let bytes = stable_modified_timestamp(bytes)?;
        let shaping = ShapingFont::from_bytes(bytes.clone())?;
        use skrifa::MetadataProvider as _;
        let axis_tags = skrifa::FontRef::new(&bytes)
            .map_err(|error| error.to_string())?
            .axes()
            .iter()
            .map(|axis| axis.tag().to_string())
            .collect();
        let glyph_order = shaping.glyph_order().to_vec();
        let glyph_ids = glyph_order
            .iter()
            .enumerate()
            .filter_map(|(index, name)| Some((name.clone(), u32::try_from(index).ok()?)))
            .collect();
        Ok(Self {
            bytes: Arc::new(bytes),
            glyph_order,
            axis_tags,
            glyph_ids,
        })
    }
}

/// A compiler message on one line, for a panel row or the status bar.
///
/// A feature-file error can span several lines; its first line names the fault,
/// and the count of the other lines tells the reader that more text exists.
fn one_line(message: &str) -> String {
    let mut lines = message
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let first = lines.next().unwrap_or("the compiler failed").to_string();
    match lines.count() {
        0 => first,
        more => format!("{first} (and {more} more lines)"),
    }
}

#[cfg(test)]
#[test]
fn a_compiler_message_is_one_line() {
    assert_eq!(
        one_line("'tet-hb' has interpolation-incompatible paths"),
        "'tet-hb' has interpolation-incompatible paths"
    );
    assert_eq!(
        one_line("feature error\n  at line 3\n\n  at line 9\n"),
        "feature error (and 2 more lines)"
    );
    assert_eq!(one_line(""), "the compiler failed");
}

#[cfg(test)]
#[test]
fn the_layout_revision_ignores_point_moves_and_outline_compatibility() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core/Core.designspace");
    let mut project = Project::load(&path).unwrap();
    let mut sources = project.document_sources();
    let regular = sources.next().unwrap().default_layer();
    let bold = sources.next().unwrap().default_layer();
    drop(sources);
    let glyph = project
        .glyph_names()
        .find(|name| {
            project
                .document_layer(name, &regular)
                .is_some_and(|layer| layer.contours().next().is_some())
        })
        .expect("a glyph with contours")
        .to_string();
    let layout = project.layout_revision();
    let revision = project.document_revision();

    // A point move is a new document revision, but not a new layout.
    let point = project
        .document_layer(&glyph, &regular)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .next()
        .unwrap();
    let (id, at) = (point.id(), point.position());
    project
        .edit_document_layer(&glyph, &regular, |draft| {
            draft.set_point_position(id, at + kurbo::Vec2::new(8.0, 0.0))?;
            Ok(())
        })
        .unwrap();
    assert_ne!(project.document_revision(), revision);
    assert_eq!(project.layout_revision(), layout);

    // Contours that no longer interpolate stop the complete font, not the layout.
    project
        .edit_document_layer(&glyph, &bold, |draft| {
            draft.clear_contours();
            Ok(())
        })
        .unwrap();
    assert!(project.compile().is_err());
    assert_eq!(project.layout_revision(), layout);

    // A width edit changes layout.
    project
        .edit_document_layer(&glyph, &regular, |draft| {
            draft.set_width(777.0)?;
            Ok(())
        })
        .unwrap();
    assert_ne!(project.layout_revision(), layout);
}

/// Keep a compiled snapshot independent of the second when fontc finishes.
///
/// fontbe stamps `head.modified` with the wall clock. The creation time comes from the captured
/// Babelfont source and remains unchanged; using it for `modified` makes preview and export bytes
/// reproducible for the same snapshot. Repair both OpenType checksums after changing the header.
fn stable_modified_timestamp(mut bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let (head_offset, head_length, record_index) = {
        let font = skrifa::raw::FontRef::new(&bytes).map_err(|error| error.to_string())?;
        let (index, record) = font
            .table_directory()
            .table_records()
            .iter()
            .enumerate()
            .find(|(_, record)| record.tag() == write_fonts::types::Tag::new(b"head"))
            .ok_or("compiled font has no head table")?;
        (
            usize::try_from(record.offset()).map_err(|_| "head offset exceeds platform size")?,
            usize::try_from(record.length()).map_err(|_| "head length exceeds platform size")?,
            index,
        )
    };
    let head_end = head_offset
        .checked_add(head_length)
        .ok_or("compiled head table exceeds font data")?;
    if head_length < 36 || bytes.get(head_offset..head_end).is_none() {
        return Err("compiled font has a truncated head table".into());
    }
    let record_checksum_offset = 12_usize
        .checked_add(
            record_index
                .checked_mul(16)
                .ok_or("compiled table directory is too large")?,
        )
        .and_then(|offset| offset.checked_add(4))
        .ok_or("compiled table directory is too large")?;
    if bytes
        .get(record_checksum_offset..record_checksum_offset + 4)
        .is_none()
    {
        return Err("compiled font has a truncated table directory".into());
    }

    let created: [u8; 8] = bytes[head_offset + 20..head_offset + 28]
        .try_into()
        .expect("validated head table contains creation time");
    if bytes[head_offset + 28..head_offset + 36] == created {
        return Ok(bytes);
    }
    bytes[head_offset + 28..head_offset + 36].copy_from_slice(&created);
    bytes[head_offset + 8..head_offset + 12].fill(0);
    let head_checksum = skrifa::raw::tables::compute_checksum(&bytes[head_offset..head_end]);
    bytes[record_checksum_offset..record_checksum_offset + 4]
        .copy_from_slice(&head_checksum.to_be_bytes());
    let font_checksum = skrifa::raw::tables::compute_checksum(&bytes);
    let adjustment = 0xB1B0_AFBA_u32.wrapping_sub(font_checksum);
    bytes[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    Ok(bytes)
}

impl Project {
    /// A Babelfont snapshot of all live source layers, including unsaved edits.
    /// UFO-only metadata remains in the document's persistence adapter.
    pub fn babelfont_snapshot(&self) -> Result<babelfont::Font, String> {
        self.snapshot_with_features(None)
    }

    /// Check a feature draft with all live masters, axes and Designspace rules.
    /// This does not apply the draft or change the cached preview revision.
    pub fn check_features(&self, features: &str) -> Result<(), String> {
        CompiledFont::build(self.snapshot_with_features(Some(features))?).map(|_| ())
    }

    fn snapshot_with_features(
        &self,
        feature_text: Option<&str>,
    ) -> Result<babelfont::Font, String> {
        let mut font = self.source_snapshot_with_features(feature_text)?;
        self.propagate_component_anchors(&mut font);
        self.bake_hoi_snapshot(&mut font, HoiBakeOptions::default())?;
        Ok(font)
    }

    /// Give each composite without anchors the anchors of its components.
    ///
    /// fontc reads only the anchors a glyph has itself for UFO sources, so a letter built from
    /// a base and a dot would take no marks in the compiled font.
    /// The editor and the text line use [`effective_document_anchors`]; the compiled font
    /// gets the same anchors here, so both attach marks in the same way.
    ///
    /// [`effective_document_anchors`]: super::composites::effective_document_anchors
    fn propagate_component_anchors(&self, font: &mut babelfont::Font) {
        for source in self.document_sources() {
            let layer_id = source.default_layer();
            let key = super::babelfont::layer_key(&layer_id);
            for glyph in &mut font.glyphs.0 {
                let Some(view) = self.document_layer(&glyph.name, &layer_id) else {
                    continue;
                };
                if view.anchors().next().is_some() || view.components().next().is_none() {
                    continue;
                }
                let anchors = super::composites::effective_document_anchors(view, |name| {
                    self.document_layer(name, &layer_id)
                });
                if let Some(layer) = glyph.get_layer_mut(&key) {
                    layer.anchors = anchors
                        .into_iter()
                        .map(|(name, point)| babelfont::Anchor {
                            x: point.x,
                            y: point.y,
                            name,
                            ..babelfont::Anchor::default()
                        })
                        .collect();
                }
            }
        }
    }

    fn source_snapshot_with_features(
        &self,
        feature_text: Option<&str>,
    ) -> Result<babelfont::Font, String> {
        let mut font = self.variable_font().clone();
        let structure = self.compiler_structure();
        let axes = structure
            .as_ref()
            .map(|structure| structure.axes.as_slice())
            .unwrap_or_default();
        font.axes = axes
            .iter()
            .map(|axis| {
                let mut backend = axis.coordinates.backend()?;
                backend.hidden = axis.hidden;
                for label in &axis.labels {
                    backend
                        .name
                        .insert(label.language.clone(), label.value.clone());
                }
                Ok::<_, String>(backend)
            })
            .collect::<Result<_, _>>()?;
        let compile_sources = if let Some(structure) = &structure {
            structure
                .sources
                .iter()
                .map(|source| {
                    let normalized = source.location.to_normalized(&structure.axes)?;
                    let resolved_path = self
                        .document_source_path(source.id())
                        .ok_or("canonical source identity is not loaded")?
                        .to_path_buf();
                    Ok(CompileSourceInput {
                        id: source.id(),
                        name: source.display_name().to_owned(),
                        location: canonical_design_location(&structure.axes, &source.location)?,
                        path: resolved_path,
                        default_layer: source.default_layer.clone(),
                        is_default: normalized.values().all(|value| value.abs() < 1e-9),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
        } else {
            let default = self.default_source_index();
            self.document_sources()
                .enumerate()
                .map(|(index, source)| {
                    Ok(CompileSourceInput {
                        id: source.id(),
                        name: source.name().to_owned(),
                        location: DesignLocation::default(),
                        path: source.path().to_path_buf(),
                        default_layer: source.default_layer(),
                        is_default: index == default,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
        };
        let default_source = compile_sources
            .iter()
            .find(|source| source.is_default)
            .ok_or("designspace has no default source")?;
        let default_id = default_source.id;
        let info = self
            .document_font_info(default_id)
            .expect("default source retains canonical font info");
        font.upm = metadata::units_per_em(info.metrics.units_per_em.unwrap_or(1000.0))?;
        font.names.family_name = info
            .names
            .family_name
            .as_deref()
            .unwrap_or("Untitled")
            .into();
        font.names.preferred_subfamily_name =
            info.names.style_name.as_deref().unwrap_or("Regular").into();
        let features = feature_text
            .unwrap_or(
                self.document_feature_text(default_id)
                    .expect("default source metadata"),
            )
            .lines()
            .filter(|line| line.trim() != crate::text::features::INCLUDE_LINE)
            .collect::<Vec<_>>()
            .join("\n");
        font.features = babelfont::Features::from_fea(&features);
        metadata::apply(&mut font, info)?;
        metadata::rules(structure.as_ref(), &mut font)?;
        font.source = Some(default_source.path.join("features.fea"));
        font.masters.clear();
        let default_metadata = self
            .document_font_metadata(default_id)
            .expect("default source metadata");
        metadata::apply_groups(
            &mut font,
            default_metadata
                .groups()
                .iter()
                .map(|(name, members)| (name.clone(), members.clone())),
        );
        for source in &compile_sources {
            let source_id = source.id;
            let mut master = babelfont::Master::new(
                &source.name,
                source_id.0.to_string(),
                source.location.clone(),
            );
            let info = self
                .document_font_info(source_id)
                .expect("source identity retains canonical font info");
            metadata::metrics(&mut master, info)?;
            for (key, value) in [
                (babelfont::MetricType::Ascender, info.metrics.ascender),
                (babelfont::MetricType::Descender, info.metrics.descender),
                (babelfont::MetricType::XHeight, info.metrics.x_height),
                (babelfont::MetricType::CapHeight, info.metrics.cap_height),
                (
                    babelfont::MetricType::ItalicAngle,
                    info.metrics.italic_angle,
                ),
            ] {
                if let Some(value) = value {
                    let name = format!("{key:?}");
                    master.metrics.insert(key, metadata::metric(&name, value)?);
                }
            }
            metadata::apply_kerning(
                &mut master,
                self.document_font_metadata(source_id)
                    .expect("source metadata")
                    .kerning_pairs()
                    .map(|(left, right, value)| {
                        (
                            left.as_raw_name().to_owned(),
                            right.as_raw_name().to_owned(),
                            value,
                        )
                    }),
            )?;
            font.masters.push(master);
        }
        for glyph in &mut font.glyphs.0 {
            let sources = self.glyph_sources(&glyph.name)?;
            let source_glyph_metadata =
                self.document_source_glyph_metadata(default_id, &glyph.name);
            glyph.exported = !sources.is_empty()
                && source_glyph_metadata.is_none_or(|metadata| metadata.exported());
            for source in sources {
                let key = super::babelfont::layer_key(&source.layer);
                if let Some(sparse) = structure.as_ref().and_then(|structure| {
                    structure
                        .sparse_sources
                        .iter()
                        .find(|sparse| sparse.layer == source.layer)
                }) && let Some(layer) = glyph.get_layer_mut(&key)
                {
                    layer.location = Some(canonical_design_location(
                        &structure.as_ref().expect("sparse source structure").axes,
                        &sparse.location,
                    )?);
                }
            }
            if let Some(layer) = self.document_layer(&glyph.name, &default_source.default_layer) {
                glyph.codepoints = layer.codepoints().map(u32::from).collect();
                let explicit_category =
                    source_glyph_metadata.and_then(|metadata| metadata.category());
                glyph.category = glyph_category_from_values(
                    &glyph.name,
                    explicit_category,
                    layer.codepoints(),
                    layer.anchors().map(|anchor| anchor.name()),
                )?;
            }
        }
        font.instances = if let Some(structure) = &structure {
            structure
                .instances
                .iter()
                .map(|instance| {
                    Ok(babelfont::Instance {
                        id: format!("instance-{}", instance.id().get()),
                        name: instance.display_name().into(),
                        location: canonical_design_location(&structure.axes, &instance.location)?,
                        ..babelfont::Instance::default()
                    })
                })
                .collect::<Result<_, String>>()?
        } else {
            Vec::new()
        };
        Ok(font)
    }

    /// Compile current unsaved sources into a complete static or variable TTF.
    pub fn compile(&self) -> Result<CompiledFont, String> {
        CompiledFont::build(self.babelfont_snapshot()?)
    }

    /// The snapshot of the background preview compile, and what it changed so that it compiles.
    ///
    /// This is [`Self::babelfont_snapshot`] with the contours that do not interpolate removed,
    /// and with each anchor that the default master lacks removed; see the module
    /// `preview_tolerance`. One unfinished glyph then does not take interpolated outlines,
    /// proofs and the axes panel from the whole font. Export uses the complete snapshot and
    /// still reports the precise failure.
    pub fn preview_snapshot(&self) -> Result<(babelfont::Font, Vec<PreviewException>), String> {
        let mut font = self.babelfont_snapshot()?;
        let exceptions = preview_tolerance::make_tolerant(&mut font)?;
        Ok((font, exceptions))
    }

    /// The glyphs that the current compiled preview shows differently from their sources, in
    /// glyph name order.
    ///
    /// The list belongs to the result of [`Self::compiled_preview`] or
    /// [`Self::request_preview`], and it is empty while no preview result is ready.
    pub fn preview_exceptions(&self) -> Arc<[PreviewException]> {
        self.compile_cache().exceptions.clone()
    }

    fn compile_key(&self) -> CompileKey {
        (self.variable.revision, self.compiler_structure())
    }

    fn compile_cache(&self) -> std::sync::MutexGuard<'_, CompileCache> {
        self.variable
            .compiled
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// Reuse a compiled revision for headless callers and deterministic proofs.
    pub fn compiled_preview(&self) -> Result<Arc<CompiledFont>, String> {
        let key = self.compile_key();
        let mut cache = self.compile_cache();
        if cache.key.as_ref() != Some(&key) || cache.result.is_none() {
            let (result, exceptions) = match self.preview_snapshot() {
                Ok((font, exceptions)) => {
                    (CompiledFont::build(font).map(Arc::new), exceptions.into())
                }
                Err(error) => (Err(error), Arc::from([])),
            };
            cache.result = Some(result);
            cache.exceptions = exceptions;
            cache.key = Some(key);
            cache.pending = None;
        }
        cache.result.as_ref().expect("compiled revision").clone()
    }

    /// The flag that postpones the snapshot for the preview compile.
    ///
    /// A snapshot copies the whole font on the calling thread, which is the interface
    /// thread. The editor sets the flag while a pointer gesture is in progress, so the copy
    /// does not take frames from a drag. [`Self::request_preview`] takes the snapshot on its
    /// first call after the flag is cleared.
    pub fn preview_hold(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.compile_cache().hold.clone()
    }

    /// Request the latest preview without blocking the desktop event loop.
    /// One compile runs at a time. Edits during a compile wait for it to end, then one
    /// snapshot of the latest sources starts the next compile; stale results never publish.
    /// Browser hosts currently compile synchronously because Rust threads are unavailable.
    pub fn request_preview(&self) -> Result<Option<Arc<CompiledFont>>, String> {
        #[cfg(target_arch = "wasm32")]
        {
            self.compiled_preview().map(Some)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let key = self.compile_key();
            let mut cache = self.compile_cache();
            let hold = cache.hold.load(std::sync::atomic::Ordering::Relaxed);
            if cache.key.as_ref() != Some(&key) {
                cache.result = None;
                cache.exceptions = Arc::from([]);
                cache.key = Some(key);
                cache.wanted = Some(std::time::Instant::now());
            }
            if cache.pending.as_ref().is_some_and(PreviewJob::is_finished) {
                let job = cache.pending.take().expect("finished job");
                let result = job
                    .finished
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                    .expect("finished result");
                if cache.wanted.is_none() {
                    // A pending debounce means the key changed after this job started,
                    // so its result belongs to an older revision.
                    cache.result = Some(result);
                    cache.exceptions = job.exceptions;
                }
            }
            // The snapshot copies the whole font on this thread. Take it only when no
            // compile is running, so one compile needs one copy, and never during a pointer
            // gesture.
            if cache.pending.is_none()
                && !hold
                && cache
                    .wanted
                    .is_some_and(|since| since.elapsed() >= PREVIEW_DEBOUNCE)
            {
                cache.wanted = None;
                match self.preview_snapshot() {
                    Ok((font, exceptions)) => {
                        cache.pending = Some(PreviewJob::start(font, exceptions.into()));
                    }
                    Err(error) => cache.result = Some(Err(error)),
                }
            }
            cache.result.clone().transpose()
        }
    }

    /// The current background job, if the UI should arrange a completion wakeup.
    pub fn preview_job(&self) -> Option<PreviewJob> {
        self.compile_cache().pending.clone()
    }

    /// Whether a preview is still on its way: a compile is running, or edits are waiting
    /// for their pause before one starts. The UI polls [`Self::request_preview`] while true.
    pub fn preview_pending(&self) -> bool {
        let cache = self.compile_cache();
        #[cfg(not(target_arch = "wasm32"))]
        let waiting = cache.wanted.is_some();
        #[cfg(target_arch = "wasm32")]
        let waiting = false;
        cache.pending.is_some() || waiting
    }
}
