// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The panel language gate.
//!
//! A panel module describes its section in `recipes::rows` and nothing
//! else. This test reads the panel sources and counts the lines that
//! reach past the vocabulary to a bare label, column, or button.
//! A module written before the vocabulary has its count in [`BUDGET`].
//! A count may only go down: when you remove a raw call, lower its number
//! here; a module that is not listed may make none.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Raw view calls in each panel module that predates `recipes::rows`.
const BUDGET: &[(&str, usize)] = &[
    ("chat.rs", 22),
    ("editor_info.rs", 24),
    ("graph.rs", 20),
    ("image.rs", 9),
    ("info.rs", 3),
    ("label.rs", 2),
    ("metaballs.rs", 8),
    ("neural.rs", 6),
    ("preview.rs", 4),
    ("scripts.rs", 18),
    ("sections.rs", 53),
    ("sketch.rs", 13),
    ("tabs.rs", 49),
];

/// What a panel module may not call. Each is owned by a row.
const RAW_VIEWS: &[&str] = &[
    "label(",
    "column(",
    "row(",
    "flex_col(",
    "flex_row(",
    "sized_box(",
    "canvas(",
    "FlexSpacer",
    "text_input(",
    "button(",
    "toggle(",
    "prose(",
    "prose_muted(",
    ".boxed()",
];

/// Whether `code` calls `raw` as its own name, not as the tail of a
/// longer one such as `file_label(`.
fn calls(code: &str, raw: &str) -> bool {
    code.match_indices(raw).any(|(at, _)| {
        code[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    })
}

fn panels_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/application/view/panels")
}

#[test]
fn panels_are_built_from_the_row_vocabulary() {
    let budget: BTreeMap<&str, usize> = BUDGET.iter().copied().collect();
    let mut failures = Vec::new();
    for entry in fs::read_dir(panels_dir()).expect("panels directory") {
        let path = entry.expect("entry").path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with(".rs") || name == "mod.rs" {
            continue;
        }
        let source = fs::read_to_string(&path).expect("panel source");
        let mut found = Vec::new();
        for (number, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            for raw in RAW_VIEWS {
                if calls(code, raw) {
                    found.push(format!("{name}:{}: `{raw}`", number + 1));
                }
            }
        }
        let allowed = budget.get(name).copied().unwrap_or(0);
        if found.len() > allowed {
            failures.push(format!(
                "{name}: {} raw view calls, {allowed} allowed; build these from recipes::Row \
                 (see src/application/design/recipes/rows.rs):\n{}",
                found.len(),
                found.join("\n")
            ));
        } else if found.len() < allowed {
            failures.push(format!(
                "{name}: down to {} raw view calls from {allowed}; lower its BUDGET entry to {}",
                found.len(),
                found.len()
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

#[test]
fn budgeted_panels_still_exist() {
    for (name, _) in BUDGET {
        assert!(
            panels_dir().join(name).exists(),
            "{name} has a BUDGET entry but is gone; remove the entry"
        );
    }
}

#[test]
fn nothing_in_the_view_uppercases_text() {
    let view = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/application/view");
    let mut stack = vec![view];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("view directory") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let source = fs::read_to_string(&path).expect("view source");
                assert!(
                    !source.contains("to_uppercase") && !source.contains("to_ascii_uppercase"),
                    "{}: captions and titles are title case, never all caps",
                    path.display()
                );
            }
        }
    }
}
