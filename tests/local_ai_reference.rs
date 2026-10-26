// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The local AI reference gate.
//!
//! `docs/LOCAL-AI.md` lists every tool and prompt an agent can use. Each
//! list sits between `<!-- check: NAME -->` and `<!-- end -->`, and this
//! test compares the names in backticks there, and the first number, with
//! the registries in `src/automation/`. It also checks that every
//! repository path in backticks exists. When a tool, prompt or file is
//! added, removed or renamed, update the document in the same change.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use runebender::automation::{agent, live, prompts, tool_contracts};

const REFERENCE: &str = "docs/LOCAL-AI.md";

/// Top-level directories whose paths the document may name.
const REPOSITORY_ROOTS: &[&str] = &[
    "assets/", "crates/", "docs/", "scripts/", "skills/", "src/", "tests/",
];

fn reference() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(REFERENCE);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The text between `<!-- check: name -->` and the next `<!-- end -->`.
fn region<'a>(text: &'a str, name: &str) -> &'a str {
    let start = format!("<!-- check: {name} -->");
    let after = text
        .split_once(&start)
        .unwrap_or_else(|| panic!("{REFERENCE} has no `{start}` block"))
        .1;
    after
        .split_once("<!-- end -->")
        .unwrap_or_else(|| panic!("{REFERENCE}: `{start}` has no `<!-- end -->`"))
        .0
}

/// Every span in single backticks.
fn code_spans(text: &str) -> impl Iterator<Item = &str> {
    text.split('`').skip(1).step_by(2)
}

/// The first whole number in the text, which states the list's length.
fn stated_count(text: &str) -> usize {
    text.split(|c: char| !c.is_ascii_digit())
        .find(|word| !word.is_empty())
        .and_then(|word| word.parse().ok())
        .expect("the block states how many entries it lists")
}

fn check(name: &str, published: BTreeSet<String>) {
    let text = reference();
    let block = region(&text, name);
    let listed: BTreeSet<String> = code_spans(block).map(str::to_owned).collect();
    let missing: Vec<_> = published.difference(&listed).collect();
    let extra: Vec<_> = listed.difference(&published).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{REFERENCE} `{name}` is out of date.\n  not listed: {missing:?}\n  not published: {extra:?}"
    );
    assert_eq!(
        stated_count(block),
        published.len(),
        "{REFERENCE} `{name}` states the wrong count"
    );
}

fn names(tools: Vec<agent::Tool>) -> BTreeSet<String> {
    tools.into_iter().map(|tool| tool.name).collect()
}

#[test]
fn live_tools_match_the_registry() {
    let mut tools = live::tools();
    tools.extend(tool_contracts::live_host_tools());
    check("live-tools", names(tools));
}

#[test]
fn disk_tools_match_the_registry() {
    check("disk-tools", names(agent::tools()));
}

#[test]
fn prompts_match_the_registry() {
    let list = prompts::list();
    let published = list["prompts"]
        .as_array()
        .expect("prompts/list has a prompts array")
        .iter()
        .map(|prompt| {
            prompt["name"]
                .as_str()
                .expect("a prompt has a name")
                .to_owned()
        })
        .collect();
    check("prompts", published);
}

#[test]
fn every_named_path_exists() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let missing: Vec<_> = code_spans(&reference())
        .filter(|span| REPOSITORY_ROOTS.iter().any(|dir| span.starts_with(dir)))
        .filter(|span| !span.contains(['<', '*', ' ']))
        .filter(|path| !root.join(path).exists())
        .map(str::to_owned)
        .collect();
    assert!(
        missing.is_empty(),
        "{REFERENCE} names paths that do not exist: {missing:?}"
    );
}
