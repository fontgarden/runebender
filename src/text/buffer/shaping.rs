// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shaping the buffer: through the font's own features when they compile, and through the Arabic joining rules otherwise.

use super::*;

/// A parsed compiled preview with the bytes it was parsed from.
type ParsedPreview = (Arc<Vec<u8>>, Rc<ShapingFont>);

thread_local! {
    /// The compiled preview last parsed for shaping.
    ///
    /// Every text line builds its own buffer, and a view rebuild builds a new line, so the
    /// same compiled font would otherwise be parsed again for every rebuild. Holding the
    /// bytes keeps the pointer they are compared by from being reused.
    static PARSED_PREVIEW: RefCell<Option<ParsedPreview>> = const { RefCell::new(None) };
}

/// Parse a compiled preview font once per binary.
fn parsed_preview(bytes: &Arc<Vec<u8>>) -> Option<Rc<ShapingFont>> {
    PARSED_PREVIEW.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some((parsed_from, font)) = slot.as_ref()
            && Arc::ptr_eq(parsed_from, bytes)
        {
            return Some(font.clone());
        }
        let font = Rc::new(ShapingFont::from_bytes((**bytes).clone()).ok()?);
        *slot = Some((bytes.clone(), font.clone()));
        Some(font)
    })
}

impl TextBuffer {
    /// The compiled shaping font for the current inventory, or `None`
    /// when there is no features.fea or it does not compile. Built once
    /// and cached until the inventory changes.
    pub(super) fn shaping_font(&self) -> Option<Rc<ShapingFont>> {
        if let Some(bytes) = &self.compiled_font {
            if let Some(cached) = self.shaping_font.get() {
                return cached;
            }
            let built = parsed_preview(bytes)
                .map(|font| Rc::new((*font).clone().at_normalized(self.normalized.clone())));
            self.shaping_font.set(built.clone());
            return built;
        }
        if self.glyph_inventory.features.trim().is_empty() {
            return None;
        }
        if let Some(cached) = self.shaping_font.get() {
            return cached;
        }

        // Glyph order: every glyph the inventory knows, .notdef first so
        // it takes glyph id 0 the way a real font does.
        let mut names: Vec<&String> = self
            .glyph_inventory
            .widths
            .keys()
            .chain(self.glyph_inventory.outlines.keys())
            .collect();
        names.sort();
        names.dedup();

        let mut unicodes: HashMap<&str, Vec<u32>> = HashMap::new();
        for (codepoint, name) in &self.glyph_inventory.unicode {
            unicodes.entry(name.as_str()).or_default().push(*codepoint);
        }

        let glyphs: Vec<ShapingGlyph> = std::iter::once(".notdef")
            .chain(
                names
                    .iter()
                    .map(|name| name.as_str())
                    .filter(|name| *name != ".notdef"),
            )
            .map(|name| ShapingGlyph {
                name: name.to_string(),
                advance: self
                    .glyph_inventory
                    .widths
                    .get(name)
                    .copied()
                    .unwrap_or(0.0),
                unicodes: unicodes.get(name).cloned().unwrap_or_default(),
            })
            .collect();

        let built = ShapingFont::build(&ShapingSource {
            units_per_em: self.glyph_inventory.units_per_em,
            glyphs,
            features: self.glyph_inventory.features.clone(),
        })
        .map(Rc::new)
        .map_err(|e| {
            // Expected while the feature file is being edited; the old
            // joining rules carry on.
            log_shaping_failure(&e);
        })
        .ok();

        self.shaping_font.set(built.clone());
        built
    }

    /// Shape every line through the font's own rules.
    /// Returns `Some(changed)` on success, or `None` when the caller must fall back.
    ///
    /// Lines are split into runs first. A line mixing Latin and Arabic
    /// has to be shaped a run at a time: handed the whole line, the
    /// shaper takes its script from the first character, and the Arabic
    /// features, including the lam-alef ligature, never run.
    pub(super) fn shape_with_font(&mut self) -> Option<bool> {
        let Some(font) = self.shaping_font() else {
            self.shaped.0.clear();
            return None;
        };

        let mut updates: Vec<(usize, String, f64)> = Vec::new();
        let mut clusters: HashMap<usize, ShapedCluster> = HashMap::new();
        let mut absorbed: Vec<bool> = vec![false; self.sorts.len()];

        for line in 0..self.line_count() {
            let (line_start, line_end) = self.line_range_for_number(line);

            for bidi_run in self.visual_runs_for_line(line_start, line_end).iter() {
                // A glyph inserted by name must break shaping on either side: dropping it
                // from the input would let its neighbors join or form a ligature across it.
                for sorts in bidi_run
                    .sorts
                    .split(|&index| self.sort_codepoint(index).is_none())
                {
                    if sorts.is_empty() {
                        continue;
                    }
                    let mut text = String::new();
                    let mut sort_for_offset = Vec::new();
                    for &index in sorts {
                        let character = self.sort_codepoint(index).expect("a run of encoded sorts");
                        sort_for_offset.extend(std::iter::repeat_n(index, character.len_utf8()));
                        text.push(character);
                        absorbed[index] = true;
                    }
                    let Ok(shaped) = font.shape_with_options(
                        &text,
                        bidi_run.rtl,
                        &self.feature_overrides,
                        self.script_override.as_deref(),
                        self.language_override.as_deref(),
                    ) else {
                        self.shaped.0.clear();
                        return None;
                    };
                    // Monotone cluster values keep all outputs of a multiple substitution
                    // together. A sort with no output is absorbed into another cluster.
                    for group in shaped.chunk_by(|a, b| a.cluster == b.cluster) {
                        let index = sort_for_offset[group[0].cluster as usize];
                        let glyphs: Vec<_> = group
                            .iter()
                            .map(|glyph| ClusterGlyph {
                                name: font
                                    .glyph_name(glyph.glyph_id)
                                    .unwrap_or(".notdef")
                                    .to_owned(),
                                advance: glyph.x_advance,
                                x_offset: glyph.x_offset,
                                y_offset: glyph.y_offset,
                            })
                            .collect();
                        let own = if bidi_run.rtl { glyphs.len() - 1 } else { 0 };
                        updates.push((index, glyphs[own].name.clone(), glyphs[own].advance));
                        clusters.insert(index, ShapedCluster { glyphs, own });
                        absorbed[index] = false;
                    }
                }
            }
        }
        for (index, &is_absorbed) in absorbed.iter().enumerate() {
            if is_absorbed {
                updates.push((index, String::new(), 0.0));
            }
        }

        let changed = self.apply_shape_updates(updates);
        // Keep only what layout cannot read from the sort: an offset, or more than one glyph.
        clusters.retain(|_, cluster| {
            let (x, y) = cluster.own_offset();
            cluster.glyphs.len() > 1 || x != 0.0 || y != 0.0
        });
        let offsets_changed = clusters != self.shaped.0;
        self.shaped.0 = clusters;
        let mut absorbed_changed = offsets_changed;
        for (index, sort) in self.sorts.iter_mut().enumerate() {
            let want = absorbed.get(index).copied().unwrap_or(false);
            if sort.absorbed != want {
                sort.absorbed = want;
                absorbed_changed = true;
            }
        }
        Some(changed || absorbed_changed)
    }

    /// Rebuild every glyph sort from the current Unicode inventory, then shape
    /// the complete buffer again. Hosts call this after a live font or feature
    /// edit so existing text cannot retain an obsolete substitution or width.
    /// The caret, selection, active sort, and manual kerning session survive.
    pub fn refresh_shaping(&mut self) -> bool {
        let mut updates = Vec::new();
        for (index, sort) in self.sorts.iter().enumerate() {
            let TextSortKind::Glyph {
                codepoint: Some(character),
                ..
            } = sort.kind
            else {
                continue;
            };
            let Some(name) = self.glyph_inventory.unicode.get(&(character as u32)) else {
                continue;
            };
            let advance = self
                .glyph_inventory
                .widths
                .get(name)
                .copied()
                .unwrap_or_else(|| self.sort_advance(index));
            updates.push((index, name.clone(), advance));
        }
        let mut changed = self.apply_shape_updates(updates);
        self.shaped.0.clear();
        for sort in &mut self.sorts {
            if sort.absorbed {
                sort.absorbed = false;
                changed = true;
            }
        }
        changed | self.shape_arabic_if_rtl()
    }

    /// Shape the whole buffer: through the font's `features.fea` when it compiles, otherwise with the built-in Arabic joining rules on RTL lines.
    /// Updates glyph names and advance widths in place. Returns true when any sort changed.
    pub fn shape_arabic(&mut self) -> bool {
        // The font's own GSUB first: it gives ligatures and contextual
        // rules the joining table below cannot express. Falls through
        // when there is no features.fea or it does not compile.
        if let Some(changed) = self.shape_with_font() {
            return changed;
        }
        // The joining table places nothing; marks sit on the pen.
        self.shaped.0.clear();
        let chars = self.glyph_chars();
        let mut updates = Vec::new();

        for index in 0..self.sorts.len() {
            let Some(char) = self.sort_codepoint(index) else {
                continue;
            };
            let char_index = self.char_index_for_sort_index(index);
            let name = self.shaped_glyph_name_for_character(char, &chars, char_index, index);
            let advance_width = self
                .glyph_inventory
                .widths
                .get(&name)
                .copied()
                .unwrap_or_else(|| self.sort_advance(index));
            updates.push((index, name, advance_width));
        }

        self.apply_shape_updates(updates)
    }

    /// Shape when any line in the buffer reads RTL: a Latin line next
    /// to an Arabic one must not stop the Arabic from joining.
    ///
    /// With a shaping font the direction gate does not apply: the font's
    /// rules cover every script it supports, not just Arabic.
    pub fn shape_arabic_if_rtl(&mut self) -> bool {
        if let Some(changed) = self.shape_with_font() {
            return changed;
        }
        let has_rtl_line = (0..self.line_count())
            .any(|line| self.resolved_line_direction(line) == TextDirection::RightToLeft);
        if !has_rtl_line {
            return false;
        }
        self.shape_arabic()
    }

    /// Shape after an edit at `position`.
    /// With a shaping font the whole buffer is reshaped, since a ligature can form several sorts away. Without one only the Arabic neighbors are rejoined, and only when the line reads RTL. Returns true when any sort changed.
    pub fn shape_arabic_around_if_rtl(&mut self, position: usize) -> bool {
        // Reshaping the whole buffer through the font is cheap at editor
        // sizes, and a ligature can appear or break several sorts away
        // from the one that changed.
        if let Some(changed) = self.shape_with_font() {
            return changed;
        }
        let line = self.line_number_for_sort(position);
        if self.resolved_line_direction(line) != TextDirection::RightToLeft {
            return false;
        }
        self.shape_arabic_around(position)
    }

    pub(super) fn shape_arabic_around(&mut self, position: usize) -> bool {
        if self.sorts.is_empty() {
            return false;
        }

        let indices = self.arabic_shape_indices_around(position);
        if indices.is_empty() {
            return false;
        }

        let chars = self.glyph_chars();
        let mut updates = Vec::new();

        for index in indices {
            let Some(char) = self.sort_codepoint(index) else {
                continue;
            };
            if !joining::is_arabic(char) {
                continue;
            }
            let char_index = self.char_index_for_sort_index(index);
            let name = self.shaped_glyph_name_for_character(char, &chars, char_index, index);
            let advance_width = self
                .glyph_inventory
                .widths
                .get(&name)
                .copied()
                .unwrap_or_else(|| self.sort_advance(index));
            updates.push((index, name, advance_width));
        }

        self.apply_shape_updates(updates)
    }

    pub(super) fn arabic_shape_indices_around(&self, position: usize) -> Vec<usize> {
        let mut indices = Vec::new();

        if let Some(index) = self.previous_nontransparent_arabic_sort(position) {
            indices.push(index);
        }

        if let Some(index) = self.next_nontransparent_arabic_sort(position) {
            indices.push(index);
            if let Some(next_index) = self.next_nontransparent_arabic_sort(index + 1) {
                indices.push(next_index);
            }
        }

        indices.dedup();
        indices
    }

    pub(super) fn previous_nontransparent_arabic_sort(&self, position: usize) -> Option<usize> {
        let end = position.min(self.sorts.len());
        (0..end)
            .rev()
            .find(|index| self.is_nontransparent_arabic_sort(*index))
    }

    pub(super) fn next_nontransparent_arabic_sort(&self, position: usize) -> Option<usize> {
        (position..self.sorts.len()).find(|index| self.is_nontransparent_arabic_sort(*index))
    }

    pub(super) fn is_nontransparent_arabic_sort(&self, index: usize) -> bool {
        self.sort_codepoint(index).is_some_and(|char| {
            joining::is_arabic(char) && !joining::arabic_joining_type(char).is_transparent()
        })
    }

    pub(super) fn glyph_chars(&self) -> Vec<char> {
        self.sorts
            .iter()
            .filter_map(|sort| match sort.kind {
                TextSortKind::Glyph {
                    codepoint: Some(char),
                    ..
                } => Some(char),
                _ => None,
            })
            .collect()
    }

    pub(super) fn char_index_for_sort_index(&self, sort_index: usize) -> usize {
        self.sorts[..sort_index]
            .iter()
            .filter(|sort| {
                matches!(
                    sort.kind,
                    TextSortKind::Glyph {
                        codepoint: Some(_),
                        ..
                    }
                )
            })
            .count()
    }

    pub(super) fn apply_shape_updates(&mut self, updates: Vec<(usize, String, f64)>) -> bool {
        let mut changed = false;
        for (index, name, advance_width) in updates {
            let Some(sort) = self.sorts.get_mut(index) else {
                continue;
            };
            let TextSortKind::Glyph {
                name: glyph_name,
                advance_width: glyph_advance_width,
                ..
            } = &mut sort.kind
            else {
                continue;
            };
            if *glyph_name != name || *glyph_advance_width != advance_width {
                *glyph_name = name;
                *glyph_advance_width = advance_width;
                changed = true;
            }
        }

        changed
    }

    pub(super) fn shaped_glyph_name_for_character(
        &self,
        char: char,
        line_chars: &[char],
        char_index: usize,
        sort_index: usize,
    ) -> String {
        let base_name = self
            .glyph_inventory
            .unicode
            .get(&(char as u32))
            .cloned()
            .or_else(|| self.sort_glyph_name(sort_index).map(ToOwned::to_owned))
            .unwrap_or_else(|| ".notdef".to_string());
        // Shape by the *line's* direction, not the buffer's: in Auto
        // mode an Arabic line joins even when the buffer default (or
        // another line) is left-to-right.
        let line = self.line_number_for_sort(sort_index);
        if self.resolved_line_direction(line) != TextDirection::RightToLeft
            || !joining::is_arabic(char)
        {
            return base_name;
        }

        let suffix = joining::arabic_positional_form(line_chars, char_index).suffix();
        let shaped_name = format!("{base_name}{suffix}");
        if !suffix.is_empty() && self.glyph_inventory.has_glyph(&shaped_name) {
            shaped_name
        } else {
            base_name
        }
    }
}
