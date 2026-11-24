// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Placing sorts on lines, and mapping a point back to a sort or a cursor position.

use super::*;

impl TextBuffer {
    /// In auto mode a line reads right-to-left when its first strong
    /// character does; otherwise every line follows the pinned
    /// direction.
    pub fn resolved_line_direction(&self, line: usize) -> TextDirection {
        if !self.auto_direction {
            return self.direction;
        }
        let (start, end) = self.line_range_for_number(line);
        for index in start..end.min(self.sorts.len()) {
            if let TextSortKind::Glyph {
                codepoint: Some(char),
                ..
            } = &self.sorts[index].kind
                && let Some(direction) = strong_direction(*char)
            {
                return direction;
            }
        }
        self.direction
    }

    pub(super) fn line_number_for_sort(&self, sort_index: usize) -> usize {
        self.sorts[..sort_index.min(self.sorts.len())]
            .iter()
            .filter(|sort| matches!(sort.kind, TextSortKind::LineBreak))
            .count()
    }

    /// Each line as the half-open range of sort indices it holds, without its line break.
    ///
    /// There is always at least one line, and the last line ends at the end of the buffer.
    pub(super) fn line_ranges(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        let mut next = Some(0);
        std::iter::from_fn(move || {
            let start = next?;
            let end = self.next_line_end(start);
            next = (end < self.sorts.len()).then_some(end + 1);
            Some((start, end))
        })
    }

    /// Number of lines, which is one more than the number of line breaks.
    pub fn line_count(&self) -> usize {
        1 + self
            .sorts
            .iter()
            .filter(|sort| matches!(sort.kind, TextSortKind::LineBreak))
            .count()
    }

    // TODO(perf): every frame walks the whole buffer, and every sort is
    // drawn whether or not it is on screen. At a page of text this is the
    // bulk of a frame. Cache the layout against a buffer revision, and
    // cull sorts outside the viewport before handing them to the scene.
    /// Place every drawn sort and the caret in font units.
    /// Lines stack downward by `line_height`; LTR lines start at x `0` and RTL lines share one right edge at the widest line. Kerning and bidi run order are applied. Absorbed sorts and line breaks get no item.
    pub fn layout(&self, line_height: f64) -> TextLayout {
        let mut items = Vec::with_capacity(self.sorts.len());
        let mut cursor_x = 0.0;
        let mut cursor_y = 0.0;
        // RTL lines share one right edge so a paragraph stays aligned,
        // the way xilem lines them up; LTR lines start at the origin.
        let rtl_line_start_x = self.rtl_line_start_x();

        for (line_number, (line_start, line_end)) in self.line_ranges().enumerate() {
            let direction = self.resolved_line_direction(line_number);
            let y = -line_height * line_number as f64;

            // Everything is placed left to right, run by run. A line that
            // reads right to left is right-aligned rather than reversed
            // wholesale: the bidi runs carry the order, so Latin inside
            // an Arabic sentence still reads left to right.
            let runs = self.visual_runs_for_line(line_start, line_end);

            // Lay the line out from zero, then shift the finished line
            // into place: a right-to-left line is right-aligned on the
            // widest line. Measuring first would mean looking up every
            // kerning pair twice, which at a page of text is the most
            // expensive thing in the frame.
            let line_items_start = items.len();
            let mut x = 0.0;
            // Caret position within the line, filled in as the run
            // holding the cursor is placed.
            let mut caret_at: Option<f64> = if self.cursor == line_start {
                Some(0.0)
            } else {
                None
            };
            let mut caret_at_line_start = self.cursor == line_start;

            for run in runs.iter() {
                let mut previous: Option<&str> = None;
                let mut previous_edge = None;
                for &index in run.visual_order() {
                    // A character folded into a ligature has no glyph of
                    // its own: no item, no width, no kerning pair.
                    if self.sorts[index].absorbed {
                        if self.cursor == index + 1 {
                            caret_at = Some(x);
                            caret_at_line_start = false;
                        }
                        continue;
                    }
                    let glyph_name = self.sort_glyph_name(index);
                    // Kerning pairs are in logical order. Inside an RTL
                    // run the glyph placed just before this one is the
                    // logically *following* one, so the pair is reversed.
                    x += kern_between(self, previous, glyph_name, run.rtl);
                    // A mark is drawn where shaping put it, off the pen;
                    // the pen itself moves by the advance only.
                    if let Some((boundary, delta)) = self.drag_gap
                        && boundary == index
                    {
                        x += delta;
                    }
                    let sort_start = x;
                    // Each glyph of the sort's cluster, in the shaper's order.
                    for glyph in self.placed_glyphs(index) {
                        let nominal_width = self
                            .glyph_inventory
                            .widths
                            .get(glyph.name)
                            .copied()
                            .unwrap_or(glyph.advance);
                        // Marks can have GPOS offsets independent of pair spacing.
                        let spacing_before = if nominal_width > 0.0 && glyph.dy == 0.0 {
                            previous_edge.map_or(0.0, |edge: f64| x + glyph.dx - edge)
                        } else {
                            0.0
                        };
                        previous_edge = (nominal_width > 0.0 && glyph.dy == 0.0)
                            .then_some(x + glyph.dx + nominal_width);
                        items.push(TextLayoutItem {
                            index,
                            cluster_glyph: glyph.cluster_glyph,
                            nominal_width,
                            spacing_before,
                            x: x + glyph.dx,
                            y: y + glyph.dy,
                            advance_width: glyph.advance,
                        });
                        x += glyph.advance;
                    }
                    previous = glyph_name;

                    // The caret follows the text, not the screen: after a
                    // sort in an RTL run it sits at that sort's left edge.
                    if self.cursor == index + 1 {
                        caret_at = Some(if run.rtl { sort_start } else { x });
                        caret_at_line_start = false;
                    }
                }
            }

            let line_width = x;
            let line_left = match direction {
                TextDirection::LeftToRight => 0.0,
                TextDirection::RightToLeft => rtl_line_start_x - line_width,
            };
            if line_left != 0.0 {
                for item in &mut items[line_items_start..] {
                    item.x += line_left;
                }
            }
            // An empty caret slot, or one at the start of a line, sits at
            // the edge the line reads from.
            if caret_at_line_start && direction == TextDirection::RightToLeft {
                caret_at = Some(line_width);
            }
            let caret_at = caret_at.map(|caret| caret + line_left);

            let caret_at = match caret_at {
                Some(caret) => Some(caret),
                None if self.cursor >= line_start && self.cursor <= line_end => {
                    Some(match direction {
                        TextDirection::LeftToRight => line_left,
                        TextDirection::RightToLeft => line_left + line_width,
                    })
                }
                None => None,
            };
            if let Some(caret) = caret_at {
                cursor_x = caret;
                cursor_y = y;
            }

            // Skip the line-break sort.
            if line_end < self.sorts.len() && self.cursor == line_end + 1 {
                cursor_x = match self.resolved_line_direction(line_number + 1) {
                    TextDirection::LeftToRight => 0.0,
                    TextDirection::RightToLeft => rtl_line_start_x,
                };
                cursor_y = -line_height * (line_number + 1) as f64;
            }
        }

        TextLayout {
            items,
            cursor_x,
            cursor_y,
        }
    }

    /// Place every drawn sort on one visual line at y `0`, for the preview strip.
    /// Consecutive lines that read the same way run on as one paragraph. Kerning is applied within a run but never across a line break.
    pub fn preview_layout(&self) -> Vec<TextLayoutItem> {
        // The strip is one visual line. Lines that read the same way run
        // on into each other as a single paragraph — an Arabic paragraph
        // keeps reading right to left across its line breaks — while the
        // bidi runs inside still order themselves, so a Latin word in an
        // Arabic sentence reads left to right here too.
        let mut items = Vec::with_capacity(self.sorts.len());
        let mut x = 0.0;

        for (start, end, rtl) in self.preview_groups() {
            for run in self.visual_runs_in(start, end, rtl).iter() {
                let mut previous: Option<(usize, &str)> = None;
                for &index in run.visual_order() {
                    if self.sorts[index].absorbed {
                        continue;
                    }
                    let glyph_name = self.sort_glyph_name(index);
                    // Kerning stops at a run edge and at a line break:
                    // two lines are not a kerning context, however they
                    // are drawn.
                    let kern = match (previous, glyph_name) {
                        (Some((previous_index, previous_name)), Some(name))
                            if !self.line_break_between(previous_index, index) =>
                        {
                            if run.rtl {
                                self.lookup_kerning(name, previous_name)
                            } else {
                                self.lookup_kerning(previous_name, name)
                            }
                        }
                        _ => 0.0,
                    };
                    x += kern;
                    // Every glyph of the cluster at its pen position; the strip, like
                    // before, has no shaping offsets.
                    for (position, glyph) in self.placed_glyphs(index).enumerate() {
                        items.push(TextLayoutItem {
                            index,
                            cluster_glyph: glyph.cluster_glyph,
                            x,
                            y: 0.0,
                            nominal_width: glyph.advance,
                            spacing_before: if position == 0 { kern } else { 0.0 },
                            advance_width: glyph.advance,
                        });
                        x += glyph.advance;
                    }
                    previous = glyph_name.map(|name| (index, name));
                }
            }
        }

        items
    }

    /// Stretches of consecutive lines that read the same way, as
    /// (start, end, rtl). The preview strip lays out one at a time.
    pub(super) fn preview_groups(&self) -> Vec<(usize, usize, bool)> {
        let mut groups: Vec<(usize, usize, bool)> = Vec::new();
        for (line_number, (line_start, line_end)) in self.line_ranges().enumerate() {
            let rtl = self.resolved_line_direction(line_number) == TextDirection::RightToLeft;
            match groups.last_mut() {
                Some(group) if group.2 == rtl => group.1 = line_end,
                _ => groups.push((line_start, line_end, rtl)),
            }
        }
        groups
    }

    /// Report whether a line break separates sorts `a` and `b`.
    pub(super) fn line_break_between(&self, a: usize, b: usize) -> bool {
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        self.sorts[low.min(self.sorts.len())..high.min(self.sorts.len())]
            .iter()
            .any(|sort| matches!(sort.kind, TextSortKind::LineBreak))
    }

    /// Find what a point in font units lands on.
    /// Returns the sort whose box, from `descender` to `ascender` above its baseline, contains the point, with the cursor placed after it. Otherwise returns the nearest cursor boundary on the line under `y` and no sort.
    pub fn hit_test(
        &self,
        x: f64,
        y: f64,
        line_height: f64,
        ascender: f64,
        descender: f64,
    ) -> TextHit {
        let layout = self.layout(line_height);
        self.hit_test_with_layout(x, y, line_height, ascender, descender, &layout)
    }

    pub(super) fn hit_test_with_layout(
        &self,
        x: f64,
        y: f64,
        line_height: f64,
        ascender: f64,
        descender: f64,
        layout: &TextLayout,
    ) -> TextHit {
        if self.sorts.is_empty() {
            return TextHit {
                cursor: 0,
                active_sort: None,
                cluster_glyph: None,
            };
        }

        if let Some(item) = self.hit_sort_item_at(x, y, line_height, ascender, descender, layout) {
            return TextHit {
                cursor: item.index + 1,
                active_sort: Some(item.index),
                cluster_glyph: item.cluster_glyph,
            };
        }
        let target_line = self.line_number_for_y(y, line_height.max(1.0), ascender, descender);
        let (line_start, line_end) = self.line_range_for_number(target_line);
        TextHit {
            cursor: self.nearest_cursor_for_line(x, line_start, line_end, layout),
            active_sort: None,
            cluster_glyph: None,
        }
    }

    pub(super) fn next_line_end(&self, start: usize) -> usize {
        self.sorts[start..]
            .iter()
            .position(|sort| matches!(sort.kind, TextSortKind::LineBreak))
            .map(|offset| start + offset)
            .unwrap_or(self.sorts.len())
    }

    /// The sort range of one line, or of the last line when `line_number` is past the end.
    pub(super) fn line_range_for_number(&self, line_number: usize) -> (usize, usize) {
        self.line_ranges()
            .take(line_number.saturating_add(1))
            .last()
            .unwrap_or((self.sorts.len(), self.sorts.len()))
    }

    pub(super) fn line_number_for_y(
        &self,
        y: f64,
        line_height: f64,
        ascender: f64,
        descender: f64,
    ) -> usize {
        let mut nearest_line = 0;
        let mut nearest_distance = f64::INFINITY;
        for line_number in 0..self.line_count() {
            let baseline = -line_height * line_number as f64;
            let top = baseline + ascender;
            let bottom = baseline + descender;
            if y >= bottom && y <= top {
                return line_number;
            }
            let distance = if y > top { y - top } else { bottom - y };
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest_line = line_number;
            }
        }
        nearest_line
    }

    pub(super) fn hit_sort_item_at(
        &self,
        x: f64,
        y: f64,
        line_height: f64,
        ascender: f64,
        descender: f64,
        layout: &TextLayout,
    ) -> Option<TextLayoutItem> {
        if self.sorts.is_empty() {
            return None;
        }

        let line_height = line_height.max(1.0);
        let target_line = self.line_number_for_y(y, line_height, ascender, descender);
        let (line_start, line_end) = self.line_range_for_number(target_line);
        for item in layout
            .items
            .iter()
            .filter(|item| (line_start..line_end).contains(&item.index))
        {
            // Min edges inclusive, max edges exclusive, as `kurbo::Rect::contains` tests them.
            let within_x = x >= item.x && x < item.x + item.advance_width;
            let within_y = y >= item.y + descender && y < item.y + ascender;
            if within_x && within_y {
                return Some(*item);
            }
        }
        None
    }

    pub(super) fn nearest_cursor_for_line(
        &self,
        x: f64,
        line_start: usize,
        line_end: usize,
        layout: &TextLayout,
    ) -> usize {
        let mut nearest_cursor = line_start;
        let mut nearest_distance = f64::INFINITY;
        // The direction of this line, which in automatic mode is its own.
        let direction = self.resolved_line_direction(self.line_number_for_sort(line_start));

        for candidate in line_start..=line_end {
            let cursor_x = if candidate == line_start {
                match direction {
                    TextDirection::LeftToRight => 0.0,
                    TextDirection::RightToLeft => self.rtl_line_start_x(),
                }
            } else {
                Self::sort_edges(layout, candidate - 1)
                    .map(|(left, right)| match direction {
                        TextDirection::LeftToRight => right,
                        TextDirection::RightToLeft => left,
                    })
                    .unwrap_or(0.0)
            };
            let distance = (x - cursor_x).abs();
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest_cursor = candidate;
            }
        }

        nearest_cursor
    }

    /// Where an RTL line begins: the widest line in the buffer, so
    /// every RTL line ends up sharing the same right edge. Xilem
    /// summed the whole buffer instead, which is too wide once lines
    /// stack.
    pub(super) fn rtl_line_start_x(&self) -> f64 {
        self.line_ranges()
            .map(|(start, end)| {
                (start..end)
                    .filter(|&i| !self.sorts[i].absorbed)
                    .map(|i| {
                        // Anchor RTL lines using nominal widths. Shaped advances include
                        // kerning, which would move the right edge after a font rebuild.
                        self.placed_glyphs(i)
                            .map(|glyph| {
                                self.glyph_inventory
                                    .widths
                                    .get(glyph.name)
                                    .copied()
                                    .unwrap_or(glyph.advance)
                            })
                            .sum::<f64>()
                    })
                    .sum::<f64>()
            })
            .fold(0.0, f64::max)
    }

    /// The left and right edges of every item of one sort: the span of its cluster.
    pub(super) fn sort_edges(layout: &TextLayout, index: usize) -> Option<(f64, f64)> {
        layout
            .items
            .iter()
            .filter(|item| item.index == index)
            .map(|item| (item.x, item.x + item.advance_width))
            .reduce(|(left, right), (x0, x1)| (left.min(x0), right.max(x1)))
    }

    /// The glyphs one sort draws, in the order of the shaper's output: its own glyph alone at
    /// its shaped offset, or every glyph of its cluster.
    pub(super) fn placed_glyphs(&self, index: usize) -> impl Iterator<Item = PlacedGlyph<'_>> {
        let cluster = self
            .shaped
            .0
            .get(&index)
            .filter(|cluster| cluster.glyphs.len() > 1);
        let own = cluster.is_none().then(|| {
            let (dx, dy) = self.shaped_offset(index);
            PlacedGlyph {
                name: self.sort_glyph_name(index).unwrap_or_default(),
                advance: self.sort_advance(index),
                dx,
                dy,
                cluster_glyph: None,
            }
        });
        let others = cluster.into_iter().flat_map(|cluster| {
            cluster
                .glyphs
                .iter()
                .enumerate()
                .map(move |(position, glyph)| PlacedGlyph {
                    name: &glyph.name,
                    advance: glyph.advance,
                    dx: glyph.x_offset,
                    dy: glyph.y_offset,
                    cluster_glyph: (position != cluster.own).then_some(position),
                })
        });
        own.into_iter().chain(others)
    }

    pub(super) fn sort_advance(&self, index: usize) -> f64 {
        match &self.sorts[index].kind {
            TextSortKind::Glyph { advance_width, .. } => *advance_width,
            TextSortKind::LineBreak => 0.0,
        }
    }

    pub(super) fn sort_glyph_name(&self, index: usize) -> Option<&str> {
        match &self.sorts[index].kind {
            TextSortKind::Glyph { name, .. } => Some(name),
            TextSortKind::LineBreak => None,
        }
    }

    pub(super) fn sort_codepoint(&self, index: usize) -> Option<char> {
        match &self.sorts[index].kind {
            TextSortKind::Glyph { codepoint, .. } => *codepoint,
            TextSortKind::LineBreak => None,
        }
    }
}

/// One glyph of a sort as layout places it.
pub(super) struct PlacedGlyph<'a> {
    /// The glyph name.
    pub(super) name: &'a str,
    /// The advance after positioning.
    pub(super) advance: f64,
    /// The offset from the pen position.
    pub(super) dx: f64,
    pub(super) dy: f64,
    /// `None` for the sort's own glyph; see [`TextLayoutItem::cluster_glyph`].
    pub(super) cluster_glyph: Option<usize>,
}
