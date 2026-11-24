// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The application-facing font model: the engine's canonical `Project`, plus the denormalized
//! per-glyph cache the grid paints from. The shell may read the transitional active-master
//! projection, but production writes go through Project operations.

use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use crate::application::launch_options::CompileMode;

use kurbo::{BezPath, Rect};
use runebender::analysis::category::GlyphCategory;
use runebender::font::canonical_metadata::{CanonicalFontMetadata, KerningSide};
use runebender::font::model::font_info::CanonicalFontInfo;
use runebender::font::project::{CanonicalGlyphEntry, DocumentEditOutcome, Project};
use runebender::font::proposal;
use runebender::outline::glyph_paths;
use runebender::ui::theme::{self, Theme};

pub(crate) use runebender::font::axis::Axis;

/// Everything the grid and previews need for one glyph, without touching norad.
#[derive(Clone)]
pub(crate) struct GlyphEntry {
    pub name: String,
    pub codepoint: Option<char>,
    pub advance: f64,
    /// Full outline (contours plus resolved components), shared with the engine's entry.
    pub outline: Arc<BezPath>,
    /// Ink box of the outline (zero when empty).
    pub ink: Rect,
    pub mark: Option<String>,
    pub category: GlyphCategory,
}

impl GlyphEntry {
    fn from_core(entry: &CanonicalGlyphEntry, mark_theme: Option<&Theme>) -> Self {
        Self {
            name: entry.name().to_owned(),
            codepoint: entry.codepoint(),
            advance: entry.advance(),
            outline: entry.outline().clone(),
            ink: entry.ink(),
            mark: mark_theme.and_then(|theme| {
                theme::mark_label_for_values(entry.mark_label(), entry.mark_color(), theme)
            }),
            category: entry
                .codepoint()
                .map(GlyphCategory::from_codepoint)
                .unwrap_or(GlyphCategory::Other),
        }
    }
}

pub(crate) struct FontModel {
    /// The engine project: the masters, the designspace, and the undo piles.
    pub project: Project,
    /// The active source's canonical glyph entries in display order.
    pub glyphs: Vec<GlyphEntry>,
    /// Constant-time lookup into the display-ordered canonical entries.
    /// Each glyph's position in `glyphs`. Shared, so text outlines can use it without a copy.
    name_map: Arc<HashMap<String, usize>>,
    /// The entries of the masters that were active before, by master index, with the
    /// document revision they were built at. A switch back at the same revision reuses
    /// them; building them reads the outline of each glyph.
    parked_entries: HashMap<usize, ParkedEntries>,
    /// The layer being edited for a glyph, when it is not the active master's own layer.
    /// An entry for another source, or for a layer the glyph no longer has, is ignored.
    pub(crate) edit_layers: HashMap<String, runebender::font::variable::LayerId>,
    pub axes: Vec<Axis>,
    /// The text inputs last read from this model, reused while nothing they read has changed.
    pub(crate) text_inputs:
        std::sync::Mutex<Option<crate::application::editor::tools::text::CachedTextInputs>>,
    /// The interpolated advances and outlines of the text between masters, at the location last
    /// asked for.
    pub(crate) interpolated_text:
        std::sync::Mutex<Option<crate::application::editor::tools::text::CachedInterpolatedText>>,
    /// The layout tables at the location last asked for, built from live values; see
    /// [`Self::live_layout`].
    live_layout: std::sync::Mutex<LiveLayoutCache>,
    /// The outlines of masters that are not active, by master index, with the document
    /// revision they were read at; see [`Self::master_outlines`].
    master_outlines: std::sync::Mutex<HashMap<usize, (u64, MasterOutlines)>>,
    /// Inspector readouts that walk the whole font, kept until the document changes.
    readouts: std::sync::Mutex<Readouts>,
    /// Whether [`Self::preview_font`] compiles on the calling thread or on a worker thread.
    preview_mode: CompileMode,
}

/// The document state a readout was taken from: its revision and the active source.
type ReadoutKey = (u64, usize);

/// Glyph count, missing glyphs, and advance differences of one source against the active one.
type GeometryComparison = Option<(usize, usize, usize)>;

/// Readouts the inspector asks for on every rebuild. Each walks every glyph, so each is
/// kept with the document state it was read from and served until that moves on.
#[derive(Default)]
struct Readouts {
    /// The composites that use one glyph, by its name.
    used_by: Option<(ReadoutKey, String, Vec<String>)>,
    /// Geometry comparisons against the active source, by source index.
    comparisons: Option<(ReadoutKey, HashMap<usize, GeometryComparison>)>,
    /// Counts of the glyphs that match a sidebar row, by the kind of row and its indexes.
    counts: Option<(ReadoutKey, RowCounts)>,
}

/// Glyph counts by the kind of sidebar row and its two indexes.
type RowCounts = HashMap<(u8, usize, usize), usize>;

/// The document revision, the entries and the name index of a master that is not active.
type ParkedEntries = (u64, Vec<GlyphEntry>, Arc<HashMap<String, usize>>);

/// The theme whose mark names label glyph entries, parsed once.
fn mark_theme() -> Option<&'static Theme> {
    static THEME: std::sync::OnceLock<Option<Theme>> = std::sync::OnceLock::new();
    THEME.get_or_init(|| theme::load_theme("gray")).as_ref()
}

impl FontModel {
    fn default_source_id(&self) -> runebender::font::variable::SourceId {
        let index = self
            .project
            .master_locations()
            .iter()
            .position(|location| location.values().all(|value| *value == 0.0))
            .unwrap_or(0);
        self.project
            .source_id(index)
            .expect("the default source has a stable identity")
    }

    /// Canonical feature text supplied by the default source.
    pub(crate) fn feature_text(&self) -> &str {
        self.project
            .document_feature_text(self.default_source_id())
            .expect("the default source has canonical feature text")
    }

    /// Canonical group and exact kerning values for the active source.
    pub(crate) fn font_metadata(&self) -> &CanonicalFontMetadata {
        self.font_metadata_at(self.active())
            .expect("the active source has canonical font metadata")
    }

    /// Canonical group and exact kerning values for one source index.
    pub(crate) fn font_metadata_at(&self, index: usize) -> Option<&CanonicalFontMetadata> {
        self.project
            .source_id(index)
            .and_then(|source| self.project.document_font_metadata(source))
    }

    /// Canonical names, metrics and OpenType information for the active source.
    pub(crate) fn font_info(&self) -> &CanonicalFontInfo {
        self.font_info_at(self.active())
            .expect("the active source has canonical font information")
    }

    /// Canonical names, metrics and OpenType information for one source index.
    pub(crate) fn font_info_at(&self, index: usize) -> Option<&CanonicalFontInfo> {
        self.project
            .source_id(index)
            .and_then(|source| self.project.document_font_info(source))
    }

    /// Stable address of the layer being edited for one glyph: the layer chosen in the Layers
    /// panel when it belongs to the active source, else the active master's own layer.
    pub(crate) fn active_layer_address(
        &self,
        glyph: &str,
    ) -> Option<runebender::font::variable::GlyphLayerAddress> {
        let source = self.project.source_id(self.active())?;
        let layer = self
            .edit_layers
            .get(glyph)
            .filter(|layer| {
                layer.source == source && self.project.document_layer(glyph, layer).is_some()
            })
            .cloned()
            .map_or_else(
                || {
                    self.project
                        .document_source(source)
                        .map(|s| s.default_layer())
                },
                Some,
            )?;
        self.project.document_layer(glyph, &layer).map(|_| {
            runebender::font::variable::GlyphLayerAddress {
                glyph: glyph.to_owned(),
                layer,
            }
        })
    }

    /// Project-owned history depth for one active-source glyph layer.
    pub(crate) fn history_depth(
        &self,
        glyph: &str,
        direction: runebender::font::history::HistoryDirection,
    ) -> usize {
        self.active_layer_address(glyph).map_or(0, |address| {
            self.project
                .document_layer_history_depth(&address, direction)
        })
    }

    /// Whether Project-owned history can replay one active-source glyph layer.
    pub(crate) fn can_replay_history(
        &self,
        glyph: &str,
        direction: runebender::font::history::HistoryDirection,
    ) -> bool {
        self.active_layer_address(glyph).is_some_and(|address| {
            self.project
                .can_replay_document_layer_history(&address, direction)
        })
    }

    pub(crate) fn preview_font(
        &self,
    ) -> Result<Option<Arc<runebender::font::compiler::CompiledFont>>, String> {
        match self.preview_mode {
            CompileMode::Blocking => self.project.compiled_preview().map(Some),
            CompileMode::Background => self.project.request_preview(),
        }
    }

    /// Normalized coordinates, in axis order, of these user-space axis values.
    pub(crate) fn normalized_values(&self, values: &[f64]) -> Vec<f64> {
        self.axes
            .iter()
            .enumerate()
            .map(|(index, axis)| {
                axis.user_to_normalized(values.get(index).copied().unwrap_or(axis.default))
            })
            .collect()
    }

    /// The normalized designspace location, by axis name, of these user-space axis values.
    pub(crate) fn normalized_location(
        &self,
        values: &[f64],
    ) -> runebender::font::var_model::Location {
        self.axes
            .iter()
            .zip(self.normalized_values(values))
            .map(|(axis, value)| (axis.name.clone(), value))
            .collect()
    }

    /// The shaping font at these user-space axis values, built on the calling thread from live
    /// values.
    ///
    /// The tables are a function of the feature code of the active master and of the
    /// advances, anchors, kerning and groups at the location, so the same font state always
    /// gives the same layout, with no background compile.
    /// At a master the values are those of the master; between masters each glyph is
    /// interpolated alone, and a value that cannot be interpolated comes from the nearest
    /// master and is listed in `diagnostics`.
    /// The tables are rebuilt only when the layout revision, the active master or the location
    /// changes; moving contour points changes none of them.
    /// When the feature code does not compile, `font` is built from the last feature code that
    /// compiled with the current values, and `error` says why on one line; see
    /// `docs/ARABIC-TEXT-LAYOUT-PLAN.md`.
    /// `source` lets the edit canvas build the same font with the values of a gesture that is
    /// not committed yet; see [`LiveLayoutSource`].
    pub(crate) fn live_layout(&self, values: &[f64]) -> LiveLayout {
        let normalized = self.normalized_values(values);
        let key = (
            self.project.layout_revision(),
            self.active(),
            normalized.iter().map(|value| value.to_bits()).collect(),
        );
        let mut cache = self
            .live_layout
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if cache.key.as_ref() != Some(&key) {
            cache.key = Some(key);
            let source = self
                .project
                .source_id(self.active())
                .ok_or_else(|| "the active source is not loaded".to_string());
            cache.layer = source
                .clone()
                .ok()
                .and_then(|source| self.project.document_source(source))
                .map(|source| source.default_layer());
            let read = source.and_then(|source| {
                use runebender::text::layout_tables::{LayoutInputs, OverrideBasis};
                if self.at_active_master(values) {
                    LayoutInputs::from_project(&self.project, source).map(|inputs| {
                        let basis = OverrideBasis::at_master(&self.project, source);
                        (inputs, Vec::new(), basis)
                    })
                } else {
                    let location = self.normalized_location(values);
                    let (inputs, diagnostics) =
                        LayoutInputs::at_location(&self.project, &location, source)?;
                    let basis = OverrideBasis::at_location(&self.project, &location, source)?;
                    Ok((inputs, diagnostics, basis))
                }
            });
            match read {
                Ok((inputs, diagnostics, basis)) => {
                    cache.basis = Arc::new(basis);
                    cache.diagnostics = Arc::new(diagnostics);
                    cache.advances = Arc::new(
                        inputs
                            .glyphs
                            .iter()
                            .map(|glyph| (glyph.name.clone(), glyph.advance))
                            .collect(),
                    );
                    let inputs = Arc::new(inputs);
                    let built = lock_tables(&cache.tables).build_live(
                        &inputs,
                        &runebender::text::layout_tables::LayoutOverrides::default(),
                    );
                    match built {
                        Ok(built) => {
                            cache.font = Some(Arc::new(built.font.into_bytes()));
                            cache.error = built.feature_error;
                            cache.inputs = Some(inputs);
                        }
                        Err(error) => {
                            cache.error = Some(error);
                            cache.inputs = None;
                        }
                    }
                }
                Err(error) => {
                    cache.error = Some(error);
                    cache.inputs = None;
                }
            }
        }
        let source = cache
            .inputs
            .clone()
            .zip(cache.layer.clone())
            .map(|(inputs, layer)| LiveLayoutSource {
                inputs,
                basis: cache.basis.clone(),
                tables: cache.tables.clone(),
                layer,
            });
        LiveLayout {
            font: cache.font.clone(),
            error: cache.error.clone(),
            diagnostics: cache.diagnostics.clone(),
            advances: cache.advances.clone(),
            source,
        }
    }

    /// Whether these user-space axis values are the location of the active master.
    pub(crate) fn at_active_master(&self, values: &[f64]) -> bool {
        match self.project.master_locations().get(self.active()) {
            Some(master) => self.axes.iter().enumerate().all(|(index, axis)| {
                // Axis values are user coordinates; the engine stores normalized locations.
                let current =
                    axis.user_to_normalized(values.get(index).copied().unwrap_or(axis.default));
                let at = master.get(&axis.name).copied().unwrap_or(0.0);
                (current - at).abs() < 1e-6
            }),
            None => true,
        }
    }

    /// The drawn outlines of one master that is not active, by glyph name.
    ///
    /// Reading them draws every glyph of the master, so they are kept and read again only
    /// after an edit to a layer of that master, or one that the layer journal cannot follow.
    /// Edits of the active master, which is where the designer draws, keep them.
    pub(crate) fn master_outlines(&self, index: usize) -> Option<MasterOutlines> {
        let source = self.project.source_id(index)?;
        let revision = self.project.document_revision();
        let mut cache = self
            .master_outlines
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some((built_at, outlines)) = cache.get_mut(&index) {
            let current = *built_at == revision
                || self
                    .project
                    .layer_changes_since(*built_at)
                    .is_some_and(|changes| {
                        changes.iter().all(|change| change.layer.source != source)
                    });
            if current {
                *built_at = revision;
                return Some(outlines.clone());
            }
        }
        let outlines: MasterOutlines = match self.parked_entries.get(&index) {
            Some((built_at, glyphs, _)) if *built_at == revision => Arc::new(
                glyphs
                    .iter()
                    .map(|glyph| (glyph.name.clone(), glyph.outline.clone()))
                    .collect(),
            ),
            _ => Arc::new(
                self.project
                    .document_source_glyph_entries(source)
                    .ok()?
                    .iter()
                    .map(|entry| (entry.name().to_owned(), entry.outline().clone()))
                    .collect(),
            ),
        };
        cache.insert(index, (revision, outlines.clone()));
        Some(outlines)
    }

    pub(crate) fn open(path: &FsPath) -> Result<Self, String> {
        let project = Project::load(path)?;
        Ok(Self::from_project(project))
    }

    pub(crate) fn from_project(project: Project) -> Self {
        let axes = project.axes.iter().map(|axis| axis.user.clone()).collect();
        let mut model = Self {
            project,
            glyphs: Vec::new(),
            name_map: Arc::default(),
            parked_entries: HashMap::new(),
            axes,
            edit_layers: HashMap::new(),
            text_inputs: std::sync::Mutex::new(None),
            interpolated_text: std::sync::Mutex::new(None),
            live_layout: std::sync::Mutex::default(),
            master_outlines: std::sync::Mutex::default(),
            readouts: std::sync::Mutex::new(Readouts::default()),
            preview_mode: CompileMode::current(),
        };
        model.rebuild_cache();
        model
    }

    /// Rebuild every shell entry from the active canonical source.
    pub(crate) fn rebuild_cache(&mut self) {
        // The callers change the glyph list or its sources, so no parked master is valid.
        self.parked_entries.clear();
        self.fill_cache();
    }

    fn fill_cache(&mut self) {
        let source = self
            .project
            .source_id(self.active())
            .expect("the active source has a stable identity");
        let mark_theme = mark_theme();
        self.glyphs = self
            .project
            .document_source_glyph_entries(source)
            .expect("the active source remains canonical")
            .iter()
            .map(|entry| GlyphEntry::from_core(entry, mark_theme))
            .collect();
        self.name_map = Arc::new(
            self.glyphs
                .iter()
                .enumerate()
                .map(|(index, entry)| (entry.name.clone(), index))
                .collect(),
        );
    }

    /// Refresh one shell entry from its canonical default layer after an edit.
    pub(crate) fn refresh_entry(&mut self, index: usize) {
        let Some(name) = self.glyphs.get(index).map(|entry| entry.name.clone()) else {
            return;
        };
        let source = self
            .project
            .source_id(self.active())
            .expect("the active source has a stable identity");
        let Some(entry) = self
            .project
            .document_source_glyph_entry(source, &name)
            .expect("the active source remains canonical")
        else {
            return;
        };
        let mark_theme = mark_theme();
        self.glyphs[index] = GlyphEntry::from_core(&entry, mark_theme);
    }

    /// Materialize the active source only for assertions at the UFO boundary.
    #[cfg(test)]
    pub(crate) fn font_snapshot(&self) -> norad::Font {
        self.project
            .source_id(self.active())
            .and_then(|source| self.project.encode_ufo_source(source))
            .expect("the active source remains materializable")
    }

    pub(crate) fn source(&self) -> &FsPath {
        self.project
            .source_id(self.active())
            .and_then(|source| self.project.document_source(source))
            .expect("the active source remains in the document")
            .path()
    }

    /// The source that defines the whole document: the designspace when this
    /// is a multi-master project, otherwise the active UFO.
    pub(crate) fn document_source(&self) -> &FsPath {
        self.project
            .export_source
            .as_deref()
            .unwrap_or_else(|| self.source())
    }

    /// Whether every master source is writable according to its filesystem mode.
    pub(crate) fn is_writable(&self) -> bool {
        // The browser host writes through the File System Access API, not file modes.
        if cfg!(target_arch = "wasm32") {
            return true;
        }
        self.project
            .document_sources()
            .all(|source| save_target_is_writable(source.path()))
    }

    pub(crate) fn active(&self) -> usize {
        self.project.active
    }

    pub(crate) fn units_per_em(&self) -> f64 {
        self.font_info().metrics.resolved().units_per_em
    }

    pub(crate) fn ascender(&self) -> f64 {
        self.font_info().metrics.resolved().ascender
    }

    pub(crate) fn descender(&self) -> f64 {
        self.font_info().metrics.resolved().descender
    }

    pub(crate) fn master_names(&self) -> Vec<String> {
        self.project
            .master_names()
            .iter()
            .map(|n| n.to_string())
            .collect()
    }

    pub(crate) fn master_name(&self, index: usize) -> String {
        self.project
            .master_names()
            .get(index)
            .map(|n| n.to_string())
            .unwrap_or_default()
    }

    pub(crate) fn master_paths(&self) -> Vec<PathBuf> {
        self.project
            .document_sources()
            .map(|source| source.path().to_owned())
            .collect()
    }

    /// Switch the active master. Each master keeps its own edits, so
    /// nothing is flushed; the cache is rebuilt for the new one.
    pub(crate) fn set_active(&mut self, index: usize) {
        if index >= self.project.document_sources().count() || index == self.project.active {
            return;
        }
        let revision = self.project.document_revision();
        let leaving = (
            revision,
            std::mem::take(&mut self.glyphs),
            self.name_map.clone(),
        );
        self.parked_entries.insert(self.project.active, leaving);
        self.project.active = index;
        self.project.snap_location_to_master(index);
        match self.parked_entries.remove(&index) {
            Some((built_at, glyphs, name_map)) if built_at == revision => {
                self.glyphs = glyphs;
                self.name_map = name_map;
            }
            _ => self.fill_cache(),
        }
    }

    pub(crate) fn index_of(&self, name: &str) -> Option<usize> {
        self.name_map.get(name).copied()
    }

    /// Each glyph's position in `glyphs`, shared without copying.
    pub(crate) fn name_index(&self) -> Arc<HashMap<String, usize>> {
        self.name_map.clone()
    }

    /// Add an empty glyph to every master.
    ///
    /// Encoded when a codepoint is given, or when the name is a single
    /// character. A glyph that exists in one master and not another is a
    /// designspace that does not build, so this writes all of them.
    pub(crate) fn add_glyph(
        &mut self,
        name: &str,
        default_advance: f64,
        unicode: Option<u32>,
    ) -> bool {
        let Ok(DocumentEditOutcome::Changed { .. }) =
            self.project
                .add_document_glyph(name, default_advance, unicode)
        else {
            return false;
        };
        self.rebuild_cache();
        true
    }

    /// Add every glyph in `targets` that the font does not have yet, and
    /// report how many were added.
    pub(crate) fn add_missing(&mut self, targets: &[(String, Option<u32>)]) -> usize {
        let advance = (self.units_per_em() * 0.5).round();
        let Ok((added, DocumentEditOutcome::Changed { .. })) =
            self.project.add_missing_document_glyphs(targets, advance)
        else {
            return 0;
        };
        self.rebuild_cache();
        added
    }

    /// Duplicate `source` into the first free `stem.NNN` name in every master.
    ///
    /// The copy carries the editable glyph data used by the GPUI command but is
    /// deliberately unencoded. Returns the new name, or `None` when the source
    /// is absent from the active master.
    pub(crate) fn duplicate_glyph(&mut self, source: &str) -> Option<String> {
        let Ok((name, DocumentEditOutcome::Changed { .. })) =
            self.project.duplicate_document_glyph(source)
        else {
            return None;
        };
        self.rebuild_cache();
        Some(name)
    }

    /// Remove `name` from every master and refresh the active-master cache.
    pub(crate) fn remove_glyph(&mut self, name: &str) -> bool {
        let Ok(DocumentEditOutcome::Changed { .. }) = self.project.remove_document_glyph(name)
        else {
            return false;
        };
        self.rebuild_cache();
        true
    }

    /// Rename a glyph, in every master.
    ///
    /// `runebender` does the work inside one font: the glyph, the
    /// components that place it, group memberships, and kerning keys on
    /// either side. This applies that to all the masters, because a
    /// designspace whose sources disagree about a glyph name does not
    /// build.
    pub(crate) fn rename_glyph(&mut self, old: &str, new: &str) -> bool {
        let Ok(DocumentEditOutcome::Changed { .. }) = self.project.rename_document_glyph(old, new)
        else {
            return false;
        };
        self.rebuild_cache();
        true
    }

    /// Save every master to its UFO.
    pub(crate) fn save(&mut self) -> Result<(), String> {
        self.project.save()
    }

    /// The given master's axis location in user coordinates, one per axis,
    /// mapping the engine's stored normalized location back through the axis map.
    pub(crate) fn master_axis_values(&self, index: usize) -> Vec<f64> {
        let loc = self.project.master_locations().get(index);
        self.axes
            .iter()
            .map(|ax| match loc.and_then(|l| l.get(&ax.name)) {
                // For a flat terminal axis map, the inverse can also return
                // the start of the plateau. Show the master's endpoint.
                Some(&1.0) => ax.max,
                Some(&-1.0) => ax.min,
                Some(value) => ax.normalized_to_user(*value),
                None => ax.default,
            })
            .collect()
    }

    /// Interpolate `glyph_name` at the given user-unit axis location. Returns
    /// the interpolated outline (design space) or None if incompatible.
    /// Composite glyphs interpolate both their component offsets and, through
    /// recursion, each component's base outline.
    pub(crate) fn interpolate_outline(
        &self,
        glyph_name: &str,
        location: &HashMap<String, f64>,
    ) -> Option<BezPath> {
        if self.project.model.is_none() || self.axes.is_empty() {
            return None;
        }
        // The engine already stores master locations normalized. Normalize the
        // user-coordinate slider location once, preserving any axis map.
        let target: HashMap<String, f64> = self
            .axes
            .iter()
            .map(|ax| {
                let v = location.get(&ax.name).copied().unwrap_or(ax.default);
                (ax.name.clone(), ax.user_to_normalized(v))
            })
            .collect();
        self.project
            .interpolated_outline_at(glyph_name, &target)
            .ok()
    }

    /// How many masters the family has.
    pub(crate) fn master_count(&self) -> usize {
        self.project.document_sources().count()
    }

    /// Short display names for the masters: the common family prefix is
    /// dropped, so "Bricolage Grotesque 96pt `ExtraBold`" reads as
    /// "96pt `ExtraBold`" in a narrow inspector.
    pub(crate) fn short_master_names(&self) -> Vec<String> {
        let names = self.master_names();
        if names.len() < 2 {
            return names;
        }
        // The longest common prefix, cut back to a word boundary.
        let first = names[0].as_str();
        let mut cut = first.len();
        for other in &names[1..] {
            let common = first
                .char_indices()
                .zip(other.chars())
                .take_while(|((_, a), b)| a == b)
                .map(|((i, a), _)| i + a.len_utf8())
                .last()
                .unwrap_or(0);
            cut = cut.min(common);
        }
        let cut = first[..cut].rfind(' ').map(|i| i + 1).unwrap_or(0);
        names
            .iter()
            .map(|n| {
                let short = n[cut.min(n.len())..].trim();
                if short.is_empty() {
                    n.clone()
                } else {
                    short.to_string()
                }
            })
            .collect()
    }

    /// Outlines of `glyph_name` in the chosen masters, for the
    /// ghost overlay. The inspector's Layers section owns that set.
    pub(crate) fn reference_outlines(
        &self,
        glyph_name: &str,
        which: &std::collections::HashSet<usize>,
    ) -> Vec<Arc<BezPath>> {
        self.project
            .document_sources()
            .enumerate()
            .filter(|(index, _)| which.contains(index) && *index != self.project.active)
            .filter_map(|(_, source)| {
                self.project
                    .document_source_glyph_entry(source.id(), glyph_name)
                    .ok()
                    .flatten()
                    .map(|glyph| glyph.outline().clone())
            })
            .collect()
    }

    /// The layers of `glyph` in the active source that the Layers panel lists: the master's own
    /// layer first, then the glyph's other layers in the source's layer order. The background
    /// and proposal layers have their own panels and are left out.
    pub(crate) fn glyph_layers(&self, glyph: &str) -> Vec<runebender::font::variable::LayerId> {
        let Some(source) = self.project.source_id(self.active()) else {
            return Vec::new();
        };
        let Some(master) = self
            .project
            .document_source(source)
            .map(|s| s.default_layer())
        else {
            return Vec::new();
        };
        let mut layers = vec![master.clone()];
        for name in self
            .project
            .document_source_layer_names(source)
            .unwrap_or_default()
        {
            let hidden = name == "public.background"
                || name.eq_ignore_ascii_case("background")
                || name.starts_with("com.runebender.proposal.");
            let layer = runebender::font::variable::LayerId {
                source,
                name: name.to_owned(),
            };
            if !hidden && layer != master && self.project.document_layer(glyph, &layer).is_some() {
                layers.push(layer);
            }
        }
        layers
    }

    /// One layer's contours and components as a path, for drawing it behind the edited layer.
    pub(crate) fn layer_outline(
        &self,
        glyph: &str,
        layer: &runebender::font::variable::LayerId,
    ) -> Option<Arc<BezPath>> {
        let view = self.project.document_layer(glyph, layer)?;
        let mut path = glyph_paths::ordinary_layer_contours_to_bezpath(view);
        // The layer's components, resolved in this layer's source, as part of its outline.
        let address = runebender::font::variable::GlyphLayerAddress {
            glyph: glyph.to_owned(),
            layer: layer.clone(),
        };
        if let Ok(components) = crate::application::editor::session::resolved_document_components(
            &self.project,
            &address,
        ) {
            for component in &components {
                path.extend(component.path.iter());
            }
        }
        Some(Arc::new(path))
    }

    /// The glyph's outline in the background layer, as a path.
    pub(crate) fn background_outline(&self, glyph: &str) -> Option<BezPath> {
        let source = self.project.source_id(self.active())?;
        let (_, background) = self.project.document_background_layer(glyph, source)?;
        Some(glyph_paths::ordinary_layer_contours_to_bezpath(background))
    }

    /// A glyph from a waiting proposal layer, as a path for the
    /// read-only comparison overlay.
    pub(crate) fn proposal_outline(&self, task: &str, glyph: &str) -> Option<BezPath> {
        let source = self.project.source_id(self.active())?;
        let address = runebender::font::variable::GlyphLayerAddress {
            glyph: glyph.to_owned(),
            layer: runebender::font::variable::LayerId {
                source,
                name: proposal::layer_name(task),
            },
        };
        self.project.document_layer_path(&address).ok()
    }

    /// The advance a proposal gives a glyph, when the proposal names it.
    pub(crate) fn proposal_advance(&self, task: &str, glyph: &str) -> Option<f64> {
        let source = self.project.source_id(self.active())?;
        let layer = runebender::font::variable::LayerId {
            source,
            name: proposal::layer_name(task),
        };
        Some(self.project.document_layer(glyph, &layer)?.width())
    }

    /// Another glyph's outline, for the reference underlay.
    pub(crate) fn glyph_outline(&self, glyph: &str) -> Option<Arc<BezPath>> {
        let index = self.index_of(glyph)?;
        Some(self.glyphs[index].outline.clone())
    }

    /// The kerning group this glyph belongs to on one side, if any.
    ///
    /// `first_side` is the left side in left-to-right text: `public.kern1`.
    pub(crate) fn kern_group(&self, glyph: &str, first_side: bool) -> String {
        let side = if first_side {
            KerningSide::First
        } else {
            KerningSide::Second
        };
        self.font_metadata()
            .kerning_group(glyph, side)
            .map(|name| name.to_string())
            .unwrap_or_default()
    }

    /// Put the glyph in a kerning group on one side, in every master.
    ///
    /// Kerning groups are font-wide, and a designspace's masters have to
    /// agree about them or the kerning will not interpolate, so this
    /// writes all of them rather than only the active one.
    pub(crate) fn set_kern_group(&mut self, glyph: &str, first_side: bool, group: &str) -> bool {
        let side = if first_side {
            KerningSide::First
        } else {
            KerningSide::Second
        };
        let mut changed = false;
        let sources: Vec<_> = self
            .project
            .document_sources()
            .map(|source| source.id())
            .collect();
        for source in sources {
            let Some(mut metadata) = self.project.document_font_metadata(source).cloned() else {
                continue;
            };
            let Ok(edited) = metadata.set_kerning_group(glyph, side, Some(group)) else {
                continue;
            };
            if !edited {
                continue;
            }
            changed |= matches!(
                self.project.edit_document_source_metadata(source, |draft| {
                    draft.set_font_metadata(metadata);
                    Ok(())
                }),
                Ok(DocumentEditOutcome::Changed { .. })
            );
        }
        changed
    }

    /// How many glyphs are marked for export.
    ///
    /// A glyph is skipped when its lib says so, which is how both Glyphs
    /// and the UFO spec record it. The filter list shows the resulting count.
    pub(crate) fn exporting_count(&self) -> usize {
        let source = self
            .project
            .source_id(self.active())
            .expect("the active source has a stable identity");
        self.glyphs
            .iter()
            .filter(|entry| {
                self.project
                    .document_source_glyph_metadata(source, &entry.name)
                    .is_none_or(|metadata| metadata.exported())
            })
            .count()
    }

    /// Count the engine's cached compatibility results without interpolating during view rebuilds.
    pub(crate) fn incompatible_count(&self) -> usize {
        if self.project.document_sources().count() < 2 {
            return 0;
        }
        self.glyphs
            .iter()
            .filter(|entry| self.project.compat.get(&entry.name) == Some(&false))
            .count()
    }

    /// Compare one source's default-layer coverage and advances with the active source.
    pub(crate) fn source_geometry_comparison(&self, index: usize) -> Option<(usize, usize, usize)> {
        let key = self.readout_key();
        let mut readouts = self
            .readouts
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let comparisons = match &mut readouts.comparisons {
            Some((at, comparisons)) if *at == key => comparisons,
            slot => &mut slot.insert((key, HashMap::new())).1,
        };
        *comparisons
            .entry(index)
            .or_insert_with(|| self.compare_source_geometry(index))
    }

    /// A count over every glyph for one sidebar row, computed once for each state of the
    /// document. `kind` separates the rows that share an index space.
    pub(crate) fn glyph_count(
        &self,
        kind: u8,
        index: (usize, usize),
        count: impl FnOnce() -> usize,
    ) -> usize {
        let key = self.readout_key();
        let entry = (kind, index.0, index.1);
        {
            let mut readouts = self
                .readouts
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            match &readouts.counts {
                Some((at, counts)) if *at == key => {
                    if let Some(known) = counts.get(&entry) {
                        return *known;
                    }
                }
                _ => readouts.counts = Some((key, HashMap::new())),
            }
        }
        // Counted without the lock, because the count reads the model.
        let counted = count();
        let mut readouts = self
            .readouts
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some((at, counts)) = &mut readouts.counts
            && *at == key
        {
            counts.insert(entry, counted);
        }
        counted
    }

    fn readout_key(&self) -> ReadoutKey {
        (self.project.document_revision(), self.active())
    }

    /// The glyphs whose active layer places `glyph` as a component, in display order.
    pub(crate) fn composites_using(&self, glyph: &str) -> Vec<String> {
        let key = self.readout_key();
        let mut readouts = self
            .readouts
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some((at, name, users)) = &mut readouts.used_by
            && name == glyph
            && at.1 == key.1
        {
            // Which glyphs place this one changes only when some glyph's components change.
            // Layer edits that left components alone keep the answer.
            let unchanged = *at == key
                || self
                    .project
                    .layer_changes_since(at.0)
                    .is_some_and(|changes| changes.iter().all(|change| !change.mark_inputs));
            if unchanged {
                *at = key;
                return users.clone();
            }
        }
        let users = self
            .active_layer_address(glyph)
            .map(|address| {
                self.glyphs
                    .iter()
                    .filter(|entry| {
                        self.project
                            .document_layer(&entry.name, &address.layer)
                            .is_some_and(|layer| {
                                layer
                                    .components()
                                    .any(|component| component.reference() == glyph)
                            })
                    })
                    .map(|entry| entry.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        readouts.used_by = Some((key, glyph.to_owned(), users.clone()));
        users
    }

    fn compare_source_geometry(&self, index: usize) -> Option<(usize, usize, usize)> {
        let project = &self.project;
        let reference_layer = project
            .source_id(self.active())
            .and_then(|source| project.document_source(source))?
            .default_layer();
        let layer = project
            .source_id(index)
            .and_then(|source| project.document_source(source))?
            .default_layer();
        let reference_glyphs: Vec<_> = project
            .glyph_names()
            .filter(|name| project.document_layer(name, &reference_layer).is_some())
            .collect();
        let glyph_count = project
            .glyph_names()
            .filter(|name| project.document_layer(name, &layer).is_some())
            .count();
        let missing = reference_glyphs
            .iter()
            .filter(|name| project.document_layer(name, &layer).is_none())
            .count();
        let advance_differences = reference_glyphs
            .iter()
            .filter(|name| {
                project
                    .document_layer(name, &layer)
                    .zip(project.document_layer(name, &reference_layer))
                    .is_some_and(|(candidate, reference)| {
                        (candidate.width() - reference.width()).abs() > 0.5
                    })
            })
            .count();
        Some((glyph_count, missing, advance_differences))
    }

    /// The font's headline metadata, as label and value pairs.
    ///
    /// The overview shows this whenever no glyph is picked. These values are
    /// read here rather than edited: writing them back means a form per
    /// master and a rule about which values are per-master, which the
    /// editor does not have yet.
    pub(crate) fn info_rows(&self) -> Vec<(&'static str, String)> {
        let info = self.font_info();
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        let number = |value: Option<f64>| value.map(|v| format!("{v:.0}")).unwrap_or_default();
        vec![
            ("Family name", text(&info.names.family_name)),
            ("Style name", text(&info.names.style_name)),
            ("UPM", number(info.metrics.units_per_em)),
            ("Italic angle", number(info.metrics.italic_angle)),
            ("Ascender", number(info.metrics.ascender)),
            ("Descender", number(info.metrics.descender)),
            ("x-height", number(info.metrics.x_height)),
            ("Cap height", number(info.metrics.cap_height)),
            (
                "typoAsc",
                number(info.open_type_metrics.typo_ascender.map(f64::from)),
            ),
            (
                "typoDesc",
                number(info.open_type_metrics.typo_descender.map(f64::from)),
            ),
            (
                "hheaAsc",
                number(info.open_type_metrics.hhea_ascender.map(f64::from)),
            ),
            (
                "hheaDesc",
                number(info.open_type_metrics.hhea_descender.map(f64::from)),
            ),
            (
                "winAsc",
                number(info.open_type_metrics.win_ascent.map(f64::from)),
            ),
            (
                "winDesc",
                number(info.open_type_metrics.win_descent.map(f64::from)),
            ),
        ]
    }

    /// Set the advance of a glyph that is not open in the editor.
    ///
    /// The overview panel edits the selected cell directly, so this works
    /// from an index rather than from a session. Only the active master
    /// changes: an advance is a per-master measurement.
    pub(crate) fn set_glyph_advance(&mut self, index: usize, width: f64) -> bool {
        if !width.is_finite() {
            return false;
        }
        let width = width.max(0.0);
        let Some(name) = self.glyphs.get(index).map(|entry| entry.name.clone()) else {
            return false;
        };
        let Some(address) = self.active_layer_address(&name) else {
            return false;
        };
        let Ok(mut transaction) = self.project.begin_document_layer_transaction(&address) else {
            return false;
        };
        if transaction.draft_mut().set_width(width) != Ok(true)
            || !matches!(
                self.project.commit_document_layer_transaction(transaction),
                Ok(DocumentEditOutcome::Changed { .. })
            )
        {
            return false;
        }
        self.refresh_entry(index);
        true
    }

    /// Exact codepoint lists for `name`, one per document source.
    pub(crate) fn glyph_codepoints(&self, name: &str) -> Option<Vec<Vec<char>>> {
        self.project.document_glyph_codepoints(name)
    }

    /// Replace `name`'s codepoints in every document source from an exact snapshot.
    pub(crate) fn set_glyph_codepoints(&mut self, name: &str, values: &[Vec<char>]) -> bool {
        let changed = matches!(
            self.project.set_document_glyph_codepoints(name, values),
            Ok(DocumentEditOutcome::Changed { .. })
        );
        if !changed {
            return false;
        }
        self.rebuild_cache();
        true
    }
}

/// Whether an existing save target, or the directory for a new one, is writable.
fn save_target_is_writable(target: &FsPath) -> bool {
    if target.exists() {
        return std::fs::metadata(target)
            .map(|metadata| !metadata.permissions().readonly())
            .unwrap_or(false);
    }
    target
        .parent()
        .and_then(|parent| std::fs::metadata(parent).ok())
        .is_some_and(|metadata| metadata.is_dir() && !metadata.permissions().readonly())
}

#[cfg(test)]
mod tests {
    use super::*;
    use runebender::font::canonical_metadata::KerningParticipant;
    use runebender::font::project::SourceInput;

    /// Glyph name, advance and offsets of each shaped glyph of `text`, right to left.
    fn live_rows(font: &FontModel, text: &str) -> Vec<(String, f64, f64, f64)> {
        let bytes = font.live_layout(&[]).font.expect("a live layout font");
        let shaping = runebender::text::shape::ShapingFont::from_bytes((*bytes).clone())
            .expect("the live layout font loads");
        shaping
            .shape(text, true)
            .expect("the text shapes")
            .iter()
            .map(|glyph| {
                (
                    shaping
                        .glyph_name(glyph.glyph_id)
                        .unwrap_or("?")
                        .to_string(),
                    glyph.x_advance,
                    glyph.x_offset,
                    glyph.y_offset,
                )
            })
            .collect()
    }

    #[test]
    fn a_feature_error_keeps_the_last_features_with_the_live_values() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/arabic/Arabic-Regular.ufo");
        let mut font = FontModel::open(&path).expect("the fixture opens");
        assert!(font.live_layout(&[]).error.is_none());
        assert_eq!(live_rows(&font, "لا")[0].0, "lam_alef-ar");
        let kasra_before = live_rows(&font, "بِ");

        let good = font.feature_text().to_string();
        let mut lines: Vec<&str> = good.lines().collect();
        lines.insert(2, "feature broken { sub ; } broken;");
        assert!(font.project.set_feature_text(lines.join("\n")));
        let error = font.live_layout(&[]).error.expect("the error is reported");
        assert!(error.starts_with("features.fea line 3:"), "{error}");

        // An advance and an anchor edit still reach the line.
        let layer = font
            .project
            .document_sources()
            .next()
            .expect("a source")
            .default_layer();
        font.project
            .edit_document_layer("beh-ar", &layer, |draft| {
                let width = draft.view().width();
                draft.set_width(width + 30.0)?;
                let bottom = draft
                    .view()
                    .anchors()
                    .find(|anchor| anchor.name() == "bottom")
                    .expect("beh has a bottom anchor");
                let to = bottom.position() + kurbo::Vec2::new(25.0, -15.0);
                draft.set_anchor_position(bottom.id(), to)?;
                Ok(())
            })
            .expect("the edit commits");
        let live = font.live_layout(&[]);
        assert_eq!(live.error.as_deref(), Some(error.as_str()));
        assert_eq!(
            live_rows(&font, "لا")[0].0,
            "lam_alef-ar",
            "substitutions are those of the last features that compiled"
        );
        let kasra_after = live_rows(&font, "بِ");
        let beh = |rows: &[(String, f64, f64, f64)]| {
            rows.iter()
                .find(|row| row.0 == "beh-ar")
                .cloned()
                .expect("beh")
        };
        let kasra = |rows: &[(String, f64, f64, f64)]| {
            rows.iter()
                .find(|row| row.0 == "kasra-ar")
                .cloned()
                .expect("kasra")
        };
        assert_eq!(beh(&kasra_after).1, beh(&kasra_before).1 + 30.0);
        assert_eq!(kasra(&kasra_after).3, kasra(&kasra_before).3 - 15.0);

        assert!(font.project.set_feature_text(good));
        assert!(
            font.live_layout(&[]).error.is_none(),
            "fixing the text clears it"
        );
    }

    fn two_master_model() -> (PathBuf, FontModel) {
        let dir = std::env::temp_dir().join(format!(
            "runebender-xilem-glyph-commands-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        std::fs::create_dir_all(&dir).expect("the fixture directory is created");
        for (file, width) in [("Regular.ufo", 500.0), ("Bold.ufo", 620.0)] {
            let mut font = norad::Font::new();
            let mut glyph = norad::Glyph::new("A");
            glyph.width = width;
            let mut contour = norad::Contour::default();
            contour.points.push(norad::ContourPoint::new(
                width,
                0.0,
                norad::PointType::Move,
                false,
                None,
                None,
            ));
            glyph.contours.push(contour);
            glyph.codepoints = norad::Codepoints::new(['A']);
            font.default_layer_mut().insert_glyph(glyph);
            font.save(dir.join(file)).expect("the master saves");
        }
        let designspace = dir.join("Test.designspace");
        std::fs::write(
            &designspace,
            r#"<?xml version='1.0' encoding='UTF-8'?>
<designspace format="4.0">
  <axes><axis name="Weight" tag="wght" minimum="400" default="400" maximum="700"/></axes>
  <sources>
    <source familyname="Test" stylename="Regular" filename="Regular.ufo">
      <location><dimension name="Weight" xvalue="400"/></location>
    </source>
    <source familyname="Test" stylename="Bold" filename="Bold.ufo">
      <location><dimension name="Weight" xvalue="700"/></location>
    </source>
  </sources>
</designspace>"#,
        )
        .expect("the designspace saves");
        let model = FontModel::open(&designspace).expect("the designspace opens");
        (dir, model)
    }

    #[test]
    fn a_background_preview_arrives_from_a_worker_thread() {
        let (dir, mut model) = two_master_model();
        assert_eq!(model.preview_mode, CompileMode::Blocking);
        model.preview_mode = CompileMode::Background;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let compiled = loop {
            if let Some(compiled) = model.preview_font().expect("the preview compiles") {
                break compiled;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the background preview arrives"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let blocking = model
            .project
            .compiled_preview()
            .expect("the preview compiles");
        assert!(Arc::ptr_eq(&compiled, &blocking));
        std::fs::remove_dir_all(dir).expect("the fixture directory is removed");
    }

    #[test]
    fn glyph_cache_resolves_canonical_marks_against_the_gray_palette() {
        let mut font = norad::Font::new();
        for (name, label, color) in [
            ("legacy", None, Some("0.93,0.45,0.2,1")),
            ("custom", Some("review-later"), Some("0.93,0.45,0.2,1")),
            ("unknown", Some("review-later"), None),
            ("known", Some("blue"), Some("0.93,0.45,0.2,1")),
        ] {
            let mut glyph = norad::Glyph::new(name);
            if let Some(label) = label {
                glyph.lib.insert(
                    "com.runebender.markLabel".into(),
                    plist::Value::String(label.into()),
                );
            }
            if let Some(color) = color {
                glyph.lib.insert(
                    "public.markColor".into(),
                    plist::Value::String(color.into()),
                );
            }
            font.default_layer_mut().insert_glyph(glyph);
        }
        let project =
            Project::from_source(SourceInput::from_font(font, PathBuf::from("Marks.ufo")))
                .expect("valid source fixture");
        let custom = project
            .document_source_glyph_entry(runebender::font::variable::SourceId(0), "custom")
            .expect("the source exists")
            .expect("the glyph exists");
        assert_eq!(custom.mark_label(), Some("review-later"));
        assert!(custom.mark_color().is_some());
        assert!(GlyphEntry::from_core(&custom, None).mark.is_none());

        let model = FontModel::from_project(project);
        for (name, expected) in [
            ("legacy", Some("orange")),
            ("custom", Some("orange")),
            ("unknown", None),
            ("known", Some("blue")),
        ] {
            assert_eq!(
                model
                    .glyphs
                    .iter()
                    .find(|entry| entry.name == name)
                    .unwrap()
                    .mark
                    .as_deref(),
                expected,
                "{name} display mark"
            );
        }
    }

    #[test]
    fn duplicate_and_remove_glyph_apply_to_every_master() {
        let (dir, mut model) = two_master_model();

        let copy = model.duplicate_glyph("A").expect("A duplicates");
        assert_eq!(copy, "A.001");
        for (source, width) in model.project.document_sources().zip([500.0, 620.0]) {
            let glyph = model
                .project
                .document_layer(&copy, &source.default_layer())
                .expect("the copy exists");
            assert_eq!(glyph.width(), width);
            assert_eq!(glyph.contours().count(), 1);
            assert_eq!(glyph.codepoints().count(), 0);
        }
        assert!(model.remove_glyph(&copy));
        assert!(model.project.document_glyph(&copy).is_none());

        std::fs::remove_dir_all(dir).expect("the fixture is removed");
    }

    #[test]
    fn source_metadata_queries_read_canonical_unsaved_values() {
        let (_, mut model) = two_master_model();
        let source = model
            .project
            .source_id(model.active())
            .expect("active source identity");
        let mut metadata = model
            .project
            .document_font_metadata(source)
            .expect("canonical source metadata")
            .clone();
        metadata
            .set_kerning_group("A", KerningSide::First, Some("A"))
            .expect("valid group edit");
        metadata
            .set_kerning_pair(
                KerningParticipant::group(KerningSide::First, "A").expect("valid group"),
                KerningParticipant::glyph("V").expect("valid glyph"),
                Some(-80.25),
            )
            .expect("valid exact kerning pair");
        let mut font_info = model
            .project
            .document_font_info(source)
            .expect("canonical font information")
            .clone();
        font_info.names.family_name = Some("Unsaved Family".into());
        font_info.metrics.units_per_em = Some(2048.0);
        model
            .project
            .edit_document_source_metadata(source, |draft| {
                draft.set_font_metadata(metadata);
                draft.set_font_info(font_info);
                Ok(())
            })
            .expect("canonical metadata edit commits");

        assert_eq!(model.kern_group("A", true), "public.kern1.A");
        assert!(
            model
                .font_metadata()
                .kerning_pairs()
                .any(
                    |(first, second, value)| first.as_raw_name() == "public.kern1.A"
                        && second.as_raw_name() == "V"
                        && value == -80.25
                )
        );
        assert_eq!(model.units_per_em(), 2048.0);
        let session = crate::application::editor::session::Session::new_from_model(&model, "A")
            .expect("the canonical source glyph opens");
        assert_eq!(session.metrics.upm, 2048.0);
        assert_eq!(session.metrics.ascender, 2048.0 * 0.8);
        assert_eq!(
            model.info_rows().into_iter().take(3).collect::<Vec<_>>(),
            [
                ("Family name", "Unsaved Family".into()),
                ("Style name", String::new()),
                ("UPM", "2048".into()),
            ]
        );
    }

    #[test]
    fn master_locations_are_presented_on_the_user_axis_scale() {
        let (dir, model) = two_master_model();

        assert_eq!(model.master_axis_values(0), vec![400.0]);
        assert_eq!(model.master_axis_values(1), vec![700.0]);
        assert_eq!(model.source_geometry_comparison(1), Some((1, 0, 1)));

        std::fs::remove_dir_all(dir).expect("the fixture is removed");
    }

    #[test]
    fn mapped_axis_coordinates_round_trip_through_normalized_space() {
        let axis = Axis {
            name: "Weight".into(),
            tag: "wght".into(),
            min: 100.0,
            default: 400.0,
            max: 900.0,
            map: vec![(100.0, 0.0), (400.0, 50.0), (900.0, 100.0)],
        };

        for (user, normalized) in [(100.0, -1.0), (400.0, 0.0), (650.0, 0.5), (900.0, 1.0)] {
            assert!((axis.user_to_normalized(user) - normalized).abs() < 1e-9);
            assert!((axis.normalized_to_user(normalized) - user).abs() < 1e-9);
        }
    }

    #[test]
    fn fixture_keeps_an_incompatible_master_pair() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/incompatible/Test.designspace");
        let model = FontModel::open(&path).expect("the checked-in test fixture opens");

        assert_eq!(model.incompatible_count(), 1);
        let detail = model
            .project
            .compat_detail("A")
            .expect("A is deliberately incompatible");
        assert!(detail.contains("Regular 0c"));
        assert!(detail.contains("Bold 1c"));
    }

    #[test]
    fn cached_incompatible_count_tracks_edits_and_history() {
        use runebender::font::history::HistoryDirection;
        use runebender::font::variable::GlyphLayerAddress;

        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/incompatible/Test.designspace");
        let mut model = FontModel::open(&path).unwrap();
        let source = model.project.source_id(1).unwrap();
        let address = GlyphLayerAddress {
            glyph: "A".into(),
            layer: model
                .project
                .document_source(source)
                .unwrap()
                .default_layer(),
        };
        assert_eq!(model.incompatible_count(), 1);
        let mut transaction = model
            .project
            .begin_document_layer_transaction(&address)
            .unwrap();
        assert!(transaction.draft_mut().clear_contours());
        model
            .project
            .commit_document_layer_transaction(transaction)
            .unwrap();
        assert_eq!(model.incompatible_count(), 0);
        model
            .project
            .replay_document_layer_history(&address, HistoryDirection::Undo)
            .unwrap();
        assert_eq!(model.incompatible_count(), 1);
        model
            .project
            .replay_document_layer_history(&address, HistoryDirection::Redo)
            .unwrap();
        assert_eq!(model.incompatible_count(), 0);
    }

    #[test]
    fn save_targets_require_a_writable_directory() {
        let (dir, model) = two_master_model();
        assert!(model.is_writable());

        let file = dir.join("not-a-directory");
        std::fs::write(&file, "fixture").expect("the ordinary file is created");
        let invalid = FontModel::from_project(
            Project::from_source(SourceInput::from_font(
                norad::Font::new(),
                file.join("New.ufo"),
            ))
            .expect("valid source fixture"),
        );
        assert!(!invalid.is_writable());

        std::fs::remove_dir_all(dir).expect("the fixture is removed");
    }
}

/// The drawn outlines of one master by glyph name; see [`FontModel::master_outlines`].
pub(crate) type MasterOutlines = Arc<HashMap<String, Arc<BezPath>>>;

/// What [`FontModel::live_layout`] returns.
#[derive(Clone, Debug, Default)]
pub(crate) struct LiveLayout {
    /// The shaping font, or the last one that could be built.
    pub(crate) font: Option<Arc<Vec<u8>>>,
    /// Why the current font state could not be built.
    pub(crate) error: Option<String>,
    /// The values at the location that came from the nearest master instead of the
    /// interpolation; empty at a master.
    pub(crate) diagnostics: Arc<Vec<runebender::text::layout_tables::LayoutDiagnostic>>,
    /// The advance of each glyph at the location.
    pub(crate) advances: Arc<HashMap<String, f64>>,
    /// The committed inputs of `font` and its builder, when `font` was built from them.
    pub(crate) source: Option<LiveLayoutSource>,
}

/// The committed layout inputs at the displayed location, what moves the values of a gesture
/// on the active master there, and the builder that turned the inputs into the live layout
/// font.
///
/// The edit canvas keeps this with its text line.
/// During a gesture it builds the shaping font from these inputs with the values of the
/// gesture in place, through the same [`LayoutTables::build_live`] call that the commit takes,
/// and with the same caches, so a drag step rebuilds only the lookups whose values changed.
///
/// [`LayoutTables::build_live`]: runebender::text::layout_tables::LayoutTables::build_live
#[derive(Clone, Debug)]
pub(crate) struct LiveLayoutSource {
    /// The committed inputs at the displayed location.
    inputs: Arc<runebender::text::layout_tables::LayoutInputs>,
    /// How the values of a gesture on `layer` reach `inputs`.
    basis: Arc<runebender::text::layout_tables::OverrideBasis>,
    tables: Arc<std::sync::Mutex<runebender::text::layout_tables::LayoutTables>>,
    /// The layer that a gesture edits: the default layer of the active master.
    layer: runebender::font::variable::LayerId,
}

impl PartialEq for LiveLayoutSource {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inputs, &other.inputs) && Arc::ptr_eq(&self.tables, &other.tables)
    }
}

impl LiveLayoutSource {
    /// Whether an edit of `layer` changes the values that these inputs hold.
    pub(crate) fn reads_layer(&self, layer: &runebender::font::variable::LayerId) -> bool {
        self.layer == *layer
    }

    /// Values of the edited master as the overrides of the inputs at the location that change
    /// a committed value; see
    /// [`OverrideBasis::locate`](runebender::text::layout_tables::OverrideBasis::locate).
    pub(crate) fn locate(
        &self,
        overrides: &runebender::text::layout_tables::LayoutOverrides,
    ) -> runebender::text::layout_tables::LayoutOverrides {
        self.basis.locate(&self.inputs, overrides)
    }

    /// The shaping font of the committed values with `overrides` in place, as bytes.
    pub(crate) fn build(
        &self,
        overrides: &runebender::text::layout_tables::LayoutOverrides,
    ) -> Option<Arc<Vec<u8>>> {
        match lock_tables(&self.tables).build_live(&self.inputs, overrides) {
            Ok(built) => Some(Arc::new(built.font.into_bytes())),
            Err(error) => {
                runebender::text::shape::log_shaping_failure(&error);
                None
            }
        }
    }

    /// How many times each step of the shared builder has run.
    #[cfg(test)]
    pub(crate) fn build_counts(&self) -> runebender::text::layout_tables::LayoutBuildCounts {
        lock_tables(&self.tables).build_counts()
    }
}

fn lock_tables(
    tables: &std::sync::Mutex<runebender::text::layout_tables::LayoutTables>,
) -> std::sync::MutexGuard<'_, runebender::text::layout_tables::LayoutTables> {
    tables.lock().unwrap_or_else(|error| error.into_inner())
}

/// The layout revision, the active master and the bits of the normalized location of a build.
type LiveLayoutKey = (u64, usize, Vec<u64>);

/// The last build of the live layout and the state it was built from.
#[derive(Debug, Default)]
struct LiveLayoutCache {
    /// The state of the last build attempt.
    key: Option<LiveLayoutKey>,
    tables: Arc<std::sync::Mutex<runebender::text::layout_tables::LayoutTables>>,
    /// The committed inputs of `font`, when the last build attempt succeeded.
    inputs: Option<Arc<runebender::text::layout_tables::LayoutInputs>>,
    /// The layer that `inputs` were read from.
    layer: Option<runebender::font::variable::LayerId>,
    /// How a gesture on that layer reaches `inputs`.
    basis: Arc<runebender::text::layout_tables::OverrideBasis>,
    font: Option<Arc<Vec<u8>>>,
    error: Option<String>,
    diagnostics: Arc<Vec<runebender::text::layout_tables::LayoutDiagnostic>>,
    advances: Arc<HashMap<String, f64>>,
}
