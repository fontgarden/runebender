// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Proof section of the editor panel: the open glyph in a proof sample of its script,
//! shaped from the live outline, so that it follows each edit before the font is saved.
//! The samples are those of the proof view: the defaults and the project's sample file.

use std::sync::Arc;

use crate::application::design::recipes::{self, Group, Row};
use crate::application::view::canvas::specimen;
use crate::application::workspace::Workspace;
use xilem::WidgetView;

/// The collapse key and the title of the section.
pub(crate) const KEY: &str = "Proof";

pub(crate) fn section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let samples = app.glyph_proof_samples();
    let shown = app.glyph_proof_sample(app.panel_sample.as_deref());
    let mut rows = Vec::new();
    if let Some(sample) = shown {
        let text = app.proof_sample_text(sample);
        match specimen::live_block(app, &text) {
            Some((block, ascender, descender)) => rows.push(Row::Specimen {
                block,
                ascender,
                descender,
            }),
            None => rows.push(Row::Meta("The sample has no drawn glyphs.".into())),
        }
        let missing = missing_characters(app, &text);
        if !missing.is_empty() {
            rows.push(Row::Meta(format!("Not in the font: {missing}")));
        }
    }
    let mut groups = vec![Group::new("", rows)];
    if samples.len() > 1 {
        let keys: Vec<String> = samples.iter().map(|sample| sample.key.clone()).collect();
        groups.push(Group::new(
            "Sample",
            vec![Row::Chips {
                items: samples.iter().map(|sample| sample.label.clone()).collect(),
                chosen: shown.and_then(|shown| keys.iter().position(|key| *key == shown.key)),
                on_pick: Arc::new(move |app: &mut Workspace, index| {
                    if let Some(key) = keys.get(index) {
                        app.panel_sample = Some(key.clone());
                    }
                }),
            }],
        ));
    }
    if let Some(error) = &app.proof_samples.error {
        groups.push(Group::new("", vec![Row::Prose(error.clone())]));
    }
    recipes::section_of(app, KEY, KEY, groups)
}

/// The characters of `text` that no glyph of the font has, at most eight, as `U+05B8` codes.
fn missing_characters(app: &Workspace, text: &str) -> String {
    let mut missing: Vec<char> = text
        .chars()
        .filter(|c| !c.is_whitespace() && app.font.glyphs.iter().all(|g| g.codepoint != Some(*c)))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    missing
        .iter()
        .take(8)
        .map(|c| format!("U+{:04X}", u32::from(*c)))
        .collect::<Vec<_>>()
        .join(", ")
}
