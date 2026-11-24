// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Kerning between sorts: the model with group fallback, and manual kerning by dragging a sort.
//!
//! A kerning pair is two bases next to each other in logical order.
//! Marks between them are skipped, as the kerning writer of fontc skips them with its
//! `IgnoreMarks` lookups: in `رَا` the pair is reh and alef, not fatha and alef.
//! A pair inside a right-to-left run reads right to left on the screen whatever the
//! direction of the line; Arabic-Indic digits form left-to-right runs, so their pairs read
//! left to right.

use super::*;

impl TextBuffer {
    /// Direction of the bidi run containing the active glyph.
    pub fn active_run_is_rtl(&self) -> Option<bool> {
        let active = self.active_sort()?;
        let (start, end) = self.line_range_for_number(self.line_number_for_sort(active));
        self.visual_runs_for_line(start, end)
            .iter()
            .find(|r| r.visual_order().contains(&active))
            .map(|r| r.rtl)
    }

    /// Whether the sort draws a mark: a glyph of the mark class in the GDEF table of the
    /// shaping font, or, without one, a character of a mark category.
    pub fn is_mark_sort(&self, index: usize) -> bool {
        if let Some(name) = self.sorts.get(index).and_then(TextSort::glyph_name)
            && let Some(font) = self.shaping_font()
            && let Some(class) = font.glyph_class(name).filter(|class| *class != 0)
        {
            return class == 3;
        }
        self.sorts
            .get(index)
            .and_then(|sort| match sort.kind {
                TextSortKind::Glyph { codepoint, .. } => codepoint,
                TextSortKind::LineBreak => None,
            })
            .is_some_and(|character| {
                use unicode_general_category::{GeneralCategory, get_general_category};
                matches!(
                    get_general_category(character),
                    GeneralCategory::NonspacingMark
                        | GeneralCategory::SpacingMark
                        | GeneralCategory::EnclosingMark
                )
            })
    }

    /// Whether a sort takes part in a kerning pair: it draws a glyph and is not a mark.
    fn kerns(&self, index: usize) -> bool {
        !self.sorts[index].absorbed && !self.is_mark_sort(index)
    }

    /// Pair on a physical side of the active shaped sort, in font kerning order.
    /// Marks are skipped; an active mark has no pair.
    pub fn active_visual_kern(&self, left: bool) -> Option<(&str, &str, f64)> {
        let active = self.active_sort()?;
        if self.is_mark_sort(active) {
            return None;
        }
        let (start, end) = self.line_range_for_number(self.line_number_for_sort(active));
        let runs = self.visual_runs_for_line(start, end);
        for run in runs.iter() {
            let mut previous = None;
            for &index in run.visual_order() {
                if !self.kerns(index) {
                    continue;
                }
                if let Some(prev) = previous
                    && ((left && index == active) || (!left && prev == active))
                {
                    let (first, second) = if run.rtl {
                        (index, prev)
                    } else {
                        (prev, index)
                    };
                    let a = self.sort_glyph_name(first)?;
                    let b = self.sort_glyph_name(second)?;
                    return Some((a, b, self.lookup_kerning(a, b)));
                }
                previous = Some(index);
            }
        }
        None
    }

    /// Set a temporary displacement at one visual boundary, after shaping.
    pub fn preview_drag_gap(&mut self, gap: Option<(usize, f64)>) {
        self.drag_gap = gap;
    }

    /// Logical pair ending at the grabbed sort, plus the visual boundary to move.
    ///
    /// Returns the first and second glyph of the pair in logical order, the sort before which
    /// a preview without live layout tables moves the line, whether the pair's run reads right
    /// to left, and the current value.
    /// Marks are skipped on both sides: a grabbed mark kerns as its base, and the pair is that
    /// base and the base before it, with the marks of the earlier base moving with it.
    pub fn drag_pair(&self, grabbed: usize) -> Option<(&str, &str, usize, bool, f64)> {
        let (start, end) = self.line_range_for_number(self.line_number_for_sort(grabbed));
        let runs = self.visual_runs_for_line(start, end);
        let run = runs.iter().find(|r| r.sorts.contains(&grabbed))?;
        let at = run.sorts.iter().position(|&i| i == grabbed)?;
        // A mark kerns as the base it follows.
        let at = run.sorts[..=at].iter().rposition(|&i| self.kerns(i))?;
        let grabbed = run.sorts[at];
        let prev_at = run.sorts[..at].iter().rposition(|&i| self.kerns(i))?;
        let prev = run.sorts[prev_at];
        let a = self.sort_glyph_name(prev)?;
        let b = self.sort_glyph_name(grabbed)?;
        // On the screen the gap is between the grabbed base and the next drawn sort: in a
        // left-to-right run that is the grabbed base, after the earlier base and its marks;
        // in a right-to-left run it is the drawn sort logically just before the grabbed one,
        // a mark of the earlier base or that base.
        let boundary = if run.rtl {
            *run.sorts[prev_at..at]
                .iter()
                .rev()
                .find(|&&i| !self.sorts[i].absorbed)?
        } else {
            grabbed
        };
        Some((a, b, boundary, run.rtl, self.lookup_kerning(a, b)))
    }

    /// The members of a kerning group, or `None` for a name that is not a group.
    pub fn group_members(&self, group: &str) -> Option<&[String]> {
        self.kerning.groups.get(group).map(Vec::as_slice)
    }

    /// Match the card's most specific existing pair, then use available groups.
    pub fn kern_scope(&self, a: &str, b: &str) -> [bool; 2] {
        let [ga, gb] = self.pair_groups(a, b);
        [[false, false], [false, true], [true, false], [true, true]]
            .into_iter()
            .find(|s| {
                let x = if s[0] { ga } else { Some(a) };
                let y = if s[1] { gb } else { Some(b) };
                x.zip(y)
                    .is_some_and(|(x, y)| self.stored_kern(x, y).is_some())
            })
            .unwrap_or([ga.is_some(), gb.is_some()])
    }

    /// Index of the sort being dragged in a manual kerning session, or `None` when no drag is in progress.
    pub fn manual_kerning_sort(&self) -> Option<usize> {
        self.manual_kerning.map(|session| session.sort_index)
    }

    /// Replace the kerning model. Any manual kerning edits in the old model are lost.
    pub fn set_kerning_model(&mut self, kerning: TextKerningModel) {
        self.kerning = Arc::new(kerning);
    }

    /// Replace the kerning model with one shared between buffers, without copying it.
    pub fn set_shared_kerning_model(&mut self, kerning: Arc<TextKerningModel>) {
        self.kerning = kerning;
    }

    /// The current kerning model, including pairs written by manual kerning.
    pub fn kerning_model(&self) -> &TextKerningModel {
        &self.kerning
    }

    /// Groups for the logical first and second members of a shaped pair.
    pub fn pair_groups(&self, first: &str, second: &str) -> [Option<&str>; 2] {
        [
            self.kerning.right_groups.get(first).map(String::as_str),
            self.kerning.left_groups.get(second).map(String::as_str),
        ]
    }

    /// Stored value at exactly this scope, including an explicit zero.
    pub fn stored_kern(&self, first: &str, second: &str) -> Option<f64> {
        self.kerning.kerning.get(first)?.get(second).copied()
    }

    /// Start dragging the kerning of the pair that ends at `sort_index`, with the pointer at `start_x`.
    /// Records the pair's current value as the baseline and activates the sort. Returns false for index `0`, a line break, or an index out of range.
    pub fn begin_manual_kerning(&mut self, sort_index: usize, start_x: f64) -> bool {
        if sort_index == 0
            || !matches!(
                self.sorts.get(sort_index).map(|sort| &sort.kind),
                Some(TextSortKind::Glyph { .. })
            )
        {
            return false;
        }
        let original_value = self
            .glyph_pair_names(sort_index)
            .map(|(left, right)| self.lookup_kerning(&left, &right))
            .unwrap_or(0.0)
            .round();
        self.manual_kerning = Some(ManualKerningSession {
            sort_index,
            start_x,
            original_value,
            current_offset: 0.0,
        });
        self.activate_sort(sort_index);
        true
    }

    /// Update the dragged pair from the pointer's new x, rounding the offset to whole units.
    /// The offset is negated on an RTL line so a rightward drag still closes the gap. Writes the new value into the kerning model and returns it, or `None` when no session is open or the rounded offset did not change.
    pub fn drag_manual_kerning(&mut self, current_x: f64) -> Option<f64> {
        let session = self.manual_kerning?;
        let mut current_offset = (current_x - session.start_x).round();
        // In a right-to-left line the pair's visual gap sits on the
        // other side of the dragged sort — the logical-previous glyph
        // draws to the right — so a rightward drag closes the gap and
        // the offset flips sign.
        let line = self.line_number_for_sort(session.sort_index);
        if self.resolved_line_direction(line) == TextDirection::RightToLeft {
            current_offset = -current_offset;
        }
        if current_offset == session.current_offset {
            return None;
        }
        self.manual_kerning = Some(ManualKerningSession {
            current_offset,
            ..session
        });
        let (left, right) = self.glyph_pair_names(session.sort_index)?;
        let value = (session.original_value + current_offset).round();
        self.set_direct_kerning(&left, &right, value);
        Some(value)
    }

    /// Close the manual kerning session, keeping the value written so far. Returns false when no session was open.
    pub fn end_manual_kerning(&mut self) -> bool {
        self.manual_kerning.take().is_some()
    }

    pub(super) fn glyph_pair_names(&self, sort_index: usize) -> Option<(String, String)> {
        let left = self.sort_glyph_name(sort_index.checked_sub(1)?)?;
        let right = self.sort_glyph_name(sort_index)?;
        Some((left.to_string(), right.to_string()))
    }

    pub(super) fn lookup_kerning(&self, left: &str, right: &str) -> f64 {
        lookup_xilem_kerning(
            &self.kerning.kerning,
            &self.kerning.groups,
            left,
            self.kerning.right_groups.get(left).map(String::as_str),
            right,
            self.kerning.left_groups.get(right).map(String::as_str),
        )
    }

    pub(super) fn set_direct_kerning(&mut self, left: &str, right: &str, value: f64) {
        // This buffer's own pairs change, so it takes its own copy of a shared model.
        let kerning = &mut Arc::make_mut(&mut self.kerning).kerning;
        if value == 0.0 {
            if let Some(pairs) = kerning.get_mut(left) {
                pairs.remove(right);
                if pairs.is_empty() {
                    kerning.remove(left);
                }
            }
            return;
        }
        kerning
            .entry(left.to_string())
            .or_default()
            .insert(right.to_string(), value);
    }
}
