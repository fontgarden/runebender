// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Line-by-line syntax highlighting for the script languages the editor runs.
//!
//! This is a lexical highlighter: it reads characters, not a syntax tree.
//! A [`LineState`] carries a block comment or a multi-line string from one line to the next.

/// A language the highlighter reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    /// Rust, as in `DesignBot` specimen scripts.
    Rust,
    /// Python, as in `DrawBot` specimen scripts and recipes.
    Python,
}

impl Syntax {
    /// Choose the language a script is most likely written in.
    pub fn detect(text: &str) -> Self {
        let rust = ["fn ", "let ", "use ", "pub ", "impl ", "//"];
        let python = ["def ", "import ", "elif ", "print(", "#"];
        let score = |markers: &[&str]| {
            text.lines()
                .map(str::trim_start)
                .filter(|line| markers.iter().any(|marker| line.starts_with(marker)))
                .count()
        };
        if score(&python) > score(&rust) {
            Self::Python
        } else {
            Self::Rust
        }
    }
}

/// What a run of characters is, for coloring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Identifiers, delimiters and anything else.
    Plain,
    /// A language keyword.
    Keyword,
    /// A name that is called or defined as a function.
    Function,
    /// A type name.
    Type,
    /// A string or character literal.
    String,
    /// A numeric literal.
    Number,
    /// `true`, `false`, `True` or `False`.
    Boolean,
    /// A comment.
    Comment,
    /// A constant, a macro or a decorator.
    Constant,
    /// An operator.
    Operator,
}

/// One colored run within a line, in character columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    /// First column.
    pub start: usize,
    /// Column after the last.
    pub end: usize,
    /// What the run is.
    pub kind: TokenKind,
}

/// What continues from the previous line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineState {
    /// Nothing continues.
    #[default]
    Code,
    /// A Rust block comment, nested this deep.
    BlockComment(u32),
    /// A string that ends at this delimiter: one quote character, repeated one or three times.
    String(char, u8),
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "type", "unsafe", "use",
    "where", "while",
];
const RUST_TYPES: &[&str] = &[
    "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "str", "u8", "u16",
    "u32", "u64", "u128", "usize",
];
const PYTHON_KEYWORDS: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda",
    "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with", "yield",
];

/// Highlight one line. `state` is read for what continues into the line and left as what
/// continues out of it. Runs of [`TokenKind::Plain`] are included, so the tokens cover the line.
pub fn highlight_line(line: &[char], syntax: Syntax, state: &mut LineState) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut push = |start: usize, end: usize, kind: TokenKind| {
        if end <= start {
            return;
        }
        match tokens.last_mut() {
            Some(last) if last.kind == kind && last.end == start => last.end = end,
            _ => tokens.push(Token { start, end, kind }),
        }
    };
    let len = line.len();
    let at = |index: usize| line.get(index).copied();
    let mut i = 0;
    while i < len {
        match *state {
            LineState::BlockComment(depth) => {
                let start = i;
                let mut depth = depth;
                while i < len && depth > 0 {
                    if at(i) == Some('*') && at(i + 1) == Some('/') {
                        depth -= 1;
                        i += 2;
                    } else if at(i) == Some('/') && at(i + 1) == Some('*') {
                        depth += 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                *state = if depth == 0 {
                    LineState::Code
                } else {
                    LineState::BlockComment(depth)
                };
                push(start, i, TokenKind::Comment);
                continue;
            }
            LineState::String(quote, repeat) => {
                let start = i;
                let repeat = usize::from(repeat);
                let mut closed = false;
                while i < len {
                    if at(i) == Some('\\') {
                        i += 2;
                    } else if (0..repeat).all(|offset| at(i + offset) == Some(quote)) {
                        i += repeat;
                        closed = true;
                        break;
                    } else {
                        i += 1;
                    }
                }
                let i_end = i.min(len);
                push(start, i_end, TokenKind::String);
                i = i_end;
                if closed {
                    *state = LineState::Code;
                } else if repeat == 1 && syntax == Syntax::Python {
                    // A single-quoted Python string ends with its line.
                    *state = LineState::Code;
                }
                continue;
            }
            LineState::Code => {}
        }
        let c = line[i];
        let next = at(i + 1);
        // Comments.
        if (syntax == Syntax::Rust && c == '/' && next == Some('/'))
            || (syntax == Syntax::Python && c == '#')
        {
            push(i, len, TokenKind::Comment);
            break;
        }
        if syntax == Syntax::Rust && c == '/' && next == Some('*') {
            push(i, i + 2, TokenKind::Comment);
            i += 2;
            *state = LineState::BlockComment(1);
            continue;
        }
        // Strings.
        if c == '"' || (c == '\'' && syntax == Syntax::Python) {
            let triple = syntax == Syntax::Python && next == Some(c) && at(i + 2) == Some(c);
            let repeat: u8 = if triple { 3 } else { 1 };
            push(i, i + usize::from(repeat), TokenKind::String);
            i += usize::from(repeat);
            *state = LineState::String(c, repeat);
            continue;
        }
        if c == '\'' {
            // A Rust character literal, or else a lifetime.
            let close = if next == Some('\\') {
                (i + 2..len.min(i + 12)).find(|index| line[*index] == '\'')
            } else if at(i + 2) == Some('\'') {
                Some(i + 2)
            } else {
                None
            };
            if let Some(close) = close {
                push(i, close + 1, TokenKind::String);
                i = close + 1;
            } else {
                push(i, i + 1, TokenKind::Plain);
                i += 1;
            }
            continue;
        }
        // Numbers.
        if c.is_ascii_digit() {
            let start = i;
            while i < len
                && (line[i].is_ascii_alphanumeric()
                    || line[i] == '_'
                    || (line[i] == '.' && at(i + 1).is_some_and(|d| d.is_ascii_digit())))
            {
                i += 1;
            }
            push(start, i, TokenKind::Number);
            continue;
        }
        // Words.
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < len && (line[i].is_alphanumeric() || line[i] == '_') {
                i += 1;
            }
            let word: String = line[start..i].iter().collect();
            let follows = at(i);
            let keywords = match syntax {
                Syntax::Rust => RUST_KEYWORDS,
                Syntax::Python => PYTHON_KEYWORDS,
            };
            let constant_case = word.len() > 1
                && word.chars().any(char::is_alphabetic)
                && word
                    .chars()
                    .all(|c| c.is_uppercase() || c.is_ascii_digit() || c == '_');
            let kind = if matches!(word.as_str(), "true" | "false" | "True" | "False") {
                TokenKind::Boolean
            } else if keywords.contains(&word.as_str()) {
                TokenKind::Keyword
            } else if syntax == Syntax::Python && word == "None" {
                TokenKind::Constant
            } else if syntax == Syntax::Rust && follows == Some('!') && at(i + 1) != Some('=') {
                // A macro takes its `!` with it.
                i += 1;
                TokenKind::Constant
            } else if syntax == Syntax::Rust && RUST_TYPES.contains(&word.as_str()) {
                TokenKind::Type
            } else if constant_case {
                TokenKind::Constant
            } else if follows == Some('(') {
                if word.starts_with(char::is_uppercase) {
                    TokenKind::Type
                } else {
                    TokenKind::Function
                }
            } else if word.starts_with(char::is_uppercase) {
                TokenKind::Type
            } else {
                TokenKind::Plain
            };
            push(start, i, kind);
            continue;
        }
        // A Python decorator.
        if syntax == Syntax::Python && c == '@' && line[..i].iter().all(|c| c.is_whitespace()) {
            let start = i;
            i += 1;
            while i < len && (line[i].is_alphanumeric() || line[i] == '_' || line[i] == '.') {
                i += 1;
            }
            push(start, i, TokenKind::Constant);
            continue;
        }
        let kind = if "+-*/%=<>!&|^~?:".contains(c) {
            TokenKind::Operator
        } else {
            TokenKind::Plain
        };
        push(i, i + 1, kind);
        i += 1;
    }
    // A Rust string continues past the end of its line; nothing else does.
    if let LineState::String(_, 1) = *state
        && syntax == Syntax::Python
    {
        *state = LineState::Code;
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str, syntax: Syntax, state: &mut LineState) -> Vec<(String, TokenKind)> {
        let chars: Vec<char> = line.chars().collect();
        highlight_line(&chars, syntax, state)
            .into_iter()
            .filter(|token| {
                chars[token.start..token.end]
                    .iter()
                    .any(|c| !c.is_whitespace())
            })
            .map(|token| (chars[token.start..token.end].iter().collect(), token.kind))
            .collect()
    }

    fn kind_of(line: &str, syntax: Syntax, word: &str) -> TokenKind {
        kinds(line, syntax, &mut LineState::Code)
            .into_iter()
            .find(|(text, _)| text.trim() == word)
            .unwrap_or_else(|| panic!("no token '{word}' in '{line}'"))
            .1
    }

    #[test]
    fn rust_tokens_get_their_kinds() {
        let line = "pub fn draw(page: &mut Page, size: f64) -> bool { // note";
        assert_eq!(kind_of(line, Syntax::Rust, "pub"), TokenKind::Keyword);
        assert_eq!(kind_of(line, Syntax::Rust, "fn"), TokenKind::Keyword);
        assert_eq!(kind_of(line, Syntax::Rust, "draw"), TokenKind::Function);
        assert_eq!(kind_of(line, Syntax::Rust, "Page"), TokenKind::Type);
        assert_eq!(kind_of(line, Syntax::Rust, "f64"), TokenKind::Type);
        assert_eq!(kind_of(line, Syntax::Rust, "bool"), TokenKind::Type);
        assert_eq!(kind_of(line, Syntax::Rust, "// note"), TokenKind::Comment);
        let line = "let x = vec![1.5, 0xFF]; let s = \"a \\\" b\"; let c = 'x'; MAX_SIZE; true";
        assert_eq!(kind_of(line, Syntax::Rust, "vec!"), TokenKind::Constant);
        assert_eq!(kind_of(line, Syntax::Rust, "1.5"), TokenKind::Number);
        assert_eq!(kind_of(line, Syntax::Rust, "0xFF"), TokenKind::Number);
        assert_eq!(
            kind_of(line, Syntax::Rust, "\"a \\\" b\""),
            TokenKind::String
        );
        assert_eq!(kind_of(line, Syntax::Rust, "'x'"), TokenKind::String);
        assert_eq!(kind_of(line, Syntax::Rust, "MAX_SIZE"), TokenKind::Constant);
        assert_eq!(kind_of(line, Syntax::Rust, "true"), TokenKind::Boolean);
        assert_eq!(kind_of(line, Syntax::Rust, "="), TokenKind::Operator);
        // A lifetime is not an unterminated character literal.
        assert_eq!(
            kind_of("fn f<'a>(x: &'a str)", Syntax::Rust, "str"),
            TokenKind::Type
        );
    }

    #[test]
    fn python_tokens_get_their_kinds() {
        let line = "def draw(size=10): return Font('a') if None else False  # note";
        assert_eq!(kind_of(line, Syntax::Python, "def"), TokenKind::Keyword);
        assert_eq!(kind_of(line, Syntax::Python, "draw"), TokenKind::Function);
        assert_eq!(kind_of(line, Syntax::Python, "10"), TokenKind::Number);
        assert_eq!(kind_of(line, Syntax::Python, "Font"), TokenKind::Type);
        assert_eq!(kind_of(line, Syntax::Python, "'a'"), TokenKind::String);
        assert_eq!(kind_of(line, Syntax::Python, "None"), TokenKind::Constant);
        assert_eq!(kind_of(line, Syntax::Python, "False"), TokenKind::Boolean);
        assert_eq!(kind_of(line, Syntax::Python, "# note"), TokenKind::Comment);
        assert_eq!(
            kind_of("@staticmethod", Syntax::Python, "@staticmethod"),
            TokenKind::Constant
        );
    }

    #[test]
    fn comments_and_strings_continue_across_lines() {
        let mut state = LineState::Code;
        kinds("let a = 1; /* start", Syntax::Rust, &mut state);
        assert_eq!(state, LineState::BlockComment(1));
        let line = kinds("still */ let b = 2;", Syntax::Rust, &mut state);
        assert_eq!(state, LineState::Code);
        assert_eq!(line[0], ("still */".into(), TokenKind::Comment));
        assert!(line.contains(&("let".into(), TokenKind::Keyword)));

        let mut state = LineState::Code;
        kinds("text = \"\"\"first", Syntax::Python, &mut state);
        assert_eq!(state, LineState::String('"', 3));
        let line = kinds("last\"\"\" + 1", Syntax::Python, &mut state);
        assert_eq!(state, LineState::Code);
        assert_eq!(line[0], ("last\"\"\"".into(), TokenKind::String));
        assert!(line.contains(&("1".into(), TokenKind::Number)));

        // An unterminated single-quoted Python string does not color the next line.
        let mut state = LineState::Code;
        kinds("name = 'open", Syntax::Python, &mut state);
        assert_eq!(state, LineState::Code);
    }

    #[test]
    fn tokens_cover_the_whole_line() {
        for (line, syntax) in [
            ("    let x: f64 = draw(1.0) + 2; // done", Syntax::Rust),
            ("    return [x for x in range(3)]  # done", Syntax::Python),
        ] {
            let chars: Vec<char> = line.chars().collect();
            let tokens = highlight_line(&chars, syntax, &mut LineState::Code);
            assert_eq!(tokens.first().map(|token| token.start), Some(0));
            assert_eq!(tokens.last().map(|token| token.end), Some(chars.len()));
            for pair in tokens.windows(2) {
                assert_eq!(pair[0].end, pair[1].start);
            }
        }
    }

    #[test]
    fn the_language_is_detected_from_the_script() {
        assert_eq!(
            Syntax::detect("use designbot::*;\nfn main() {\n    let a = 1;\n}"),
            Syntax::Rust
        );
        assert_eq!(
            Syntax::detect("import drawBot\n# page\ndef draw():\n    pass"),
            Syntax::Python
        );
    }
}
