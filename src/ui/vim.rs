// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A small modal text buffer with the core of Vim's normal, insert and visual commands.
//!
//! The buffer is toolkit-independent: a host translates its key events to [`VimKey`], calls
//! [`VimBuffer::key`], and draws the lines, cursor and selection it reads back.
//!
//! Supported in normal mode: counts; the motions `h j k l w b e 0 ^ $ gg G`; the operators
//! `d c y > <` with a motion, doubled for whole lines, or with `iw`; and
//! `x X D C s S Y J r i a I A o O p P u v V`.
//! Visual and visual-line modes take the same motions and `d x y c > <`.

/// The editing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Keys are commands.
    Normal,
    /// Keys are typed into the text.
    Insert,
    /// A character-wise selection from the anchor to the cursor.
    Visual,
    /// A selection of whole lines from the anchor to the cursor.
    VisualLine,
}

/// A key press, as the buffer reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimKey {
    /// A printable character, with Shift already applied.
    Char(char),
    /// Escape.
    Escape,
    /// Return.
    Enter,
    /// Backspace.
    Backspace,
    /// Forward delete.
    Delete,
    /// Tab.
    Tab,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Home.
    Home,
    /// End.
    End,
    /// Control-R: redo.
    Redo,
    /// Control-D: move down half a page.
    HalfPageDown,
    /// Control-U: move up half a page.
    HalfPageUp,
}

/// A selection in reading order, for drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// First selected position, as `(row, column)`.
    pub start: (usize, usize),
    /// Last selected position, inclusive, as `(row, column)`.
    pub end: (usize, usize),
    /// Whether whole lines are selected.
    pub linewise: bool,
}

const INDENT: usize = 4;
const MAX_UNDO_STEPS: usize = 200;

type Line = Vec<char>;
type Pos = (usize, usize);

#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    lines: Vec<Line>,
    cursor: Pos,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Span {
    /// Up to, but not including, the target.
    Exclusive,
    /// Through the target.
    Inclusive,
    /// Whole lines.
    Linewise,
}

#[derive(Debug, Default)]
struct Pending {
    count: Option<usize>,
    /// The count typed before the operator, which multiplies the count typed after it.
    operator_count: Option<usize>,
    operator: Option<char>,
    /// `g`, `r`, or the `i` of a text object.
    prefix: Option<char>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Space,
    Word,
    Punctuation,
}

fn class(c: char) -> Class {
    if c.is_whitespace() {
        Class::Space
    } else if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else {
        Class::Punctuation
    }
}

/// The modal buffer.
#[derive(Debug)]
pub struct VimBuffer {
    lines: Vec<Line>,
    final_newline: bool,
    cursor: Pos,
    /// The column vertical motions return to after passing a shorter line.
    wanted_column: usize,
    mode: Mode,
    anchor: Pos,
    pending: Pending,
    register: Vec<Line>,
    register_linewise: bool,
    yanked: Option<String>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    page: usize,
}

impl VimBuffer {
    /// Create a buffer in normal mode with the cursor at the start.
    pub fn new(text: &str) -> Self {
        let (lines, final_newline) = split_lines(text);
        Self {
            lines,
            final_newline,
            cursor: (0, 0),
            wanted_column: 0,
            mode: Mode::Normal,
            anchor: (0, 0),
            pending: Pending::default(),
            register: Vec::new(),
            register_linewise: false,
            yanked: None,
            undo: Vec::new(),
            redo: Vec::new(),
            page: 20,
        }
    }

    /// The whole text.
    pub fn text(&self) -> String {
        let mut text = self
            .lines
            .iter()
            .map(|line| line.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        if self.final_newline {
            text.push('\n');
        }
        text
    }

    /// Replace the text from outside the editor.
    /// The mode returns to normal, the history is dropped, and the cursor stays as near as it can.
    pub fn set_text(&mut self, text: &str) {
        let (lines, final_newline) = split_lines(text);
        self.lines = lines;
        self.final_newline = final_newline;
        self.mode = Mode::Normal;
        self.pending = Pending::default();
        self.undo.clear();
        self.redo.clear();
        self.clamp_cursor();
    }

    /// Number of lines; at least one.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// One line's characters.
    pub fn line(&self, row: usize) -> &[char] {
        self.lines.get(row).map_or(&[], Vec::as_slice)
    }

    /// The cursor as `(row, column)`.
    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    /// The current mode.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The visual selection, if one is active.
    pub fn selection(&self) -> Option<Selection> {
        let linewise = match self.mode {
            Mode::Visual => false,
            Mode::VisualLine => true,
            Mode::Normal | Mode::Insert => return None,
        };
        let (start, end) = ordered(self.anchor, self.cursor);
        Some(Selection {
            start,
            end,
            linewise,
        })
    }

    /// Set how many lines a half-page motion moves; the host passes half its visible lines.
    pub fn set_half_page(&mut self, lines: usize) {
        self.page = lines.max(1);
    }

    /// Move the cursor to the nearest valid position, as for a pointer click.
    pub fn set_cursor(&mut self, row: usize, column: usize) {
        self.cursor = (row, column);
        self.pending = Pending::default();
        self.clamp_cursor();
        self.wanted_column = self.cursor.1;
    }

    /// Text copied by the last yank or delete, once, for the system clipboard.
    pub fn take_yanked(&mut self) -> Option<String> {
        self.yanked.take()
    }

    /// Insert text from outside, such as a paste, at the cursor. Returns whether the text changed.
    pub fn insert_str(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        if self.mode != Mode::Insert {
            self.checkpoint();
        }
        if matches!(self.mode, Mode::Visual | Mode::VisualLine) {
            self.delete_selection();
            self.mode = Mode::Normal;
        }
        let (lines, _) = split_lines(text);
        let at = self.cursor;
        self.cursor = self.insert_lines(at, &lines);
        if self.mode != Mode::Insert {
            self.cursor.1 = self.cursor.1.saturating_sub(1);
            self.clamp_cursor();
        }
        self.wanted_column = self.cursor.1;
        true
    }

    /// Undo one step. Returns whether the text changed.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(previous);
        true
    }

    /// Redo one step. Returns whether the text changed.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(next);
        true
    }

    /// Handle one key. Returns whether the text changed.
    pub fn key(&mut self, key: VimKey) -> bool {
        let before = self.lines.clone();
        match self.mode {
            Mode::Insert => self.insert_key(key),
            Mode::Normal => self.normal_key(key),
            Mode::Visual | Mode::VisualLine => self.visual_key(key),
        }
        if self.lines.is_empty() {
            self.lines.push(Vec::new());
        }
        self.clamp_cursor();
        self.lines != before
    }

    // ---- state helpers ----

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            cursor: self.cursor,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.lines = snapshot.lines;
        self.cursor = snapshot.cursor;
        self.mode = Mode::Normal;
        self.pending = Pending::default();
        self.clamp_cursor();
        self.wanted_column = self.cursor.1;
    }

    /// Record the state before a change.
    fn checkpoint(&mut self) {
        let snapshot = self.snapshot();
        if self.undo.last() != Some(&snapshot) {
            self.undo.push(snapshot);
            if self.undo.len() > MAX_UNDO_STEPS {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
    }

    fn len(&self, row: usize) -> usize {
        self.lines.get(row).map_or(0, Vec::len)
    }

    fn last_row(&self) -> usize {
        self.lines.len().saturating_sub(1)
    }

    fn clamp_cursor(&mut self) {
        self.cursor.0 = self.cursor.0.min(self.last_row());
        let len = self.len(self.cursor.0);
        let limit = if self.mode == Mode::Insert {
            len
        } else {
            len.saturating_sub(1)
        };
        self.cursor.1 = self.cursor.1.min(limit);
    }

    fn first_non_blank(&self, row: usize) -> usize {
        self.line(row)
            .iter()
            .position(|c| !c.is_whitespace())
            .unwrap_or(0)
    }

    fn indent_of(&self, row: usize) -> Line {
        self.line(row)
            .iter()
            .copied()
            .take_while(|c| c.is_whitespace())
            .collect()
    }

    fn class_at(&self, (row, column): Pos) -> Class {
        self.line(row)
            .get(column)
            .map_or(Class::Space, |c| class(*c))
    }

    /// The next position in reading order; the end of a line counts as one position.
    fn next(&self, (row, column): Pos) -> Option<Pos> {
        if column < self.len(row) {
            Some((row, column + 1))
        } else if row < self.last_row() {
            Some((row + 1, 0))
        } else {
            None
        }
    }

    fn previous(&self, (row, column): Pos) -> Option<Pos> {
        if column > 0 {
            Some((row, column - 1))
        } else if row > 0 {
            Some((row - 1, self.len(row - 1)))
        } else {
            None
        }
    }

    fn word_forward(&self, mut at: Pos) -> Pos {
        let start = self.class_at(at);
        if start != Class::Space {
            while self.class_at(at) == start && at.1 < self.len(at.0) {
                at.1 += 1;
            }
        }
        while self.class_at(at) == Class::Space {
            match self.next(at) {
                // An empty line is a word of its own.
                Some(next) if next.1 == 0 && self.len(next.0) == 0 => return next,
                Some(next) => at = next,
                None => return at,
            }
        }
        at
    }

    fn word_end(&self, at: Pos) -> Pos {
        let Some(mut at) = self.next(at) else {
            return at;
        };
        while self.class_at(at) == Class::Space {
            match self.next(at) {
                Some(next) => at = next,
                None => return at,
            }
        }
        let kind = self.class_at(at);
        while at.1 + 1 < self.len(at.0) && self.class_at((at.0, at.1 + 1)) == kind {
            at.1 += 1;
        }
        at
    }

    fn word_back(&self, at: Pos) -> Pos {
        let Some(mut at) = self.previous(at) else {
            return at;
        };
        while self.class_at(at) == Class::Space {
            match self.previous(at) {
                Some(previous) => at = previous,
                None => return at,
            }
        }
        let kind = self.class_at(at);
        while at.1 > 0 && self.class_at((at.0, at.1 - 1)) == kind {
            at.1 -= 1;
        }
        at
    }

    /// Where a motion key leads from the cursor, and how an operator spans it.
    fn motion(&mut self, key: char, count: Option<usize>) -> Option<(Pos, Span)> {
        let n = count.unwrap_or(1);
        let (row, column) = self.cursor;
        let vertical = |this: &Self, target: usize| {
            let target = target.min(this.last_row());
            (
                (target, this.wanted_column.min(this.len(target))),
                Span::Linewise,
            )
        };
        Some(match key {
            'h' => ((row, column.saturating_sub(n)), Span::Exclusive),
            'l' => ((row, (column + n).min(self.len(row))), Span::Exclusive),
            'j' => vertical(self, row + n),
            'k' => vertical(self, row.saturating_sub(n)),
            '0' => ((row, 0), Span::Exclusive),
            '^' => ((row, self.first_non_blank(row)), Span::Exclusive),
            '$' => {
                let target = (row + n - 1).min(self.last_row());
                (
                    (target, self.len(target).saturating_sub(1)),
                    Span::Inclusive,
                )
            }
            'w' => {
                let mut at = self.cursor;
                for _ in 0..n {
                    at = self.word_forward(at);
                }
                (at, Span::Exclusive)
            }
            'e' => {
                let mut at = self.cursor;
                for _ in 0..n {
                    at = self.word_end(at);
                }
                (at, Span::Inclusive)
            }
            'b' => {
                let mut at = self.cursor;
                for _ in 0..n {
                    at = self.word_back(at);
                }
                (at, Span::Exclusive)
            }
            'G' => {
                let target = count.map_or(self.last_row(), |line| line.saturating_sub(1));
                let target = target.min(self.last_row());
                ((target, self.first_non_blank(target)), Span::Linewise)
            }
            _ => return None,
        })
    }

    fn move_to(&mut self, target: Pos, vertical: bool) {
        self.cursor = target;
        self.clamp_cursor();
        if !vertical {
            self.wanted_column = self.cursor.1;
        }
    }

    // ---- text edits ----

    /// Remove `start..end` in reading order and return it.
    fn remove(&mut self, start: Pos, end: Pos) -> Vec<Line> {
        if start.0 == end.0 {
            let line = &mut self.lines[start.0];
            let to = end.1.min(line.len());
            let from = start.1.min(to);
            return vec![line.drain(from..to).collect()];
        }
        let tail: Line = {
            let last = &self.lines[end.0];
            last[end.1.min(last.len())..].to_vec()
        };
        let mut removed = Vec::new();
        let first = &mut self.lines[start.0];
        let from = start.1.min(first.len());
        removed.push(first.drain(from..).collect());
        first.extend(tail);
        for (offset, line) in self.lines.drain(start.0 + 1..=end.0).enumerate() {
            let last = start.0 + 1 + offset == end.0;
            removed.push(if last {
                line[..end.1.min(line.len())].to_vec()
            } else {
                line
            });
        }
        removed
    }

    /// Insert `text` at `at` and return the position after it.
    fn insert_lines(&mut self, at: Pos, text: &[Line]) -> Pos {
        let Some((first, rest)) = text.split_first() else {
            return at;
        };
        let line = &mut self.lines[at.0];
        let column = at.1.min(line.len());
        let tail: Line = line.drain(column..).collect();
        line.extend(first);
        let Some((last, middle)) = rest.split_last() else {
            let end = (at.0, line.len());
            line.extend(tail);
            return end;
        };
        let mut row = at.0 + 1;
        for inserted in middle {
            self.lines.insert(row, inserted.clone());
            row += 1;
        }
        let mut closing = last.clone();
        let end = (row, closing.len());
        closing.extend(tail);
        self.lines.insert(row, closing);
        end
    }

    fn store(&mut self, text: Vec<Line>, linewise: bool) {
        let mut copied = text
            .iter()
            .map(|line| line.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        if linewise {
            copied.push('\n');
        }
        self.yanked = Some(copied);
        self.register = text;
        self.register_linewise = linewise;
    }

    fn remove_rows(&mut self, first: usize, last: usize) -> Vec<Line> {
        let removed: Vec<Line> = self
            .lines
            .drain(first..=last.min(self.last_row()))
            .collect();
        if self.lines.is_empty() {
            self.lines.push(Vec::new());
        }
        removed
    }

    fn shift_rows(&mut self, first: usize, last: usize, right: bool) {
        for row in first..=last.min(self.last_row()) {
            let line = &mut self.lines[row];
            if right {
                if !line.is_empty() {
                    line.splice(0..0, std::iter::repeat_n(' ', INDENT));
                }
            } else {
                let spaces = line.iter().take(INDENT).take_while(|c| **c == ' ').count();
                line.drain(..spaces);
            }
        }
    }

    /// Apply an operator between two positions.
    fn operate(&mut self, operator: char, from: Pos, to: Pos, span: Span) {
        let (start, end) = ordered(from, to);
        if operator != 'y' {
            self.checkpoint();
        }
        if span == Span::Linewise {
            match operator {
                'y' => {
                    let copied = self.lines[start.0..=end.0].to_vec();
                    self.store(copied, true);
                    self.cursor = (start.0, self.cursor.1);
                }
                'd' => {
                    let removed = self.remove_rows(start.0, end.0);
                    self.store(removed, true);
                    let row = start.0.min(self.last_row());
                    self.cursor = (row, self.first_non_blank(row));
                }
                'c' => {
                    let indent = self.indent_of(start.0);
                    let everything = end.0 - start.0 + 1 >= self.lines.len();
                    let removed = self.remove_rows(start.0, end.0);
                    self.store(removed, true);
                    // `remove_rows` leaves one empty line when it removed them all.
                    if everything {
                        self.lines[0] = indent.clone();
                        self.cursor = (0, indent.len());
                    } else {
                        let row = start.0.min(self.lines.len());
                        self.lines.insert(row, indent.clone());
                        self.cursor = (row, indent.len());
                    }
                    self.mode = Mode::Insert;
                }
                '>' | '<' => {
                    self.shift_rows(start.0, end.0, operator == '>');
                    self.cursor = (start.0, self.first_non_blank(start.0));
                }
                _ => {}
            }
        } else {
            let end = if span == Span::Inclusive {
                (end.0, end.1 + 1)
            } else {
                end
            };
            match operator {
                'y' => {
                    let snapshot = self.lines.clone();
                    let copied = self.remove(start, end);
                    self.lines = snapshot;
                    self.store(copied, false);
                    self.cursor = start;
                }
                'd' | 'c' => {
                    let removed = self.remove(start, end);
                    self.store(removed, false);
                    self.cursor = start;
                    if operator == 'c' {
                        self.mode = Mode::Insert;
                    }
                }
                '>' | '<' => {
                    self.shift_rows(start.0, end.0, operator == '>');
                    self.cursor = (start.0, self.first_non_blank(start.0));
                }
                _ => {}
            }
        }
        self.clamp_cursor();
        self.wanted_column = self.cursor.1;
    }

    fn paste(&mut self, after: bool, count: usize) {
        if self.register.is_empty() {
            return;
        }
        self.checkpoint();
        let register = self.register.clone();
        if self.register_linewise {
            let mut row = if after {
                self.cursor.0 + 1
            } else {
                self.cursor.0
            };
            let first = row;
            for _ in 0..count {
                for line in &register {
                    self.lines.insert(row, line.clone());
                    row += 1;
                }
            }
            self.cursor = (first, self.first_non_blank(first));
        } else {
            let mut at = self.cursor;
            if after && self.len(at.0) > 0 {
                at.1 += 1;
            }
            for _ in 0..count {
                at = self.insert_lines(at, &register);
            }
            self.cursor = (at.0, at.1.saturating_sub(1));
        }
        self.wanted_column = self.cursor.1;
    }

    fn delete_selection(&mut self) {
        if let Some(selection) = self.selection() {
            let span = if selection.linewise {
                Span::Linewise
            } else {
                Span::Inclusive
            };
            self.mode = Mode::Normal;
            self.operate('d', selection.start, selection.end, span);
        }
    }

    // ---- modes ----

    fn enter_insert(&mut self) {
        self.checkpoint();
        self.mode = Mode::Insert;
    }

    fn insert_key(&mut self, key: VimKey) {
        let (row, column) = self.cursor;
        match key {
            VimKey::Escape => {
                self.mode = Mode::Normal;
                self.cursor.1 = column.saturating_sub(1);
                // An insert that changed nothing leaves no undo step.
                if self
                    .undo
                    .last()
                    .is_some_and(|last| last.lines == self.lines)
                {
                    self.undo.pop();
                }
            }
            VimKey::Char(c) => {
                self.lines[row].insert(column, c);
                self.cursor.1 += 1;
            }
            VimKey::Tab => {
                let spaces = INDENT - column % INDENT;
                self.lines[row].splice(column..column, std::iter::repeat_n(' ', spaces));
                self.cursor.1 += spaces;
            }
            VimKey::Enter => {
                let mut indent = self.indent_of(row);
                indent.truncate(column);
                let tail: Line = self.lines[row].drain(column..).collect();
                let width = indent.len();
                indent.extend(tail);
                self.lines.insert(row + 1, indent);
                self.cursor = (row + 1, width);
            }
            VimKey::Backspace => {
                if column > 0 {
                    let line = &self.lines[row];
                    let only_spaces = line[..column].iter().all(|c| *c == ' ');
                    // In the indent, Backspace returns to the previous tab stop.
                    let width = if only_spaces {
                        (column - 1) % INDENT + 1
                    } else {
                        1
                    };
                    self.lines[row].drain(column - width..column);
                    self.cursor.1 -= width;
                } else if row > 0 {
                    let line = self.lines.remove(row);
                    let width = self.len(row - 1);
                    self.lines[row - 1].extend(line);
                    self.cursor = (row - 1, width);
                }
            }
            VimKey::Delete => {
                if column < self.len(row) {
                    self.lines[row].remove(column);
                } else if row < self.last_row() {
                    let line = self.lines.remove(row + 1);
                    self.lines[row].extend(line);
                }
            }
            VimKey::Left => self.cursor.1 = column.saturating_sub(1),
            VimKey::Right => self.cursor.1 = column + 1,
            VimKey::Up => self.cursor.0 = row.saturating_sub(1),
            VimKey::Down => self.cursor.0 = row + 1,
            VimKey::Home => self.cursor.1 = 0,
            VimKey::End => self.cursor.1 = self.len(row),
            VimKey::Redo | VimKey::HalfPageDown | VimKey::HalfPageUp => {}
        }
        self.wanted_column = self.cursor.1;
    }

    /// Translate keys shared by normal and visual mode to their letter commands.
    fn as_command(key: VimKey) -> Option<char> {
        Some(match key {
            VimKey::Char(c) => c,
            VimKey::Left | VimKey::Backspace => 'h',
            VimKey::Right => 'l',
            VimKey::Up => 'k',
            VimKey::Down | VimKey::Enter => 'j',
            VimKey::Home => '0',
            VimKey::End => '$',
            _ => return None,
        })
    }

    fn half_page(&mut self, down: bool) {
        let row = if down {
            (self.cursor.0 + self.page).min(self.last_row())
        } else {
            self.cursor.0.saturating_sub(self.page)
        };
        self.cursor = (row, self.wanted_column.min(self.len(row)));
    }

    fn normal_key(&mut self, key: VimKey) {
        match key {
            VimKey::Escape => {
                self.pending = Pending::default();
                return;
            }
            VimKey::Redo => {
                self.pending = Pending::default();
                self.redo();
                return;
            }
            VimKey::HalfPageDown | VimKey::HalfPageUp => {
                self.pending = Pending::default();
                self.half_page(key == VimKey::HalfPageDown);
                return;
            }
            VimKey::Delete => {
                self.pending = Pending::default();
                self.normal_command('x');
                return;
            }
            _ => {}
        }
        // `r` replaces with a typed character only, and any other key cancels it.
        if self.pending.prefix == Some('r') {
            match key {
                VimKey::Char(c) => self.normal_command(c),
                _ => self.pending = Pending::default(),
            }
            return;
        }
        if let Some(c) = Self::as_command(key) {
            self.normal_command(c);
        }
    }

    fn normal_command(&mut self, c: char) {
        // `r` takes the next character literally.
        if self.pending.prefix == Some('r') {
            let (row, column) = self.cursor;
            let count = self.pending.count.unwrap_or(1);
            self.pending = Pending::default();
            if column + count <= self.len(row) {
                self.checkpoint();
                for offset in 0..count {
                    self.lines[row][column + offset] = c;
                }
                self.cursor.1 = column + count - 1;
            }
            return;
        }
        if let Some(digit) = c.to_digit(10)
            && (digit != 0 || self.pending.count.is_some())
            && self.pending.prefix.is_none()
        {
            let count = self.pending.count.unwrap_or(0);
            self.pending.count = Some(count.saturating_mul(10).saturating_add(digit as usize));
            return;
        }
        // `2d3w` acts on six words: the two counts multiply.
        let count = match (self.pending.operator_count, self.pending.count) {
            (Some(before), Some(after)) => Some(before.saturating_mul(after)),
            (before, after) => before.or(after),
        };
        let n = count.unwrap_or(1);
        let operator = self.pending.operator;
        let prefix = self.pending.prefix.take();

        if prefix == Some('g') {
            self.pending = Pending::default();
            if c == 'g' {
                let target = count.map_or(0, |line| line.saturating_sub(1));
                let target = (target.min(self.last_row()), 0);
                let target = (target.0, self.first_non_blank(target.0));
                match operator {
                    Some(operator) => self.operate(operator, self.cursor, target, Span::Linewise),
                    None => self.move_to(target, false),
                }
            }
            return;
        }
        if prefix == Some('i') {
            self.pending = Pending::default();
            if let (Some(operator), 'w') = (operator, c) {
                let (row, column) = self.cursor;
                if self.len(row) > 0 {
                    let kind = self.class_at(self.cursor);
                    let mut start = column;
                    while start > 0 && self.class_at((row, start - 1)) == kind {
                        start -= 1;
                    }
                    let mut end = column;
                    while end + 1 < self.len(row) && self.class_at((row, end + 1)) == kind {
                        end += 1;
                    }
                    self.operate(operator, (row, start), (row, end), Span::Inclusive);
                }
            }
            return;
        }

        if let Some(operator) = operator {
            self.pending = Pending::default();
            if c == operator {
                let last = (self.cursor.0 + n - 1).min(self.last_row());
                self.operate(operator, self.cursor, (last, 0), Span::Linewise);
            } else if c == 'g' || c == 'i' {
                self.pending = Pending {
                    count,
                    operator_count: None,
                    operator: Some(operator),
                    prefix: Some(c),
                };
            } else if operator == 'c' && c == 'w' && self.class_at(self.cursor) != Class::Space {
                // `cw` changes to the end of the word, like `ce`.
                if let Some((target, span)) = self.motion('e', count) {
                    self.operate(operator, self.cursor, target, span);
                }
            } else if let Some((mut target, span)) = self.motion(c, count) {
                // `dw` on the last word of a line stops at the end of that line.
                if c == 'w' && target.0 > self.cursor.0 {
                    target = (self.cursor.0, self.len(self.cursor.0));
                }
                self.operate(operator, self.cursor, target, span);
            }
            return;
        }

        self.pending = Pending::default();
        let (row, column) = self.cursor;
        match c {
            'd' | 'c' | 'y' | '>' | '<' => {
                self.pending = Pending {
                    count: None,
                    operator_count: count,
                    operator: Some(c),
                    prefix: None,
                };
            }
            'g' | 'r' => {
                self.pending = Pending {
                    count,
                    operator_count: None,
                    operator: None,
                    prefix: Some(c),
                };
            }
            'x' => {
                let end = (column + n).min(self.len(row));
                if end > column {
                    self.operate('d', (row, column), (row, end), Span::Exclusive);
                }
            }
            'X' => {
                let start = column.saturating_sub(n);
                if start < column {
                    self.operate('d', (row, start), (row, column), Span::Exclusive);
                }
            }
            'D' | 'C' => {
                let operator = if c == 'D' { 'd' } else { 'c' };
                self.operate(
                    operator,
                    (row, column),
                    (row, self.len(row)),
                    Span::Exclusive,
                );
            }
            's' => {
                let end = (column + n).min(self.len(row));
                self.operate('c', (row, column), (row, end), Span::Exclusive);
            }
            'S' => self.operate('c', (row, 0), (row, 0), Span::Linewise),
            'Y' => {
                let last = (row + n - 1).min(self.last_row());
                self.operate('y', (row, column), (last, 0), Span::Linewise);
            }
            'J' => {
                if row < self.last_row() {
                    self.checkpoint();
                    let next = self.lines.remove(row + 1);
                    let line = &mut self.lines[row];
                    while line.last().is_some_and(|c| c.is_whitespace()) {
                        line.pop();
                    }
                    let joint = line.len();
                    let rest: Line = next.into_iter().skip_while(|c| c.is_whitespace()).collect();
                    if !line.is_empty() && !rest.is_empty() {
                        line.push(' ');
                    }
                    line.extend(rest);
                    self.cursor.1 = joint;
                }
            }
            'i' => self.enter_insert(),
            'a' => {
                self.enter_insert();
                self.cursor.1 = (column + 1).min(self.len(row));
            }
            'I' => {
                self.enter_insert();
                self.cursor.1 = self.first_non_blank(row);
            }
            'A' => {
                self.enter_insert();
                self.cursor.1 = self.len(row);
            }
            'o' | 'O' => {
                self.enter_insert();
                let indent = self.indent_of(row);
                let target = if c == 'o' { row + 1 } else { row };
                self.cursor = (target, indent.len());
                self.lines.insert(target, indent);
            }
            'p' => self.paste(true, n),
            'P' => self.paste(false, n),
            'u' => {
                for _ in 0..n {
                    self.undo();
                }
            }
            'v' | 'V' => {
                self.anchor = self.cursor;
                self.mode = if c == 'v' {
                    Mode::Visual
                } else {
                    Mode::VisualLine
                };
            }
            _ => {
                if let Some((target, span)) = self.motion(c, count) {
                    self.move_to(target, span == Span::Linewise && c != 'G');
                }
            }
        }
        self.wanted_column = if matches!(c, 'j' | 'k') {
            self.wanted_column
        } else {
            self.cursor.1
        };
    }

    fn visual_key(&mut self, key: VimKey) {
        match key {
            VimKey::Escape => {
                self.mode = Mode::Normal;
                self.pending = Pending::default();
                return;
            }
            VimKey::HalfPageDown | VimKey::HalfPageUp => {
                self.half_page(key == VimKey::HalfPageDown);
                return;
            }
            _ => {}
        }
        let Some(c) = Self::as_command(key) else {
            return;
        };
        if let Some(digit) = c.to_digit(10)
            && (digit != 0 || self.pending.count.is_some())
        {
            let count = self.pending.count.unwrap_or(0);
            self.pending.count = Some(count.saturating_mul(10).saturating_add(digit as usize));
            return;
        }
        let count = self.pending.count.take();
        if self.pending.prefix.take() == Some('g') {
            if c == 'g' {
                self.move_to((0, 0), false);
            }
            return;
        }
        let Some(selection) = self.selection() else {
            return;
        };
        let span = if selection.linewise {
            Span::Linewise
        } else {
            Span::Inclusive
        };
        match c {
            'g' => self.pending.prefix = Some('g'),
            'v' | 'V' => {
                let wanted = if c == 'v' {
                    Mode::Visual
                } else {
                    Mode::VisualLine
                };
                self.mode = if self.mode == wanted {
                    Mode::Normal
                } else {
                    wanted
                };
            }
            'd' | 'x' | 'y' | 'c' | 's' | '>' | '<' => {
                let operator = match c {
                    'x' => 'd',
                    's' => 'c',
                    other => other,
                };
                self.mode = Mode::Normal;
                self.operate(operator, selection.start, selection.end, span);
            }
            _ => {
                if let Some((target, span)) = self.motion(c, count) {
                    // In visual mode `$` and `l` may rest on the last character.
                    self.cursor = target;
                    if span != Span::Linewise || c == 'G' {
                        self.wanted_column = target.1;
                    }
                }
            }
        }
    }
}

fn ordered(a: Pos, b: Pos) -> (Pos, Pos) {
    if a <= b { (a, b) } else { (b, a) }
}

fn split_lines(text: &str) -> (Vec<Line>, bool) {
    let final_newline = text.ends_with('\n');
    let body = text.strip_suffix('\n').unwrap_or(text);
    let lines = body
        .split('\n')
        .map(|line| line.trim_end_matches('\r').chars().collect())
        .collect();
    (lines, final_newline)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, keys: &str) -> VimBuffer {
        let mut buffer = VimBuffer::new(text);
        feed(&mut buffer, keys);
        buffer
    }

    /// `<esc>`, `<cr>`, `<bs>` and `<c-r>` name the keys that are not characters.
    fn feed(buffer: &mut VimBuffer, keys: &str) {
        let mut rest = keys;
        while let Some(c) = rest.chars().next() {
            let named = [
                ("<esc>", VimKey::Escape),
                ("<cr>", VimKey::Enter),
                ("<bs>", VimKey::Backspace),
                ("<tab>", VimKey::Tab),
                ("<c-r>", VimKey::Redo),
            ]
            .into_iter()
            .find(|(name, _)| rest.starts_with(name));
            if let Some((name, key)) = named {
                buffer.key(key);
                rest = &rest[name.len()..];
            } else {
                buffer.key(VimKey::Char(c));
                rest = &rest[c.len_utf8()..];
            }
        }
    }

    #[test]
    fn typing_needs_insert_mode_and_escape_returns_to_normal() {
        let buffer = run("abc", "ixy<esc>");
        assert_eq!(buffer.text(), "xyabc");
        assert_eq!(buffer.mode(), Mode::Normal);
        assert_eq!(buffer.cursor(), (0, 1));
        // In normal mode `x` is a command, not text.
        assert_eq!(run("abc", "x").text(), "bc");
    }

    #[test]
    fn motions_move_by_character_word_and_line() {
        let text = "let size = 10;\n    draw(size);\nend";
        assert_eq!(run(text, "w").cursor(), (0, 4));
        assert_eq!(run(text, "3w").cursor(), (0, 11));
        assert_eq!(run(text, "e").cursor(), (0, 2));
        assert_eq!(run(text, "$").cursor(), (0, 13));
        assert_eq!(run(text, "$b").cursor(), (0, 11));
        assert_eq!(run(text, "j^").cursor(), (1, 4));
        assert_eq!(run(text, "G").cursor(), (2, 0));
        assert_eq!(run(text, "Ggg").cursor(), (0, 0));
        assert_eq!(run(text, "2G").cursor(), (1, 4));
        // The column returns after a shorter line.
        assert_eq!(run(text, "$jjk").cursor(), (1, 13));
    }

    #[test]
    fn operators_delete_change_and_yank() {
        assert_eq!(run("one two three", "dw").text(), "two three");
        assert_eq!(run("one two three", "wd$").text(), "one ");
        assert_eq!(run("one two three", "wD").text(), "one ");
        assert_eq!(run("one two three", "wcwsix<esc>").text(), "one six three");
        assert_eq!(
            run("one two three", "wlciwsix<esc>").text(),
            "one six three"
        );
        assert_eq!(run("a\nb\nc", "jdd").text(), "a\nc");
        assert_eq!(run("a\nb\nc", "2dd").text(), "c");
        assert_eq!(run("a\nb\nc", "dj").text(), "c");
        assert_eq!(run("a\nb\nc", "yyjp").text(), "a\nb\na\nc");
        assert_eq!(run("a\nb\nc", "ddp").text(), "b\na\nc");
        assert_eq!(run("ab", "xp").text(), "ba");
        assert_eq!(run("  a\nb", "ccx<esc>").text(), "  x\nb");
        assert_eq!(run("only", "dd").text(), "");
    }

    #[test]
    fn dw_on_the_last_word_keeps_the_next_line() {
        assert_eq!(run("one two\nthree", "wdw").text(), "one \nthree");
    }

    #[test]
    fn opening_a_line_keeps_the_indent() {
        assert_eq!(run("    a\nb", "ox<esc>").text(), "    a\n    x\nb");
        assert_eq!(run("    a\nb", "Ox<esc>").text(), "    x\n    a\nb");
        assert_eq!(run("    ab", "A<cr>c<esc>").text(), "    ab\n    c");
    }

    #[test]
    fn insert_mode_tab_and_backspace_use_tab_stops() {
        assert_eq!(run("", "i<tab>a<esc>").text(), "    a");
        assert_eq!(run("        a", "wi<bs><esc>").text(), "    a");
        assert_eq!(run("a\nb", "ji<bs><esc>").text(), "ab");
    }

    #[test]
    fn undo_and_redo_step_through_whole_commands() {
        let mut buffer = run("one two", "dwix<esc>");
        assert_eq!(buffer.text(), "xtwo");
        feed(&mut buffer, "u");
        assert_eq!(buffer.text(), "two");
        feed(&mut buffer, "u");
        assert_eq!(buffer.text(), "one two");
        feed(&mut buffer, "<c-r>");
        assert_eq!(buffer.text(), "two");
        // An insert that typed nothing is not an undo step.
        let mut buffer = run("one", "xi<esc>u");
        assert_eq!(buffer.text(), "one");
        assert!(!buffer.undo());
    }

    #[test]
    fn visual_modes_select_and_operate() {
        assert_eq!(run("one two three", "wved").text(), "one  three");
        assert_eq!(run("a\nb\nc", "Vjd").text(), "c");
        assert_eq!(run("a\nb", "Vj>").text(), "    a\n    b");
        assert_eq!(run("    a\n    b", "Vj<").text(), "a\nb");
        let buffer = run("one two", "vl");
        let selection = buffer.selection().expect("a selection");
        assert_eq!((selection.start, selection.end), ((0, 0), (0, 1)));
        assert_eq!(run("one two", "vly$p").text(), "one twoon");
    }

    #[test]
    fn small_commands_replace_join_and_shift() {
        assert_eq!(run("abc", "rx").text(), "xbc");
        assert_eq!(run("a\n  b", "J").text(), "a b");
        assert_eq!(run("a", ">>").text(), "    a");
        assert_eq!(run("      a", "<<").text(), "  a");
        assert_eq!(run("abc", "lsx<esc>").text(), "axc");
    }

    #[test]
    fn replace_followed_by_a_key_that_is_not_a_character_cancels() {
        assert_eq!(run("abc", "r<bs>").text(), "abc");
        assert_eq!(run("abc", "r<cr>").text(), "abc");
        let mut buffer = VimBuffer::new("abc");
        buffer.key(VimKey::Char('r'));
        buffer.key(VimKey::Right);
        assert_eq!(buffer.text(), "abc");
        assert_eq!(buffer.cursor(), (0, 0));
        // The next character is a command again.
        feed(&mut buffer, "x");
        assert_eq!(buffer.text(), "bc");
    }

    #[test]
    fn counts_before_and_after_an_operator_multiply() {
        let text = "a b c d e f g h";
        assert_eq!(run(text, "2d3w").text(), "g h");
        assert_eq!(run(text, "d3w").text(), "d e f g h");
        assert_eq!(run(text, "3dw").text(), "d e f g h");
        assert_eq!(run("1\n2\n3\n4\n5", "2d2d").text(), "5");
    }

    #[test]
    fn a_yank_is_offered_to_the_clipboard_once() {
        let mut buffer = run("one\ntwo", "yy");
        assert_eq!(buffer.take_yanked().as_deref(), Some("one\n"));
        assert_eq!(buffer.take_yanked(), None);
    }

    #[test]
    fn external_text_keeps_the_final_newline_and_paste_inserts_at_the_cursor() {
        let mut buffer = VimBuffer::new("a\nb\n");
        assert_eq!(buffer.line_count(), 2);
        assert_eq!(buffer.text(), "a\nb\n");
        assert!(buffer.insert_str("x\ny"));
        assert_eq!(buffer.text(), "x\nya\nb\n");
        buffer.set_text("z");
        assert_eq!(buffer.cursor(), (0, 0));
        assert!(!buffer.undo());
    }
}
