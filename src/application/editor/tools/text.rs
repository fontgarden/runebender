// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The text tool: type glyphs into a line, and edit them in context.
//!
//! Spacing and kerning are judged in words, not on one glyph at a time,
//! so a font editor needs a place to type. The engine is
//! `runebender::text::buffer`, shared with other Runebender interfaces:
//! it owns the buffer, the shaping, the bidi runs, the kerning, the
//! caret, and the hit testing. What lives here is the part that is
//! specific to this editor: keeping the buffer fed with the current
//! master's metrics, drawing the laid-out sorts, and turning a click
//! into either a caret position or a glyph to edit.

use std::sync::Arc;

use masonry::kurbo::{BezPath, Point};
use runebender::text::buffer::{TextBuffer, TextDirection, TextGlyphInventory, TextKerningModel};

use crate::application::font_model::{FontModel, LiveLayoutSource};
use runebender::text::layout_tables::LayoutOverrides;

/// What the view can carry.
///
/// `TextBuffer` holds `Rc` and `RefCell` (a shaping-font cache and a run
/// cache), so it is neither `Send` nor `Sync`, and a Xilem view has to be
/// both. So the view passes this, which is plain data, and the widget
/// builds the buffer on the other side. The buffer then lives where the
/// editing happens, which is where it wanted to live anyway.
#[derive(Clone)]
pub(crate) struct TextInputs {
    /// Document and tab identity. A change means the widget must restore a
    /// different parked buffer instead of refreshing the current one.
    context_id: (u64, u64),
    inventory: Arc<TextGlyphInventory>,
    kerning: Arc<TextKerningModel>,
    outlines: GlyphOutlines,
    /// The glyph being edited and its unsaved outline, drawn in place of its entry above.
    live_outline: Option<(String, Arc<BezPath>)>,
    compiled: Option<Arc<Vec<u8>>>,
    /// The committed inputs and builder of `compiled`, when it is the live layout of the
    /// active master; a gesture shapes the line from them with its own values in place.
    live_layout: Option<LiveLayoutSource>,
    normalized: Vec<f64>,
    line_height: f64,
    ascender: f64,
    descender: f64,
    /// Text to start with. Only used when the buffer is created, so it
    /// is a starting state and not a binding.
    initial: String,
    /// The glyph whose editor tab owns this text line. It remains the active
    /// sort when a text buffer is first opened.
    active_glyph: Option<(String, Option<char>, f64)>,
    /// Initial logical range for deterministic visual evidence.
    initial_selection: Option<(usize, usize)>,
    /// Writing direction, or `None` for automatic. This one *is* a
    /// binding: the direction chips live in the title bar, which is
    /// view-land, so the setting has to travel in with the inputs.
    direction: Option<TextDirection>,
    feature_overrides: Vec<(String, bool)>,
    script: Option<String>,
    language: Option<String>,
}

/// Whether two shared values are the same allocation, or hold equal contents.
/// The first check is what makes comparing unchanged inputs cheap.
fn same<T: PartialEq>(a: &Arc<T>, b: &Arc<T>) -> bool {
    Arc::ptr_eq(a, b) || a == b
}

impl PartialEq for TextInputs {
    /// A widget compares its inputs on every rebuild, so the glyph inventory, kerning model
    /// and outlines are compared by identity before by content. Unchanged inputs share them.
    fn eq(&self, other: &Self) -> bool {
        self.context_id == other.context_id
            && self.normalized == other.normalized
            && self.line_height == other.line_height
            && self.ascender == other.ascender
            && self.descender == other.descender
            && self.initial == other.initial
            && self.active_glyph == other.active_glyph
            && self.initial_selection == other.initial_selection
            && self.direction == other.direction
            && self.feature_overrides == other.feature_overrides
            && self.script == other.script
            && self.language == other.language
            && match (&self.compiled, &other.compiled) {
                (Some(a), Some(b)) => same(a, b),
                (None, None) => true,
                _ => false,
            }
            && self.live_layout == other.live_layout
            && match (&self.live_outline, &other.live_outline) {
                (Some((a_name, a)), Some((b_name, b))) => a_name == b_name && same(a, b),
                (None, None) => true,
                _ => false,
            }
            && same(&self.inventory, &other.inventory)
            && same(&self.kerning, &other.kerning)
            && self.outlines == other.outlines
    }
}

/// What `TextInputs::new` last read, with the document state it read it from.
///
/// Every rebuild of a view with a text line asks for the inputs again, and reading them walks
/// every glyph of the font. The document revision moves on any edit, so the same inputs
/// serve until it does.
pub(crate) struct CachedTextInputs {
    key: (u64, usize, usize),
    inputs: TextInputs,
}

/// Glyph outlines by name, shared between the text inputs and every buffer made from them.
///
/// Two values are equal when they are the same shared source, which is how unchanged inputs
/// compare; sources built separately are treated as different.
#[derive(Clone)]
pub(crate) struct GlyphOutlines(Arc<OutlineSource>);

enum OutlineSource {
    /// Outlines listed by name.
    Listed(std::collections::HashMap<String, Arc<BezPath>>),
    /// The glyph list's outlines, in its order, with its name index.
    /// Sharing the index means an edit copies no names, only one pointer per glyph.
    Model {
        index: Arc<std::collections::HashMap<String, usize>>,
        outlines: Vec<Arc<BezPath>>,
    },
    /// The live masters interpolated at one location between them.
    Interpolated(InterpolatedOutlines),
}

/// One master's drawn outlines, as the interpolated text reads them.
enum MasterOutlineSet {
    /// The active master: the glyph list's outlines, in its order, with its name index.
    Model {
        index: Arc<std::collections::HashMap<String, usize>>,
        outlines: Vec<Arc<BezPath>>,
    },
    /// Another master, by glyph name.
    Listed(crate::application::font_model::MasterOutlines),
}

impl MasterOutlineSet {
    fn get(&self, name: &str) -> Option<&Arc<BezPath>> {
        match self {
            Self::Model { index, outlines } => outlines.get(*index.get(name)?),
            Self::Listed(outlines) => outlines.get(name),
        }
    }
}

/// The outlines of the live masters, blended at one location for each glyph alone.
///
/// A glyph is drawn the first time text shows it; drawing every glyph after each edit would
/// take longer than the rest of a rebuild.
/// A glyph whose outlines do not have the same structure in every master that has it, or that
/// has no layer at the default location, is drawn from the master nearest to the location,
/// as its layout values are.
struct InterpolatedOutlines {
    /// Each master's normalized location and outlines, in master order.
    masters: Vec<(runebender::font::var_model::Location, MasterOutlineSet)>,
    target: runebender::font::var_model::Location,
    /// The active master, which wins a tie for the nearest master.
    preferred: usize,
    /// The weights of each set of masters, by their indexes.
    weights: std::sync::Mutex<std::collections::HashMap<Vec<usize>, Option<Vec<f64>>>>,
    drawn: std::sync::Mutex<std::collections::HashMap<String, Option<Arc<BezPath>>>>,
}

impl InterpolatedOutlines {
    fn contains(&self, name: &str) -> bool {
        self.masters
            .iter()
            .any(|(_, outlines)| outlines.get(name).is_some())
    }

    fn get(&self, name: &str) -> Option<Arc<BezPath>> {
        use runebender::font::var_model::{VariationModel, locations_match, squared_distance};
        let mut drawn = self.drawn.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(found) = drawn.get(name) {
            return found.clone();
        }
        let present: Vec<(usize, &Arc<BezPath>)> = self
            .masters
            .iter()
            .enumerate()
            .filter_map(|(index, (_, outlines))| Some((index, outlines.get(name)?)))
            .collect();
        let outline = if present.is_empty() {
            None
        } else if let Some((_, exact)) = present
            .iter()
            .find(|(index, _)| locations_match(&self.masters[*index].0, &self.target))
        {
            Some((*exact).clone())
        } else {
            let indexes: Vec<usize> = present.iter().map(|(index, _)| *index).collect();
            let weights = self
                .weights
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .entry(indexes.clone())
                .or_insert_with(|| {
                    let locations: Vec<_> = indexes
                        .iter()
                        .map(|index| self.masters[*index].0.clone())
                        .collect();
                    VariationModel::new(&locations)
                        .and_then(|model| model.weights(&self.target))
                        .ok()
                })
                .clone();
            let paths: Vec<&BezPath> = present.iter().map(|(_, path)| path.as_ref()).collect();
            weights
                .and_then(|weights| runebender::outline::glyph_paths::blend_paths(&paths, &weights))
                .map(Arc::new)
                .or_else(|| {
                    // The nearest master, with the active one winning a tie.
                    present
                        .iter()
                        .min_by(|(a, _), (b, _)| {
                            let distance = |index: usize| {
                                squared_distance(&self.masters[index].0, &self.target)
                            };
                            distance(*a)
                                .total_cmp(&distance(*b))
                                .then((*a != self.preferred).cmp(&(*b != self.preferred)))
                        })
                        .map(|(_, path)| (*path).clone())
                })
        };
        drawn.insert(name.to_owned(), outline.clone());
        outline
    }
}

impl Default for GlyphOutlines {
    fn default() -> Self {
        Self(Arc::new(OutlineSource::Listed(
            std::collections::HashMap::new(),
        )))
    }
}

impl PartialEq for GlyphOutlines {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl GlyphOutlines {
    /// The live masters of `font`, each glyph interpolated alone at `target` when text shows it.
    fn interpolated(font: &FontModel, target: runebender::font::var_model::Location) -> Self {
        let active = font.active();
        let masters = font
            .project
            .master_locations()
            .iter()
            .enumerate()
            .filter_map(|(index, location)| {
                let outlines = if index == active {
                    MasterOutlineSet::Model {
                        index: font.name_index(),
                        outlines: font
                            .glyphs
                            .iter()
                            .map(|glyph| glyph.outline.clone())
                            .collect(),
                    }
                } else {
                    MasterOutlineSet::Listed(font.master_outlines(index)?)
                };
                Some((location.clone(), outlines))
            })
            .collect();
        Self(Arc::new(OutlineSource::Interpolated(
            InterpolatedOutlines {
                masters,
                target,
                preferred: active,
                weights: std::sync::Mutex::default(),
                drawn: std::sync::Mutex::default(),
            },
        )))
    }

    fn contains(&self, name: &str) -> bool {
        match &*self.0 {
            OutlineSource::Listed(outlines) => outlines.contains_key(name),
            OutlineSource::Model { index, .. } => index.contains_key(name),
            OutlineSource::Interpolated(interpolated) => interpolated.contains(name),
        }
    }

    fn get(&self, name: &str) -> Option<Arc<BezPath>> {
        match &*self.0 {
            OutlineSource::Listed(outlines) => outlines.get(name).cloned(),
            OutlineSource::Model { index, outlines } => outlines.get(*index.get(name)?).cloned(),
            OutlineSource::Interpolated(interpolated) => interpolated.get(name),
        }
    }
}

/// The text between masters at one location: the interpolated outlines, and the inventory with
/// the interpolated advances.
pub(crate) struct CachedInterpolatedText {
    /// The document revision, the active master, the glyph count and the location bits.
    key: (u64, usize, usize, Vec<u64>),
    outlines: GlyphOutlines,
    /// The advances of the live layout at the location.
    advances: Arc<std::collections::HashMap<String, f64>>,
    /// The inventory these advances were applied to, and the result. Applying them walks every
    /// glyph, so it is done once for an inventory and a set of advances, not on every rebuild.
    base_inventory: Arc<TextGlyphInventory>,
    inventory: Arc<TextGlyphInventory>,
}

impl TextInputs {
    /// Read a master: glyph advances, kerning, outlines, metrics.
    pub(crate) fn new(font: &FontModel) -> Self {
        let key = (
            font.project.document_revision(),
            font.active(),
            font.glyphs.len(),
        );
        let mut cache = font
            .text_inputs
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(cached) = cache.as_ref().filter(|cached| cached.key == key) {
            return cached.inputs.clone();
        }
        let inputs = cache
            .as_ref()
            .and_then(|cached| Self::follow_layer_edits(font, cached, key))
            .unwrap_or_else(|| Self::read(font));
        *cache = Some(CachedTextInputs {
            key,
            inputs: inputs.clone(),
        });
        inputs
    }

    /// The inputs after layer edits to a few glyphs, made from the inputs before them.
    ///
    /// Reading the inputs generates mark features from every glyph's anchors, which is most of
    /// its cost. A layer edit that changed no anchors, components or code points leaves the
    /// features and the code point map as they were, so only the edited glyphs' advances and
    /// the outline list are read again.
    /// `None` when the inputs have to be read in full.
    fn follow_layer_edits(
        font: &FontModel,
        cached: &CachedTextInputs,
        key: (u64, usize, usize),
    ) -> Option<Self> {
        // Another master or a changed glyph list is a different inventory.
        if (cached.key.1, cached.key.2) != (key.1, key.2) {
            return None;
        }
        let changes = font.project.layer_changes_since(cached.key.0)?;
        if changes
            .iter()
            .any(|change| change.codepoints || change.mark_inputs)
        {
            return None;
        }
        let source = font
            .project
            .document_sources()
            .nth(font.project.active)?
            .id();
        let mut inputs = cached.inputs.clone();
        if changes.iter().any(|change| change.metrics) {
            let mut inventory = (*inputs.inventory).clone();
            inventory.refresh_widths(
                &font.project,
                source,
                changes
                    .iter()
                    .filter(|change| change.metrics)
                    .map(|change| change.glyph.as_str()),
            )?;
            inputs.inventory = Arc::new(inventory);
        }
        inputs.outlines = Self::model_outlines(font);
        Some(inputs)
    }

    /// Every glyph's outline as the glyph list holds it.
    fn model_outlines(font: &FontModel) -> GlyphOutlines {
        GlyphOutlines(Arc::new(OutlineSource::Model {
            index: font.name_index(),
            outlines: font
                .glyphs
                .iter()
                .map(|glyph| glyph.outline.clone())
                .collect(),
        }))
    }

    fn read(font: &FontModel) -> Self {
        let source = font
            .project
            .document_sources()
            .nth(font.project.active)
            .expect("the active source exists")
            .id();
        Self {
            context_id: (0, 0),
            inventory: Arc::new(
                TextGlyphInventory::from_project(&font.project, source)
                    .expect("the active source has canonical text inputs"),
            ),
            kerning: Arc::new(
                TextKerningModel::from_project(&font.project, source)
                    .expect("the active source has canonical kerning inputs"),
            ),
            outlines: Self::model_outlines(font),
            live_outline: None,
            compiled: None,
            live_layout: None,
            normalized: Vec::new(),
            line_height: (font.units_per_em().max(font.ascender()) - font.descender()).max(1.0),
            ascender: font.ascender(),
            descender: font.descender(),
            initial: String::new(),
            active_glyph: None,
            initial_selection: None,
            direction: None,
            feature_overrides: Vec::new(),
            script: None,
            language: None,
        }
    }

    pub(crate) fn for_editor(app: &crate::application::workspace::Workspace) -> Self {
        let inputs = Self::for_glyph(&app.font, app.session.root_glyph());
        inputs.located(&app.font, &app.axis_values)
    }

    /// Read a master and remember the glyph whose editor tab is open.
    pub(crate) fn for_glyph(font: &FontModel, glyph_name: &str) -> Self {
        let mut inputs = Self::new(font);
        inputs.active_glyph = font
            .glyphs
            .iter()
            .find(|glyph| glyph.name == glyph_name)
            .map(|glyph| (glyph.name.clone(), glyph.codepoint, glyph.advance));
        inputs
    }

    /// Preview the live font at the sliders' user coordinates.
    ///
    /// Advances, kerning, marks and substitutions come from layout tables built from live
    /// values at the location; see [`FontModel::live_layout`].
    /// Outlines come from the live source at a master, and between masters from the live
    /// masters, each glyph interpolated alone.
    /// No compiled font takes part, so a glyph that does not interpolate changes only itself.
    pub(crate) fn with_location(self, font: &FontModel, values: &[f64]) -> Self {
        self.located(font, values)
    }

    fn located(mut self, font: &FontModel, values: &[f64]) -> Self {
        if font.glyphs.is_empty() {
            return self;
        }
        self.normalized = font.normalized_values(values);
        // The tables come from the live values at this location, built in this call.
        // When the feature code does not compile, they are the last that did.
        let live = font.live_layout(values);
        if let Some(tables) = live.font.clone() {
            self.compiled = Some(tables);
            self.live_layout = live.source.clone();
        }
        // At a master the advances in the inventory and the outlines are already live.
        if font.at_active_master(values) {
            return self;
        }
        let key = (
            font.project.document_revision(),
            font.active(),
            font.glyphs.len(),
            self.normalized
                .iter()
                .map(|value| value.to_bits())
                .collect(),
        );
        let mut cache = font
            .interpolated_text
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let outlines = match cache.as_ref().filter(|cached| cached.key == key) {
            Some(cached) => cached.outlines.clone(),
            None => GlyphOutlines::interpolated(font, font.normalized_location(values)),
        };
        let inventory = match cache.as_ref().filter(|cached| {
            Arc::ptr_eq(&cached.advances, &live.advances)
                && Arc::ptr_eq(&cached.base_inventory, &self.inventory)
        }) {
            Some(cached) => cached.inventory.clone(),
            None => {
                let mut inventory = (*self.inventory).clone();
                for (name, advance) in live.advances.iter() {
                    inventory.set_advance(name.clone(), *advance);
                }
                Arc::new(inventory)
            }
        };
        *cache = Some(CachedInterpolatedText {
            key,
            outlines: outlines.clone(),
            advances: live.advances.clone(),
            base_inventory: self.inventory.clone(),
            inventory: inventory.clone(),
        });
        if let Some((active, _, width)) = &mut self.active_glyph
            && let Some(advance) = live.advances.get(active.as_str())
        {
            *width = *advance;
        }
        self.outlines = outlines;
        self.inventory = inventory;
        self
    }

    /// Substitute the editor's live draft without recompiling the shaping font.
    pub(crate) fn with_live_outline(mut self, name: &str, outline: Arc<BezPath>) -> Self {
        // Kept beside the shared outlines; replacing one entry in them would copy them all.
        if self.outlines.contains(name) {
            self.live_outline = Some((name.to_owned(), outline));
        }
        self
    }

    /// Associate these inputs with one document tab's parked text buffer.
    pub(crate) fn with_context(mut self, context_id: (u64, u64)) -> Self {
        self.context_id = context_id;
        self
    }

    pub(crate) fn same_context(&self, other: &Self) -> bool {
        self.context_id == other.context_id
    }
    /// The app changed the line independently of the widget's own typing.
    pub(crate) fn replaced_text(&self, previous: &Self) -> bool {
        self.initial != previous.initial
    }

    pub(crate) fn initial_text(&self) -> &str {
        &self.initial
    }

    /// Set the writing direction, or clear it back to automatic.
    pub(crate) fn with_direction(mut self, direction: Option<TextDirection>) -> Self {
        self.direction = direction;
        self
    }

    /// Start the buffer with some text. `RUNEBENDER_TEXT` uses this, so
    /// a headless render can show a shaped line without typing.
    pub(crate) fn with_text(mut self, text: &str) -> Self {
        self.initial = text.to_string();
        self
    }

    /// Start with a logical text range selected.
    pub(crate) fn with_selection(mut self, selection: Option<(usize, usize)>) -> Self {
        self.initial_selection = selection;
        self
    }

    /// Apply preview-only OpenType feature and locale choices.
    pub(crate) fn with_shaping_options(
        mut self,
        disabled: &std::collections::HashSet<String>,
        script: Option<&str>,
        language: Option<&str>,
    ) -> Self {
        self.feature_overrides = disabled.iter().map(|tag| (tag.clone(), false)).collect();
        self.feature_overrides.sort();
        self.script = script.map(str::to_string);
        self.language = language.map(str::to_string);
        self
    }
}

/// The buffer plus everything the editor needs to draw it.
pub(crate) struct TextState {
    pub buffer: TextBuffer,
    /// Native IME composition shown in the line but not committed to the
    /// document text until the platform sends `Ime::Commit`.
    pub preedit: String,
    /// Line height in design units, from the master's metrics.
    pub line_height: f64,
    /// The master's ascender and descender, which the engine needs to
    /// work out which line a click landed on.
    ascender: f64,
    descender: f64,
    /// Outlines by glyph name, so painting does not touch the font.
    outlines: GlyphOutlines,
    live_outline: Option<(String, Arc<BezPath>)>,
    /// What the line is shaped from when no gesture is in progress.
    committed: CommittedLayout,
    /// The values of the gesture in progress that the line is shaped with; empty without one.
    overrides: LayoutOverrides,
}

/// The shaping inputs of the line as the app supplied them, from committed values.
struct CommittedLayout {
    compiled: Option<Arc<Vec<u8>>>,
    normalized: Vec<f64>,
    inventory: Arc<TextGlyphInventory>,
    source: Option<LiveLayoutSource>,
}

impl CommittedLayout {
    fn of(inputs: &TextInputs) -> Self {
        Self {
            compiled: inputs.compiled.clone(),
            normalized: inputs.normalized.clone(),
            inventory: inputs.inventory.clone(),
            source: inputs.live_layout.clone(),
        }
    }
}

impl TextState {
    #[cfg(test)]
    pub(crate) fn test_buffer() -> Self {
        let mut font = norad::Font::new();
        for (name, codepoint, width) in
            [("A", 'A', 500.0), ("B", 'B', 500.0), ("space", ' ', 250.0)]
        {
            let mut glyph = norad::Glyph::new(name);
            glyph.width = width;
            glyph.codepoints.insert(codepoint);
            font.default_layer_mut().insert_glyph(glyph);
        }
        let project = runebender::font::project::Project::from_source(
            runebender::font::project::SourceInput::from_font(font, "text-test.ufo".into()),
        )
        .expect("valid source fixture");
        let source = project.source_id(0).unwrap();
        Self::new(&TextInputs {
            context_id: (0, 0),
            inventory: TextGlyphInventory::from_project(&project, source)
                .unwrap()
                .into(),
            kerning: TextKerningModel::from_project(&project, source)
                .unwrap()
                .into(),
            outlines: GlyphOutlines::default(),
            live_outline: None,
            compiled: None,
            live_layout: None,
            normalized: Vec::new(),
            line_height: 1000.0,
            ascender: 800.0,
            descender: -200.0,
            initial: "A".into(),
            active_glyph: Some(("A".into(), Some('A'), 500.0)),
            initial_selection: None,
            direction: None,
            feature_overrides: Vec::new(),
            script: None,
            language: None,
        })
    }

    /// A buffer wired to a master.
    pub(crate) fn new(inputs: &TextInputs) -> Self {
        let mut buffer = TextBuffer::new();
        buffer.set_shared_glyph_inventory(inputs.inventory.clone());
        buffer.set_compiled_font(inputs.compiled.clone(), inputs.normalized.clone());
        buffer.set_shared_kerning_model(inputs.kerning.clone());
        buffer.set_feature_overrides(inputs.feature_overrides.clone());
        buffer.set_shaping_locale(inputs.script.clone(), inputs.language.clone());
        match inputs.direction {
            Some(direction) => buffer.set_direction(direction),
            None => buffer.set_auto_direction(),
        }
        // The stored text holds a line break as a line ending, so restore it as one.
        buffer.insert_text(&inputs.initial);
        if let Some((name, codepoint, advance)) = &inputs.active_glyph {
            let existing = {
                buffer
                    .iter()
                    .position(|sort| sort.glyph_name() == Some(name.as_str()))
            };
            match existing {
                Some(index) => {
                    buffer.activate_sort(index);
                }
                None if buffer.is_empty() => {
                    // An empty line still needs the open glyph as an edit target.
                    buffer.insert_glyph(name.clone(), *codepoint, *advance);
                    buffer.shape_arabic_if_rtl();
                }
                None => {} // Preserve a supplied line without the open glyph.
            }
        }
        if let Some((start, end)) = inputs.initial_selection {
            buffer.select_range(start, end);
        }
        Self {
            buffer,
            preedit: String::new(),
            line_height: inputs.line_height,
            ascender: inputs.ascender,
            descender: inputs.descender,
            outlines: inputs.outlines.clone(),
            live_outline: inputs.live_outline.clone(),
            committed: CommittedLayout::of(inputs),
            overrides: LayoutOverrides::default(),
        }
    }

    /// Re-read the master, keeping what has been typed. Switching master
    /// or editing a glyph changes advances and outlines, and a text line
    /// that does not follow is showing yesterday's spacing.
    pub(crate) fn refresh(&mut self, inputs: &TextInputs) {
        self.buffer
            .set_shared_glyph_inventory(inputs.inventory.clone());
        self.buffer
            .set_compiled_font(inputs.compiled.clone(), inputs.normalized.clone());
        self.buffer.set_shared_kerning_model(inputs.kerning.clone());
        self.buffer
            .set_feature_overrides(inputs.feature_overrides.clone());
        self.buffer
            .set_shaping_locale(inputs.script.clone(), inputs.language.clone());
        let direction_changed = match inputs.direction {
            Some(direction) => {
                let changed =
                    self.buffer.direction_is_auto() || self.buffer.direction() != direction;
                self.buffer.set_direction(direction);
                changed
            }
            None => {
                let changed = !self.buffer.direction_is_auto();
                self.buffer.set_auto_direction();
                changed
            }
        };
        if direction_changed {
            self.buffer.shape_arabic_if_rtl();
        }
        self.buffer.refresh_shaping();
        self.outlines = inputs.outlines.clone();
        self.live_outline = inputs.live_outline.clone();
        self.line_height = inputs.line_height;
        self.ascender = inputs.ascender;
        self.descender = inputs.descender;
        // New inputs hold every committed value; a gesture still in progress applies its
        // values again through `follow`.
        self.committed = CommittedLayout::of(inputs);
        self.overrides = LayoutOverrides::default();
    }

    /// Whether the line is shaped by the live layout, which `follow` can rebuild with the
    /// values of a gesture.
    pub(crate) fn has_live_layout(&self) -> bool {
        self.committed.source.is_some()
    }

    /// Whether an edit of `layer` changes the layout values that the line is shaped from.
    pub(crate) fn reads_layer(&self, layer: &runebender::font::variable::LayerId) -> bool {
        self.committed
            .source
            .as_ref()
            .is_some_and(|source| source.reads_layer(layer))
    }

    /// Shape the line with the values of a gesture in progress in place of the committed ones,
    /// at a master or between masters.
    ///
    /// The shaping font comes from the same builder and the same call as the committed one,
    /// so a drag step and the commit of the same values give the same layout.
    /// Empty overrides restore the committed font.
    /// Overrides equal to the committed values build nothing.
    /// Returns whether the shaping inputs changed.
    pub(crate) fn follow(&mut self, overrides: &LayoutOverrides) -> bool {
        let Some(source) = &self.committed.source else {
            return false;
        };
        // The values of the edited master, at the displayed location, with the composites that
        // take dragged anchors.
        let overrides = source.locate(overrides);
        if overrides == self.overrides {
            return false;
        }
        let (compiled, inventory) = if overrides.is_empty() {
            (
                self.committed.compiled.clone(),
                self.committed.inventory.clone(),
            )
        } else {
            let Some(font) = source.build(&overrides) else {
                return false;
            };
            // Nominal widths, and the right edge of a right-to-left line, read the inventory.
            let mut inventory = (*self.committed.inventory).clone();
            for glyph in &overrides.glyphs {
                if inventory.advance(&glyph.name).is_some() {
                    inventory.set_advance(glyph.name.clone(), glyph.advance);
                }
            }
            (Some(font), Arc::new(inventory))
        };
        self.overrides = overrides;
        self.buffer.set_shared_glyph_inventory(inventory);
        self.buffer
            .set_compiled_font(compiled, self.committed.normalized.clone());
        self.buffer.refresh_shaping();
        true
    }

    /// The active sort and its layout origin.
    pub(crate) fn active_placement(&self) -> Option<(usize, Point)> {
        Some((self.buffer.active_sort()?, self.active_origin()?))
    }

    fn outline(&self, name: &str) -> Option<Arc<BezPath>> {
        match &self.live_outline {
            Some((live, path)) if live == name => Some(path.clone()),
            _ => self.outlines.get(name),
        }
    }

    /// Type a character. Returns false when the font has no glyph for it,
    /// which is worth knowing rather than silently swallowing.
    pub(crate) fn insert(&mut self, character: char) -> bool {
        let inserted = self.buffer.insert_character(character);
        if inserted {
            self.buffer.shape_arabic_if_rtl();
        }
        inserted
    }

    /// Replace the native IME composition without changing the committed
    /// buffer. An empty preedit is cancellation.
    pub(crate) fn set_preedit(&mut self, text: String) {
        self.preedit = text;
    }

    /// Commit one native IME result exactly once and clear its preview.
    pub(crate) fn commit_preedit(&mut self, committed: &str) -> bool {
        self.preedit.clear();
        let mut changed = false;
        let normalized = committed.replace("\r\n", "\n").replace('\r', "\n");
        for character in normalized.chars() {
            if character == '\n' {
                self.buffer.insert_line_break();
                changed = true;
            } else {
                changed |= self.insert(character);
            }
        }
        changed
    }

    /// Every sort to draw, sharing its untransformed outline and carrying its line origin.
    ///
    /// Absorbed sorts (a character folded into a ligature drawn by an
    /// earlier sort) contribute nothing, which is what `is_absorbed` is
    /// for.
    pub(crate) fn placed(&self) -> Vec<PlacedSort> {
        let display;
        let buffer = if self.preedit.is_empty() {
            &self.buffer
        } else {
            display = {
                let mut buffer = self.buffer.clone();
                for character in self.preedit.chars() {
                    buffer.insert_character(character);
                }
                buffer.shape_arabic_if_rtl();
                buffer
            };
            &display
        };
        let layout = buffer.layout(self.line_height);
        let active = buffer.active_sort();
        let selection = buffer.selection_range();
        layout
            .items
            .iter()
            .filter_map(|item| {
                let sort = buffer.sort(item.index)?;
                if sort.is_absorbed() {
                    return None;
                }
                let name = sort.glyph_name()?;
                let outline = self.outline(name)?;
                let cluster_end = ((item.index + 1)..buffer.len())
                    .find(|index| !buffer.sort(*index).is_some_and(|sort| sort.is_absorbed()))
                    .unwrap_or(buffer.len());
                let selected = selection
                    .as_ref()
                    .is_some_and(|range| range.start < cluster_end && range.end > item.index);
                Some(PlacedSort {
                    index: item.index,
                    outline: outline.clone(),
                    origin: Point::new(item.x, item.y),
                    advance: item.advance_width,
                    nominal_width: item.nominal_width,
                    spacing_before: item.spacing_before,
                    active: active == Some(item.index),
                    selected,
                })
            })
            .collect()
    }

    /// Where the caret sits, in design space.
    pub(crate) fn caret(&self) -> Point {
        let display;
        let buffer = if self.preedit.is_empty() {
            &self.buffer
        } else {
            display = {
                let mut buffer = self.buffer.clone();
                for character in self.preedit.chars() {
                    buffer.insert_character(character);
                }
                buffer.shape_arabic_if_rtl();
                buffer
            };
            &display
        };
        let layout = buffer.layout(self.line_height);
        Point::new(layout.cursor_x, layout.cursor_y)
    }

    /// Layout origin of the sort whose glyph is open in the outline editor.
    pub(crate) fn active_origin(&self) -> Option<Point> {
        let active = self.buffer.active_sort()?;
        self.buffer
            .layout(self.line_height)
            .items
            .iter()
            .find(|item| item.index == active)
            .map(|item| Point::new(item.x, item.y))
    }

    /// Prefer visible ink where two heavily kerned advance boxes overlap.
    pub(crate) fn kern_hit(&self, at: Point) -> Option<usize> {
        use masonry::kurbo::Shape;
        let layout = self.buffer.layout(self.line_height);
        for item in layout.items.iter().rev() {
            let Some(name) = self.buffer.sort(item.index).and_then(|s| s.glyph_name()) else {
                continue;
            };
            if self
                .outline(name)
                .is_some_and(|p| p.winding(Point::new(at.x - item.x, at.y - item.y)) != 0)
            {
                return Some(item.index);
            }
        }
        self.buffer
            .hit_test(at.x, at.y, self.line_height, self.ascender, self.descender)
            .active_sort
    }

    /// A click: put the caret there, and report the sort under it.
    pub(crate) fn click(&mut self, at: Point) -> Option<usize> {
        let hit = self
            .buffer
            .hit_test(at.x, at.y, self.line_height, self.ascender, self.descender);
        self.buffer
            .place_cursor_at(at.x, at.y, self.line_height, self.ascender, self.descender);
        hit.active_sort
    }

    /// Activate the sort box under `at` without moving the text caret.
    pub(crate) fn activate_at(&mut self, at: Point) -> Option<String> {
        let activation = self.buffer.activate_sort_at(
            at.x,
            at.y,
            self.line_height,
            self.ascender,
            self.descender,
        )?;
        self.buffer
            .sort(activation.index)
            .and_then(|sort| sort.glyph_name())
            .map(str::to_string)
    }

    /// Make a sort the one being edited, and report its glyph.
    pub(crate) fn activate(&mut self, index: usize) -> Option<String> {
        self.buffer.activate_sort(index).then(|| {
            self.buffer
                .sort(index)
                .and_then(|sort| sort.glyph_name())
                .map(str::to_string)
        })?
    }
}

/// One sort, ready to draw.
pub(crate) struct PlacedSort {
    pub nominal_width: f64,
    pub spacing_before: f64,
    pub index: usize,
    pub outline: Arc<BezPath>,
    pub origin: Point,
    pub advance: f64,
    pub active: bool,
    pub selected: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "integration-fonts")]
    use masonry::kurbo::Shape;

    /// A font with two glyphs of width 500, opened without files.
    fn two_glyph_model() -> FontModel {
        use runebender::font::project::{Project, SourceInput};
        let mut font = norad::Font::new();
        for (name, character) in [("A", 'A'), ("B", 'B')] {
            let mut glyph = norad::Glyph::new(name);
            glyph.width = 500.0;
            glyph.codepoints = norad::Codepoints::new([character]);
            font.default_layer_mut().insert_glyph(glyph);
        }
        let project = Project::from_source(SourceInput::from_font(font, "Two.ufo".into())).unwrap();
        FontModel::from_project(project)
    }

    /// Whether two inputs hold the same glyph data, whatever they share.
    /// `==` treats outline sources built separately as different.
    fn same_contents(a: &TextInputs, b: &TextInputs) -> bool {
        let outlines = |inputs: &TextInputs| {
            ["A", "B", "missing"].map(|name| inputs.outlines.get(name).map(|path| (*path).clone()))
        };
        a.inventory == b.inventory
            && a.kerning == b.kerning
            && outlines(a) == outlines(b)
            && a.line_height == b.line_height
            && a.active_glyph == b.active_glyph
    }

    #[test]
    fn inputs_follow_a_width_edit_without_a_full_read() {
        let mut font = two_glyph_model();
        let layer = font
            .project
            .document_sources()
            .next()
            .unwrap()
            .default_layer();
        let before = TextInputs::new(&font);
        let revision = font.project.document_revision();

        font.project
            .edit_document_layer("A", &layer, |draft| {
                draft.set_width(640.0)?;
                Ok(())
            })
            .unwrap();
        // The edit is one the inputs can follow: no code point, anchor or component changed.
        let changes = font.project.layer_changes_since(revision).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].glyph, "A");
        assert!(changes[0].metrics && !changes[0].codepoints && !changes[0].mark_inputs);

        let followed = TextInputs::new(&font);
        assert!(followed != before, "the new width is in the inputs");
        assert!(
            same_contents(&followed, &TextInputs::read(&font)),
            "following equals reading in full"
        );
        // The kerning model and the feature text were not rebuilt.
        assert!(Arc::ptr_eq(&followed.kerning, &before.kerning));

        // Nothing changed since the current revision, and a revision that never existed, or
        // one before the journal starts, cannot be followed.
        let current = font.project.document_revision();
        assert_eq!(font.project.layer_changes_since(current), Some(Vec::new()));
        assert_eq!(font.project.layer_changes_since(current + 1), None);
        assert_eq!(
            font.project.layer_changes_since(revision.wrapping_sub(1)),
            None
        );
    }

    #[test]
    fn inputs_are_read_in_full_after_a_code_point_edit() {
        let mut font = two_glyph_model();
        let layer = font
            .project
            .document_sources()
            .next()
            .unwrap()
            .default_layer();
        let before = TextInputs::new(&font);
        let revision = font.project.document_revision();
        font.project
            .edit_document_layer("A", &layer, |draft| {
                draft.set_codepoints(['Z']);
                Ok(())
            })
            .unwrap();
        let changes = font.project.layer_changes_since(revision).unwrap();
        assert!(changes[0].codepoints);
        let after = TextInputs::new(&font);
        assert!(same_contents(&after, &TextInputs::read(&font)));
        assert!(
            !Arc::ptr_eq(&after.kerning, &before.kerning),
            "a full read rebuilds it"
        );
    }

    #[test]
    fn shaping_options_reach_the_widget_buffer() {
        let disabled = std::collections::HashSet::from(["liga".to_string(), "kern".to_string()]);
        let inputs = TextInputs {
            context_id: (0, 0),
            inventory: TextGlyphInventory::default().into(),
            kerning: TextKerningModel::default().into(),
            outlines: GlyphOutlines::default(),
            live_outline: None,
            compiled: None,
            live_layout: None,
            normalized: Vec::new(),
            line_height: 1000.0,
            ascender: 800.0,
            descender: -200.0,
            initial: String::new(),
            active_glyph: None,
            initial_selection: None,
            direction: None,
            feature_overrides: Vec::new(),
            script: None,
            language: None,
        }
        .with_shaping_options(&disabled, Some("arab"), Some("ur"));

        let state = TextState::new(&inputs);

        assert_eq!(
            state.buffer.feature_overrides(),
            &[("kern".into(), false), ("liga".into(), false)]
        );
        assert_eq!(state.buffer.shaping_locale(), (Some("arab"), Some("ur")));
    }

    #[test]
    fn empty_text_starts_with_the_open_glyph_active() {
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("five");
        glyph.width = 612.0;
        glyph.codepoints.insert('5');
        font.default_layer_mut().insert_glyph(glyph);
        let mut other = norad::Glyph::new("A");
        other.width = 500.0;
        other.codepoints.insert('A');
        font.default_layer_mut().insert_glyph(other);
        let project = runebender::font::project::Project::from_source(
            runebender::font::project::SourceInput::from_font(font, "text-test.ufo".into()),
        )
        .expect("valid source fixture");
        let source = project.source_id(0).unwrap();
        let mut inputs = TextInputs {
            context_id: (0, 0),
            inventory: TextGlyphInventory::from_project(&project, source)
                .unwrap()
                .into(),
            kerning: TextKerningModel::default().into(),
            outlines: GlyphOutlines::default(),
            live_outline: None,
            compiled: None,
            live_layout: None,
            normalized: Vec::new(),
            line_height: 1000.0,
            ascender: 800.0,
            descender: -200.0,
            initial: String::new(),
            active_glyph: Some(("five".into(), Some('5'), 612.0)),
            initial_selection: None,
            direction: None,
            feature_overrides: Vec::new(),
            script: None,
            language: None,
        };
        let state = TextState::new(&inputs);

        assert_eq!(state.buffer.len(), 1);
        assert_eq!(state.buffer.active_sort(), Some(0));
        assert_eq!(state.buffer.sort(0).unwrap().glyph_name(), Some("five"));

        inputs.initial = "5".into();
        let state = TextState::new(&inputs);
        assert_eq!(state.buffer.len(), 1, "the open glyph is not duplicated");
        assert_eq!(state.buffer.active_sort(), Some(0));

        inputs.initial = "A".into();
        let state = TextState::new(&inputs);
        assert_eq!(state.buffer.text(), "A", "a supplied line is not discarded");
        assert_eq!(state.buffer.sort(0).unwrap().glyph_name(), Some("A"));
    }

    #[test]
    fn non_unicode_open_glyph_is_still_seeded() {
        let inputs = TextInputs {
            context_id: (0, 0),
            inventory: TextGlyphInventory::default().into(),
            kerning: TextKerningModel::default().into(),
            outlines: GlyphOutlines::default(),
            live_outline: None,
            compiled: None,
            live_layout: None,
            normalized: Vec::new(),
            line_height: 1000.0,
            ascender: 800.0,
            descender: -200.0,
            initial: String::new(),
            active_glyph: Some(("alternate.001".into(), None, 500.0)),
            initial_selection: None,
            direction: None,
            feature_overrides: Vec::new(),
            script: None,
            language: None,
        };
        let state = TextState::new(&inputs);

        assert_eq!(state.buffer.len(), 1);
        assert_eq!(
            state.buffer.sort(0).unwrap().glyph_name(),
            Some("alternate.001")
        );
        assert_eq!(state.buffer.active_sort(), Some(0));
    }

    #[cfg(feature = "integration-fonts")]
    #[test]
    fn nudging_reh_keeps_the_composed_word_in_place() {
        use crate::application::workspace::Workspace;
        let path = crate::integration_fonts::dir().join("VirtuaGrotesk.designspace");
        let mut app = Workspace::open(&path).unwrap();
        let name = "reh-ar.fina";
        app.open_glyph(app.font.index_of(name).unwrap());
        app.font
            .project
            .compiled_preview()
            .expect("initial compiled word");
        let inputs = TextInputs::for_editor(&app).with_text("فيرتوا غروتيسك");
        assert!(inputs.compiled.is_some());
        let mut text = TextState::new(&inputs);
        let positions = |t: &TextState| t.placed().iter().map(|s| s.origin).collect::<Vec<_>>();
        let before = positions(&text);
        assert!(before.len() > 4);
        let first = app.session.points()[0].point;
        app.apply_op(|s| {
            s.select_all();
            s.nudge(8.0, 0.0)
        });
        assert_eq!(app.session.points()[0].point.x, first.x + 8.0);
        let refreshed = TextInputs::for_editor(&app);
        assert!(
            refreshed.compiled.is_some(),
            "a nudge must retain compiled positioning"
        );
        text.refresh(&refreshed);
        assert_eq!(positions(&text), before);

        // A glyph whose contours no longer interpolate stops the complete preview, but not the
        // live layout tables, which read no outlines.
        let bold = app
            .font
            .project
            .document_sources()
            .nth(1)
            .expect("a second source")
            .default_layer();
        app.font
            .project
            .edit_document_layer("tet-hb", &bold, |draft| {
                draft.clear_contours();
                Ok(())
            })
            .unwrap();
        assert!(app.font.project.compile().is_err());
        let layout = app.font.project.layout_revision();
        let live_before = app.font.live_layout(&app.axis_values).font;
        // Moving contour points does not rebuild the layout tables.
        // Anchors are not selected: moving one changes mark positions, which is layout.
        let nudge_contour = |app: &mut Workspace| {
            app.apply_op(|s| {
                let contour: Vec<_> = s
                    .current_layer()
                    .and_then(|layer| layer.contours().next())
                    .map(|contour| contour.points().map(|point| point.id()).collect())
                    .unwrap_or_default();
                assert!(!contour.is_empty());
                s.select_points(contour, false);
                s.nudge(8.0, 0.0)
            });
        };
        nudge_contour(&mut app);
        assert_eq!(app.font.project.layout_revision(), layout);
        let refreshed = TextInputs::for_editor(&app);
        // At a master the line is shaped by the live layout tables, which the complete
        // compile cannot stop and which a point edit does not rebuild.
        let live = app.font.live_layout(&app.axis_values);
        assert!(live.error.is_none());
        assert!(Arc::ptr_eq(
            refreshed.compiled.as_ref().expect("live layout tables"),
            live.font.as_ref().expect("live layout tables"),
        ));
        assert!(Arc::ptr_eq(
            live_before.as_ref().expect("live layout tables"),
            live.font.as_ref().expect("live layout tables"),
        ));
        text.refresh(&refreshed);
        assert_eq!(positions(&text), before);

        // Between masters, with tet-hb still incompatible, the line is shaped by tables
        // interpolated from the live masters, and a contour nudge does not move it.
        let regular = app.font.master_axis_values(0);
        let heavy = app.font.master_axis_values(1);
        app.axis_values = regular
            .iter()
            .zip(&heavy)
            .map(|(a, b)| (a + b) / 2.0)
            .collect();
        assert!(!app.font.at_active_master(&app.axis_values));
        let between = TextInputs::for_editor(&app).with_text("فيرتوا غروتيسك");
        assert!(between.compiled.is_some(), "a shaping font between masters");
        let live = app.font.live_layout(&app.axis_values);
        assert!(live.error.is_none(), "{:?}", live.error);
        let mut text = TextState::new(&between);
        let before = positions(&text);
        assert!(before.len() > 4);
        nudge_contour(&mut app);
        assert_eq!(app.font.project.layout_revision(), layout);
        let refreshed = TextInputs::for_editor(&app);
        assert!(Arc::ptr_eq(
            refreshed.compiled.as_ref().expect("live layout tables"),
            between.compiled.as_ref().expect("live layout tables"),
        ));
        text.refresh(&refreshed);
        assert_eq!(positions(&text), before);
    }

    #[cfg(feature = "integration-fonts")]
    #[test]
    fn virtua_mixed_text_uses_real_arabic_forms_marks_and_bidi_layout() {
        let source = crate::integration_fonts::dir().join("VirtuaGrotesk.designspace");
        let mut font = FontModel::open(&source).expect("Virtua Grotesk opens");
        let sample = "R لا 123 بِ";
        let mut state = TextState::new(&TextInputs::new(&font).with_text(sample));

        assert_eq!(
            state.buffer.len(),
            sample.chars().count(),
            "the actual Regular master covers every sample character"
        );
        assert!(
            state
                .buffer
                .iter()
                .any(|sort| sort.glyph_name() == Some("lam_alef-ar")),
            "the real feature file substitutes lam-alef"
        );
        assert!(
            state
                .buffer
                .sort(10)
                .and_then(|sort| sort.glyph_name())
                .is_some_and(|name| name.contains("kasra")),
            "the kasra remains an addressable shaped sort"
        );
        let disabled = std::collections::HashSet::from(["rlig".to_string()]);
        let no_rlig = TextState::new(
            &TextInputs::new(&font)
                .with_text(sample)
                .with_shaping_options(&disabled, Some("arab"), Some("ur")),
        );
        assert!(
            no_rlig
                .buffer
                .iter()
                .all(|sort| sort.glyph_name() != Some("lam_alef-ar")),
            "the real required-ligature override changes existing shaping"
        );
        assert!(
            !no_rlig.buffer.sort(3).unwrap().is_absorbed(),
            "disabled rlig leaves the alef independently editable"
        );
        let placed = state.placed();
        assert!(
            placed.len() < state.buffer.len(),
            "the absorbed alef remains logical text but is not painted twice"
        );
        assert!(placed.iter().all(|sort| {
            let bounds = sort.outline.bounding_box();
            bounds.x0.is_finite()
                && bounds.y0.is_finite()
                && bounds.x1.is_finite()
                && bounds.y1.is_finite()
        }));

        let layout = state.buffer.layout(state.line_height);
        let latin = layout
            .items
            .iter()
            .find(|item| item.index == 0)
            .expect("the Latin run is laid out");
        let arabic = layout
            .items
            .iter()
            .filter(|item| [2, 3, 9, 10].contains(&item.index))
            .collect::<Vec<_>>();
        assert!(!arabic.is_empty());
        assert!(
            arabic.iter().any(|item| item.x > latin.x),
            "the Arabic run occupies its bidi-resolved visual position"
        );

        let beh_name = state
            .buffer
            .sort(9)
            .and_then(|sort| sort.glyph_name())
            .expect("the shaped beh has a glyph name")
            .to_string();
        let before_advance = state
            .buffer
            .layout(state.line_height)
            .items
            .iter()
            .find(|item| item.index == 9)
            .expect("the beh remains visible")
            .advance_width;
        let beh_index = font.index_of(&beh_name).expect("the shaped beh is indexed");
        let address = font
            .active_layer_address(&beh_name)
            .expect("the shaped beh remains in the canonical source");
        let mut transaction = font
            .project
            .begin_document_layer_transaction(&address)
            .expect("the shaped beh remains editable");
        let width = transaction.draft().view().width() + 17.0;
        transaction
            .draft_mut()
            .set_width(width)
            .expect("the finite width is valid");
        font.project
            .commit_document_layer_transaction(transaction)
            .expect("the test edit commits");
        font.refresh_entry(beh_index);
        state.refresh(&TextInputs::new(&font));
        assert_eq!(
            state
                .buffer
                .layout(state.line_height)
                .items
                .iter()
                .find(|item| item.index == 9)
                .expect("the refreshed beh remains visible")
                .advance_width,
            before_advance + 17.0,
            "existing Arabic text reshapes immediately from a live glyph edit"
        );

        let lam = layout
            .items
            .iter()
            .find(|item| item.index == 2)
            .expect("the lam-alef ligature has one visible item");
        assert_eq!(
            state.click(Point::new(lam.x + lam.advance_width / 2.0, lam.y)),
            Some(2),
            "pointer hit mapping resolves the visible ligature to its logical lam"
        );
        state.buffer.select_range(2, 4);
        assert_eq!(
            state.placed().iter().filter(|sort| sort.selected).count(),
            1,
            "lam and absorbed alef share one visible selection box"
        );

        state.buffer.set_cursor(9);
        state.buffer.extend_selection_visual_right();
        assert_eq!(state.buffer.selection_range(), Some(9..10));
        assert!(
            state.placed().iter().any(|sort| sort.selected),
            "the real Arabic selection has visible geometry"
        );
        assert!(state.buffer.delete_after_cursor().is_some());
        state.buffer.shape_arabic_if_rtl();
        assert_eq!(state.buffer.len(), 10);
        assert_eq!(state.buffer.selection_range(), None);

        let mut pasted = TextState::new(&TextInputs::new(&font));
        assert!(pasted.commit_preedit("\u{0628}\u{0650}"));
        pasted.buffer.select_range(0, pasted.buffer.len());
        assert_eq!(
            pasted.buffer.selected_text().as_deref(),
            Some("\u{0628}\u{0650}"),
            "real Virtua combining-mark input round-trips as Unicode text"
        );
        assert!(
            pasted
                .buffer
                .iter()
                .any(|sort| sort.glyph_name().is_some_and(|name| name.contains("kasra"))),
            "the pasted combining mark remains a shaped sort"
        );
    }
}
