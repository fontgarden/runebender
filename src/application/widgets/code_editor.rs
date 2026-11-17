// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A modal code editor for script nodes: Vim keys, syntax colors and line numbers.
//!
//! The editing commands live in [`runebender::ui::vim`] and the highlighter in
//! [`runebender::ui::syntax`]; this widget translates key events, scrolls, and paints.
//! It has no scrollbars: the view follows the cursor, and the wheel scrolls it while focused.
//! Input methods are not enabled, so every key press arrives as a key event; a held key repeats
//! instead of opening the system's accent menu.

use std::cell::RefCell;
use std::ops::Range;

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, Modifiers, NamedKey};
use masonry::core::{
    AccessCtx, BrushIndex, ChildrenIds, CursorIcon, EventCtx, Ime, LayoutCtx, MeasureCtx, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerScrollEvent, PropertiesMut,
    PropertiesRef, QueryCtx, RegisterCtx, ScrollDelta, TextEvent, Update, UpdateCtx, Widget,
    WidgetMut, render_text,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, Point, Rect, Size};
use masonry::layout::{LenReq, Length};
use masonry::parley::{
    FontContext, FontFamily, FontFamilyName, FontStyle, FontWeight, GenericFamily, Layout,
    LayoutContext, StyleProperty,
};
use masonry::peniko::Brush;
use masonry::widgets::TextAction;
use runebender::ui::syntax::{LineState, Syntax, TokenKind, highlight_line};
use runebender::ui::vim::{Mode, VimBuffer, VimKey};

use crate::application::design::Stroke;
use crate::application::design::theme::CodeColors;

thread_local! {
    static FONT_CX: RefCell<FontContext> = RefCell::new(FontContext::new());
    static LAYOUT_CX: RefCell<LayoutContext<BrushIndex>> = RefCell::new(LayoutContext::new());
}

/// Brush slots a styled run can name.
#[derive(Clone, Copy)]
enum Ink {
    Text,
    Keyword,
    Function,
    Type,
    String,
    Number,
    Comment,
    Constant,
    Operator,
    LineNumber,
    Background,
}

impl Ink {
    fn index(self) -> BrushIndex {
        BrushIndex(self as usize)
    }
}

/// One styled run of a line, in bytes of the drawn text.
struct Run {
    range: Range<usize>,
    ink: Ink,
    bold: bool,
    italic: bool,
}

fn style_of(kind: TokenKind) -> (Ink, bool, bool) {
    match kind {
        TokenKind::Plain => (Ink::Text, false, false),
        TokenKind::Keyword => (Ink::Keyword, true, false),
        TokenKind::Function => (Ink::Function, true, false),
        TokenKind::Type => (Ink::Type, true, false),
        TokenKind::String => (Ink::String, false, false),
        TokenKind::Number => (Ink::Number, false, false),
        TokenKind::Boolean => (Ink::Number, true, false),
        TokenKind::Comment => (Ink::Comment, false, true),
        TokenKind::Constant => (Ink::Constant, false, false),
        TokenKind::Operator => (Ink::Operator, false, false),
    }
}

fn shaped(text: &str, size: f32, runs: &[Run]) -> Layout<BrushIndex> {
    FONT_CX.with(|fonts| {
        LAYOUT_CX.with(|layouts| {
            let mut fonts = fonts.borrow_mut();
            let mut layouts = layouts.borrow_mut();
            let mut builder = layouts.ranged_builder(&mut fonts, text, 1.0, true);
            builder.push_default(StyleProperty::FontSize(size));
            builder.push_default(StyleProperty::FontFamily(FontFamily::Single(
                FontFamilyName::Generic(GenericFamily::Monospace),
            )));
            builder.push_default(StyleProperty::Brush(Ink::Text.index()));
            for run in runs {
                builder.push(StyleProperty::Brush(run.ink.index()), run.range.clone());
                if run.bold {
                    builder.push(
                        StyleProperty::FontWeight(FontWeight::BOLD),
                        run.range.clone(),
                    );
                }
                if run.italic {
                    builder.push(
                        StyleProperty::FontStyle(FontStyle::Italic),
                        run.range.clone(),
                    );
                }
            }
            let mut layout: Layout<BrushIndex> = builder.build(text);
            layout.break_all_lines(None);
            layout
        })
    })
}

/// Width of one character cell and height of one line at `size`.
fn cell(size: f32) -> (f64, f64) {
    const SAMPLE: &str = "0000000000";
    let layout = shaped(SAMPLE, size, &[]);
    let width = f64::from(layout.width()) / SAMPLE.len() as f64;
    (width.max(1.0), f64::from(layout.height()).ceil().max(1.0))
}

/// A length in whole cells: floored, and zero for a negative or non-finite value.
fn whole(value: f64) -> usize {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive finite count of lines or columns on screen"
    )]
    let count = value.floor().min(1.0e6) as usize;
    count
}

/// The code editor widget.
pub(crate) struct CodeEditor {
    buffer: VimBuffer,
    syntax: Syntax,
    text_size: f32,
    colors: CodeColors,
    /// First visible line and column.
    top: usize,
    left: usize,
    /// Wheel movement not yet worth a whole line.
    wheel: f64,
    size: Size,
    cell: (f64, f64),
}

impl CodeEditor {
    /// Create an editor in normal mode that highlights `contents` as `syntax`.
    pub(crate) fn new(contents: &str, syntax: Syntax, text_size: f32, colors: CodeColors) -> Self {
        Self {
            buffer: VimBuffer::new(contents),
            syntax,
            text_size,
            colors,
            top: 0,
            left: 0,
            wheel: 0.0,
            size: Size::ZERO,
            cell: cell(text_size),
        }
    }

    /// Replace a genuinely external value; matching text keeps the cursor, mode and history.
    pub(crate) fn replace_external_content(
        this: &mut WidgetMut<'_, Self>,
        contents: &str,
        syntax: Syntax,
    ) {
        if this.widget.syntax != syntax {
            this.widget.syntax = syntax;
            this.ctx.request_render();
        }
        if this.widget.buffer.text() != contents {
            this.widget.buffer.set_text(contents);
            this.widget.reveal_cursor();
            this.ctx.request_render();
        }
    }

    /// Change the rendered text size, as the graph's zoom changes.
    pub(crate) fn set_text_size(this: &mut WidgetMut<'_, Self>, text_size: f32) {
        if (this.widget.text_size - text_size).abs() > f32::EPSILON {
            this.widget.text_size = text_size;
            this.widget.cell = cell(text_size);
            this.widget.reveal_cursor();
            this.ctx.request_render();
        }
    }

    /// Change the colors, as the theme changes.
    pub(crate) fn set_colors(this: &mut WidgetMut<'_, Self>, colors: CodeColors) {
        if this.widget.colors != colors {
            this.widget.colors = colors;
            this.ctx.request_render();
        }
    }

    /// Columns the line-number gutter takes, including its space before the text.
    fn gutter_columns(&self) -> usize {
        self.buffer.line_count().to_string().len().max(2) + 2
    }

    /// Space above the first line: half a line, so the text does not touch the node's edge.
    fn top_inset(&self) -> f64 {
        (self.cell.1 / 2.0).floor()
    }

    /// Whole text lines and columns that fit, leaving the last line for the status.
    fn visible(&self) -> (usize, usize) {
        let (width, height) = self.cell;
        let rows = whole((self.size.height - self.top_inset()) / height);
        let columns = whole(self.size.width / width);
        (
            rows.saturating_sub(1).max(1),
            columns.saturating_sub(self.gutter_columns() + 1).max(1),
        )
    }

    /// Scroll just far enough to show the cursor.
    fn reveal_cursor(&mut self) {
        let (rows, columns) = self.visible();
        let (row, column) = self.buffer.cursor();
        if row < self.top {
            self.top = row;
        } else if row >= self.top + rows {
            self.top = row + 1 - rows;
        }
        if column < self.left {
            self.left = column;
        } else if column >= self.left + columns {
            self.left = column + 1 - columns;
        }
        self.buffer.set_half_page(rows / 2);
    }

    /// Scroll by whole lines and keep the cursor inside the view, as Vim does.
    fn scroll_lines(&mut self, lines: isize) {
        let (rows, _) = self.visible();
        let last = self.buffer.line_count().saturating_sub(1);
        self.top = self.top.saturating_add_signed(lines).min(last);
        let (row, column) = self.buffer.cursor();
        let kept = row.clamp(self.top, (self.top + rows - 1).min(last));
        if kept != row {
            self.buffer.set_cursor(kept, column);
        }
    }

    fn position_at(&self, point: Point) -> (usize, usize) {
        let (width, height) = self.cell;
        let row = self.top + whole((point.y - self.top_inset()) / height);
        let text_x = self.gutter_columns() as f64 * width;
        let column = self.left + whole((point.x - text_x) / width);
        (row, column)
    }

    /// Report what a key or paste did: new text to the graph, a yank to the clipboard.
    fn after_edit(&mut self, ctx: &mut EventCtx<'_>, changed: bool) {
        if let Some(yanked) = self.buffer.take_yanked() {
            ctx.set_clipboard(yanked);
        }
        if changed {
            ctx.submit_action::<TextAction>(TextAction::Changed(self.buffer.text()));
        }
        self.reveal_cursor();
        ctx.request_render();
        ctx.set_handled();
    }

    fn status(&self) -> &'static str {
        match self.buffer.mode() {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Visual => "VISUAL",
            Mode::VisualLine => "V-LINE",
        }
    }
}

/// The platform's undo, redo and copy shortcuts, which work in every mode.
enum Shortcut {
    Undo,
    Redo,
    Copy,
}

fn shortcut(key: &Key, modifiers: Modifiers) -> Option<Shortcut> {
    let Key::Character(character) = key else {
        return None;
    };
    let character = character.to_ascii_lowercase();
    if cfg!(target_os = "macos") {
        if !modifiers.meta() {
            return None;
        }
        match character.as_str() {
            "z" if modifiers.shift() => Some(Shortcut::Redo),
            "z" => Some(Shortcut::Undo),
            "c" => Some(Shortcut::Copy),
            _ => None,
        }
    } else {
        if !modifiers.ctrl() {
            return None;
        }
        // Control-C alone stays Vim's Escape; copy takes Shift as terminals do.
        match character.as_str() {
            "z" if modifiers.shift() => Some(Shortcut::Redo),
            "z" => Some(Shortcut::Undo),
            "y" => Some(Shortcut::Redo),
            "c" if modifiers.shift() => Some(Shortcut::Copy),
            _ => None,
        }
    }
}

fn vim_key(key: &Key, modifiers: Modifiers) -> Option<VimKey> {
    if modifiers.meta() {
        return None;
    }
    if modifiers.ctrl() {
        let Key::Character(character) = key else {
            return None;
        };
        return match character.to_ascii_lowercase().as_str() {
            "r" => Some(VimKey::Redo),
            "d" => Some(VimKey::HalfPageDown),
            "u" => Some(VimKey::HalfPageUp),
            "[" | "c" => Some(VimKey::Escape),
            _ => None,
        };
    }
    Some(match key {
        Key::Named(NamedKey::Escape) => VimKey::Escape,
        Key::Named(NamedKey::Enter) => VimKey::Enter,
        Key::Named(NamedKey::Backspace) => VimKey::Backspace,
        Key::Named(NamedKey::Delete) => VimKey::Delete,
        Key::Named(NamedKey::Tab) => VimKey::Tab,
        Key::Named(NamedKey::ArrowLeft) => VimKey::Left,
        Key::Named(NamedKey::ArrowRight) => VimKey::Right,
        Key::Named(NamedKey::ArrowUp) => VimKey::Up,
        Key::Named(NamedKey::ArrowDown) => VimKey::Down,
        Key::Named(NamedKey::Home) => VimKey::Home,
        Key::Named(NamedKey::End) => VimKey::End,
        Key::Character(character) => {
            let mut characters = character.chars();
            match (characters.next(), characters.next()) {
                (Some(c), None) if !c.is_control() => VimKey::Char(c),
                _ => return None,
            }
        }
        _ => return None,
    })
}

impl Widget for CodeEditor {
    type Action = TextAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            // The press is left unhandled, so the graph also selects this editor's node.
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                ctx.request_focus();
                let (row, column) = self.position_at(ctx.local_position(state.position));
                self.buffer.set_cursor(row, column);
                ctx.request_render();
            }
            // An unfocused editor lets the wheel reach the graph, which zooms.
            PointerEvent::Scroll(PointerScrollEvent { delta, .. }) if ctx.is_focus_target() => {
                let pixels = match delta {
                    ScrollDelta::PixelDelta(position) => position.y,
                    ScrollDelta::LineDelta(_, lines) => f64::from(*lines) * self.cell.1,
                    _ => 0.0,
                };
                self.wheel -= pixels / self.cell.1;
                let lines = self.wheel.trunc();
                self.wheel -= lines;
                let count = whole(lines.abs()).cast_signed();
                self.scroll_lines(if lines < 0.0 { -count } else { count });
                ctx.request_render();
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        match event {
            TextEvent::Keyboard(key) if key.state == KeyState::Down => {
                if let Some(shortcut) = shortcut(&key.key, key.modifiers) {
                    let changed = match shortcut {
                        Shortcut::Undo => self.buffer.undo(),
                        Shortcut::Redo => self.buffer.redo(),
                        // Copy takes the visual selection; without one there is nothing to copy.
                        Shortcut::Copy => {
                            self.buffer.selection().is_some() && self.buffer.key(VimKey::Char('y'))
                        }
                    };
                    self.after_edit(ctx, changed);
                } else if let Some(key) = vim_key(&key.key, key.modifiers) {
                    let changed = self.buffer.key(key);
                    self.after_edit(ctx, changed);
                }
            }
            TextEvent::ClipboardPaste(text) => {
                let changed = self.buffer.insert_str(text);
                self.after_edit(ctx, changed);
            }
            // Composed text arrives whole; read it as the keys it stands for.
            TextEvent::Ime(Ime::Commit(text)) => {
                let mut changed = false;
                for c in text.chars() {
                    changed |= self.buffer.key(VimKey::Char(c));
                }
                self.after_edit(ctx, changed);
            }
            _ => {}
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::FocusChanged(_)) {
            ctx.request_render();
        }
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross: Option<Length>,
    ) -> Length {
        // The graph gives every editor its node's content box; this is only a fallback.
        let (width, height) = self.cell;
        Length::px(match axis {
            Axis::Horizontal => width * 40.0,
            Axis::Vertical => height * 10.0,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let colors = self.colors;
        let brushes: [Brush; 11] = [
            colors.text.into(),
            colors.keyword.into(),
            colors.function.into(),
            colors.type_name.into(),
            colors.string.into(),
            colors.number.into(),
            colors.comment.into(),
            colors.constant.into(),
            colors.operator.into(),
            colors.line_number.into(),
            colors.background.into(),
        ];
        let size = self.size;
        let (width, height) = self.cell;
        painter.fill(size.to_rect(), colors.background).draw();

        let (rows, columns) = self.visible();
        let inset = self.top_inset();
        let gutter = self.gutter_columns();
        let text_x = gutter as f64 * width;
        let (cursor_row, cursor_column) = self.buffer.cursor();
        let selection = self.buffer.selection();
        let focused = ctx.is_focus_target();

        // Highlighting state depends on every line above the first visible one.
        let mut state = LineState::Code;
        for row in 0..self.top.min(self.buffer.line_count()) {
            highlight_line(self.buffer.line(row), self.syntax, &mut state);
        }
        let last = (self.top + rows).min(self.buffer.line_count());
        for row in self.top..last {
            let y = inset + (row - self.top) as f64 * height;
            let line = self.buffer.line(row);
            let tokens = highlight_line(line, self.syntax, &mut state);
            let shown = self.left.min(line.len())..(self.left + columns + 1).min(line.len());

            if let Some(selection) = selection
                && (selection.start.0..=selection.end.0).contains(&row)
            {
                let from = if selection.linewise || row > selection.start.0 {
                    0
                } else {
                    selection.start.1
                };
                let to = if selection.linewise || row < selection.end.0 {
                    line.len().max(1)
                } else {
                    selection.end.1 + 1
                };
                let x0 = text_x + from.saturating_sub(self.left) as f64 * width;
                let x1 = text_x + to.saturating_sub(self.left) as f64 * width;
                if x1 > x0 {
                    painter
                        .fill(Rect::new(x0, y, x1, y + height), colors.selection)
                        .draw();
                }
            }

            // The line number, right-aligned; the cursor's line uses the function color.
            let number = format!("{:>width$}", row + 1, width = gutter - 2);
            let number_ink = if row == cursor_row && focused {
                Ink::Function
            } else {
                Ink::LineNumber
            };
            let number_run = Run {
                range: 0..number.len(),
                ink: number_ink,
                bold: false,
                italic: false,
            };
            let layout = shaped(&number, self.text_size, &[number_run]);
            render_text(
                painter,
                Affine::translate((width, y)),
                &layout,
                &brushes,
                false,
            );

            if !shown.is_empty() {
                let text: String = line[shown.clone()].iter().collect();
                // Byte offset of every shown character, for runs.
                let mut offsets: Vec<usize> = text.char_indices().map(|(at, _)| at).collect();
                offsets.push(text.len());
                let runs: Vec<Run> = tokens
                    .iter()
                    .filter_map(|token| {
                        let start = token.start.max(shown.start);
                        let end = token.end.min(shown.end);
                        (start < end).then(|| {
                            let (ink, bold, italic) = style_of(token.kind);
                            Run {
                                range: offsets[start - shown.start]..offsets[end - shown.start],
                                ink,
                                bold,
                                italic,
                            }
                        })
                    })
                    .collect();
                let layout = shaped(&text, self.text_size, &runs);
                render_text(
                    painter,
                    Affine::translate((text_x, y)),
                    &layout,
                    &brushes,
                    false,
                );
            }

            if focused && row == cursor_row && cursor_column >= self.left {
                let x = text_x + (cursor_column - self.left) as f64 * width;
                if self.buffer.mode() == Mode::Insert {
                    let bar = Stroke::Hairline.px().max(width / 6.0);
                    painter
                        .fill(Rect::new(x, y, x + bar, y + height), colors.text)
                        .draw();
                } else {
                    // A block cursor: the text color, with its character in the ground color.
                    painter
                        .fill(Rect::new(x, y, x + width, y + height), colors.text)
                        .draw();
                    if let Some(character) = line.get(cursor_column) {
                        let text = character.to_string();
                        let run = Run {
                            range: 0..text.len(),
                            ink: Ink::Background,
                            bold: false,
                            italic: false,
                        };
                        let layout = shaped(&text, self.text_size, &[run]);
                        render_text(painter, Affine::translate((x, y)), &layout, &brushes, false);
                    }
                }
            }
        }

        // The status line: the mode at the left, the cursor position at the right.
        if focused {
            let y = inset + rows as f64 * height;
            let position = format!("{}:{}", cursor_row + 1, cursor_column + 1);
            for (text, x) in [
                (self.status().to_owned(), width),
                (
                    position.clone(),
                    size.width - (position.len() + 1) as f64 * width,
                ),
            ] {
                let run = Run {
                    range: 0..text.len(),
                    ink: Ink::Comment,
                    bold: false,
                    italic: false,
                };
                let layout = shaped(&text, self.text_size, &[run]);
                render_text(painter, Affine::translate((x, y)), &layout, &brushes, false);
            }
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::MultilineTextInput
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_value(self.buffer.text());
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        false
    }

    fn get_cursor(&self, _ctx: &QueryCtx<'_>, _point: Point) -> CursorIcon {
        CursorIcon::Text
    }
}
