// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The architecture map gate.
//!
//! `docs/ARCHITECTURE.md` tells a contributor where each kind of change starts. This test keeps
//! the document and the source tree in step in two ways.
//!
//! - Every repository path in backticks, and every relative link, must exist.
//! - Every directory under `src` must be named in backticks, as `src/font/model`.
//! - An item in backticks before "in" and a source path, as "`poll` in
//!   `src/application/view/render.rs`", must be declared in that file or directory.
//!
//! When a file or a directory is added, moved or removed, update the document in the same
//! change. Each failure names the line of the document or the directory that is out of date.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAP: &str = "docs/ARCHITECTURE.md";

/// Top-level entries whose paths the document may name.
const REPOSITORY_ROOTS: &[&str] = &[
    ".github/",
    "assets/",
    "benches/",
    "crates/",
    "docs/",
    "examples/",
    "scripts/",
    "skills/",
    "src/",
    "tests/",
    "web/",
];

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn map() -> String {
    let path = repository().join(MAP);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Every span in single backticks.
fn code_spans(text: &str) -> impl Iterator<Item = &str> {
    text.split('`').skip(1).step_by(2)
}

/// Every span in backticks that is a repository path, without a trailing slash.
///
/// A span with a placeholder, a wildcard or a space is a pattern, not a path.
fn named_paths(line: &str) -> impl Iterator<Item = &str> {
    code_spans(line)
        .filter(|span| REPOSITORY_ROOTS.iter().any(|root| span.starts_with(root)))
        .filter(|span| !span.contains(['<', '*', ' ', '{']))
        .map(|span| span.trim_end_matches('/'))
}

/// The target of every Markdown link in `line` that points into the repository.
fn relative_links(line: &str) -> impl Iterator<Item = &str> {
    line.split("](")
        .skip(1)
        .filter_map(|rest| rest.split_once(')'))
        .map(|(target, _)| target.split_once('#').map_or(target, |(path, _)| path))
        .filter(|target| !target.is_empty() && !target.contains("://"))
}

fn directories(directory: &Path, output: &mut Vec<PathBuf>) {
    let entries =
        fs::read_dir(directory).unwrap_or_else(|error| panic!("{}: {error}", directory.display()));
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            output.push(path.clone());
            directories(&path, output);
        }
    }
}

#[test]
fn every_named_path_exists() {
    let text = map();
    let mut stale = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        for path in named_paths(line) {
            if !repository().join(path).exists() {
                stale.push(format!(
                    "{MAP}:{number} names `{path}`, which does not exist.\n    {line}"
                ));
            }
        }
        for link in relative_links(line) {
            if !repository().join("docs").join(link).exists() {
                stale.push(format!(
                    "{MAP}:{number} links to `{link}`, which does not exist.\n    {line}"
                ));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "{MAP} has {} stale reference(s). A file moved or was removed. Find its new place and \
         correct each line:\n\n{}",
        stale.len(),
        stale.join("\n")
    );
}

#[test]
fn every_source_directory_is_named() {
    let text = map();
    let named: BTreeSet<&str> = text.lines().flat_map(named_paths).collect();
    let mut found = Vec::new();
    directories(&repository().join("src"), &mut found);
    let mut missing: Vec<String> = found
        .iter()
        .map(|path| {
            path.strip_prefix(repository())
                .expect("a directory under the repository")
                .to_string_lossy()
                .replace('\\', "/")
        })
        .filter(|path| !named.contains(path.as_str()))
        .collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "{MAP} does not name {} source directories. Add a row for each to the directory table, \
         with what the directory holds:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
}

/// The keywords that declare an item a row can name.
const DECLARATIONS: &[&str] = &[
    "fn",
    "struct",
    "enum",
    "const",
    "static",
    "trait",
    "type",
    "mod",
    "impl",
    "macro_rules!",
];

/// True when `span` is a Rust item path such as `poll` or `Project::load`.
fn is_item(span: &str) -> bool {
    !span.is_empty()
        && span
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_' || character == ':')
        && !span.starts_with(':')
}

/// Every item in `line` that names the source path after it, with that path.
///
/// The forms are "`a` in `path`" and "`a` and `b` in `path`", with any words before "in".
fn located_items(line: &str) -> Vec<(&str, &str)> {
    let parts: Vec<&str> = line.split('`').collect();
    let mut located = Vec::new();
    let mut pending: Vec<&str> = Vec::new();
    let mut index = 1;
    while index < parts.len() {
        let span = parts[index];
        let gap = parts.get(index + 1).map_or("", |gap| gap.trim());
        let source = span.starts_with("src/");
        if source && index >= 2 {
            let before = parts[index - 1].trim();
            if before == "in" || before.ends_with(" in") {
                located.extend(pending.iter().map(|item| (*item, span)));
            }
            pending.clear();
        } else if is_item(span) && !gap.contains('|') {
            pending.push(span);
            if !(gap == "and" || gap == "," || gap == "in" || gap.ends_with(" in")) {
                pending.clear();
            }
        } else {
            pending.clear();
        }
        index += 2;
    }
    located
}

/// Every Rust file at `path`: the file itself, or each file below the directory.
fn rust_files(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let mut directories_found = vec![path.to_path_buf()];
    directories(path, &mut directories_found);
    directories_found
        .iter()
        .flat_map(|directory| fs::read_dir(directory).into_iter().flatten())
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|file| file.extension().is_some_and(|extension| extension == "rs"))
        .collect()
}

/// True when `source` declares `name` with one of [`DECLARATIONS`].
fn declares(source: &str, name: &str) -> bool {
    let words: Vec<&str> = source
        .split(|character: char| !(character.is_alphanumeric() || "_!".contains(character)))
        .filter(|word| !word.is_empty())
        .collect();
    words
        .windows(2)
        .any(|pair| pair[1] == name && DECLARATIONS.contains(&pair[0]))
}

#[test]
fn every_located_item_is_declared() {
    let text = map();
    let mut stale = Vec::new();
    let mut checked = 0;
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        for (item, path) in located_items(line) {
            let name = item.rsplit("::").next().expect("a path has a last segment");
            let files = rust_files(&repository().join(path));
            if files.is_empty() {
                // `every_named_path_exists` reports the path.
                continue;
            }
            checked += 1;
            let found = files
                .iter()
                .any(|file| fs::read_to_string(file).is_ok_and(|source| declares(&source, name)));
            if !found {
                stale.push(format!(
                    "{MAP}:{number} says `{item}` is in `{path}`, but no file there declares \
                     `{name}`.\n    {line}"
                ));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "{MAP} has {} stale item name(s). An item was renamed or moved. Find its new name or \
         place and correct each line:\n\n{}",
        stale.len(),
        stale.join("\n")
    );
    assert!(checked > 10, "{MAP} locates only {checked} items");
}

#[test]
fn located_items_are_read() {
    assert_eq!(
        located_items("| x | `poll` in `src/a.rs` | `A::b` and `c` in `src/d.rs`, `e` for `f` |"),
        vec![
            ("poll", "src/a.rs"),
            ("A::b", "src/d.rs"),
            ("c", "src/d.rs")
        ]
    );
    assert_eq!(
        located_items("The methods of `Draft` in `src/font/x` | `src/y.rs` for `Z`"),
        vec![("Draft", "src/font/x")]
    );
}

#[test]
fn the_map_names_source_paths() {
    // A change to the span syntax must not turn the two gates above into empty loops.
    let text = map();
    let count = text.lines().flat_map(named_paths).count();
    assert!(count > 40, "{MAP} names only {count} repository paths");
}
