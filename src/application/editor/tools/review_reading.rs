// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Captured fonts and presentation-only controls for selectable reading proofs.
//! Compilation owns immutable engine inputs. Neither sample edits nor proof axes mutate the font.

use crate::application::platform::reference_fonts::References;
use crate::application::workspace::Workspace;
use runebender::font::compiler::{CompiledFont, proof};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Sample {
    Glyph,
    #[default]
    Spacing,
    Alphabet,
    Custom,
}

type CompileResult = Result<Arc<CompiledFont>, String>;

#[derive(Clone, Debug)]
pub(crate) struct CompileJob(Arc<Mutex<Option<CompileResult>>>);
impl CompileJob {
    pub(crate) fn new(input: Result<proof::CompileProofInput, String>) -> Self {
        let job = Self(Arc::new(Mutex::new(None)));
        let result = job.0.clone();
        let work = move || {
            *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(
                input
                    .and_then(proof::compile)
                    .map(|snapshot| snapshot.compiled_font()),
            );
        };
        if cfg!(test) || std::env::var_os("RUNEBENDER_SCREENSHOT").is_some() {
            work();
        } else {
            std::thread::spawn(work);
        }
        job
    }
    pub(crate) fn result(&self) -> Option<Result<Arc<CompiledFont>, String>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub(crate) fn pending(&self) -> bool {
        self.result().is_none()
    }
}

#[derive(Debug)]
pub(crate) struct ReadingState {
    pub(crate) sample: Sample,
    pub(crate) sample_menu_open: bool,
    pub(crate) custom: String,
    pub(crate) size: f64,
    pub(crate) size_buf: String,
    pub(crate) axis_buffers: BTreeMap<String, String>,
    pub(crate) axes: BTreeMap<String, f64>,
    pub(crate) references: References,
    pub(crate) show_references: bool,
    pub(crate) error: Option<String>,
    proposal: Mutex<Option<(String, CompileJob)>>,
}
impl Default for ReadingState {
    fn default() -> Self {
        Self {
            sample: Sample::Spacing,
            sample_menu_open: false,
            custom: String::new(),
            size: 32.0,
            size_buf: "32".into(),
            axis_buffers: BTreeMap::new(),
            axes: BTreeMap::new(),
            references: References::default(),
            show_references: true,
            error: None,
            proposal: Mutex::new(None),
        }
    }
}
impl ReadingState {
    pub(crate) fn pending(&self) -> bool {
        self.proposal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|(_, job)| job.pending())
    }
}

pub(crate) struct ReadingFont {
    pub(crate) label: String,
    pub(crate) bytes: Option<Arc<Vec<u8>>>,
    pub(crate) status: Option<String>,
    pub(crate) variations: Option<String>,
}
impl ReadingFont {
    pub(crate) fn compiled(label: &str, result: Option<Result<Arc<CompiledFont>, String>>) -> Self {
        match result {
            Some(Ok(font)) => Self {
                label: label.into(),
                bytes: Some(font.bytes.clone()),
                status: None,
                variations: None,
            },
            Some(Err(error)) => Self {
                label: label.into(),
                bytes: None,
                status: Some(format!("Cannot compile: {error}")),
                variations: None,
            },
            None => Self {
                label: label.into(),
                bytes: None,
                status: Some("Compiling this revision…".into()),
                variations: None,
            },
        }
    }
}

impl Workspace {
    pub(crate) fn review_sample_text(&self) -> String {
        let glyph = self
            .font
            .glyphs
            .iter()
            .find(|g| g.name == self.session.glyph_name)
            .and_then(|g| g.codepoint);
        match self.review.reading.sample {
            Sample::Glyph => glyph.map(String::from).unwrap_or_default(),
            Sample::Spacing => glyph
                .map(|c| format!("{c}{c}{c} n{c}n o{c}o H{c}H O{c}O"))
                .unwrap_or_default(),
            Sample::Alphabet => self
                .font
                .glyphs
                .iter()
                .filter_map(|g| g.codepoint)
                .filter(|c| !c.is_control())
                .take(192)
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            Sample::Custom => self.review.reading.custom.clone(),
        }
    }
    pub(crate) fn select_review_sample(&mut self, sample: Sample) {
        if sample == Sample::Custom && self.review.reading.custom.is_empty() {
            self.review.reading.custom = self.review_sample_text();
        }
        self.review.reading.sample = sample;
        self.review.reading.sample_menu_open = false;
    }
    pub(crate) fn review_variations(&self) -> String {
        self.font
            .axes
            .iter()
            .enumerate()
            .map(|(index, axis)| {
                let value = self
                    .review
                    .reading
                    .axes
                    .get(&axis.tag)
                    .copied()
                    .unwrap_or_else(|| {
                        self.axis_values.get(index).copied().unwrap_or(axis.default)
                    });
                format!("'{}' {}", axis.tag, value)
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
    pub(crate) fn refresh_review_references(&mut self) {
        if self.proof_view && !self.untitled {
            self.review
                .reading
                .references
                .refresh(self.font.document_source());
        }
    }
    pub(crate) fn review_reference_weight(&self) -> f32 {
        let reading = matches!(
            self.review.mode,
            super::review::ReviewMode::Waterfall | super::review::ReviewMode::Text
        );
        let weight = self
            .font
            .axes
            .iter()
            .enumerate()
            .find(|(_, axis)| axis.tag == "wght")
            .map_or(400.0, |(index, axis)| {
                self.review
                    .reading
                    .axes
                    .get("wght")
                    .filter(|_| reading)
                    .copied()
                    .unwrap_or_else(|| self.axis_values.get(index).copied().unwrap_or(axis.default))
            });
        crate::application::view::render::px32(weight)
    }
    pub(crate) fn review_reading_fonts(&self) -> Vec<ReadingFont> {
        let working = || ReadingFont::compiled("Working", self.font.preview_font().transpose());
        let mut fonts = if let Some(task) = &self.ai.preview_task {
            let source = self.font.project.source_id(self.font.active());
            let key = format!(
                "{}:{:?}:{}:{task}:{}",
                self.document_id,
                source,
                self.font.project.document_revision(),
                self.session.glyph_name
            );
            let mut cache = self
                .review
                .reading
                .proposal
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if cache
                .as_ref()
                .is_none_or(|(old, job)| old != &key && !job.pending())
            {
                let input = self.review_proposal_input(task);
                *cache = Some((key.clone(), CompileJob::new(input)));
            }
            vec![
                working(),
                ReadingFont::compiled(
                    "Proposal",
                    cache
                        .as_ref()
                        .filter(|(actual, _)| actual == &key)
                        .and_then(|(_, job)| job.result()),
                ),
            ]
        } else {
            let mut fonts = Vec::new();
            if let Some(before) = self.review_before_font() {
                fonts.push(ReadingFont::compiled("Before", before.result()));
            }
            fonts.push(working());
            fonts
        };
        if self.review.reading.show_references {
            let weight = self.review_reference_weight();
            fonts.extend(
                self.review
                    .reading
                    .references
                    .selected(weight)
                    .into_iter()
                    .map(|reference| ReadingFont {
                        label: reference.label.clone(),
                        bytes: Some(reference.bytes.clone()),
                        status: (reference.weight(weight) != weight).then(|| {
                            format!("Weight {} (nearest supplied)", reference.weight(weight))
                        }),
                        variations: Some(reference.variations(weight)),
                    }),
            );
        }
        fonts
    }
}

/// List actual missing Unicode coverage rather than implying a fallback font is the specimen.
pub(crate) fn missing_coverage(bytes: &[u8], text: &str) -> String {
    use skrifa::MetadataProvider as _;
    let Ok(font) = skrifa::FontRef::new(bytes) else {
        return "Invalid reference font".into();
    };
    let charmap = font.charmap();
    let missing: std::collections::BTreeSet<_> = text
        .chars()
        .filter(|c| !c.is_control()
            && !matches!(u32::from(*c), 0x061c | 0x200c..=0x200f | 0x202a..=0x202e | 0x2066..=0x2069)
            && charmap.map(*c).is_none_or(|id| id.to_u32() == 0))
        .collect();
    if missing.is_empty() {
        return String::new();
    }
    format!(
        "Missing {} character(s): {}. Specimen not rendered.",
        missing.len(),
        missing
            .iter()
            .take(12)
            .map(|c| format!("U+{:04X}", u32::from(*c)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_reports_missing_characters_without_duplicates() {
        let bytes = include_bytes!("../../../../assets/fonts/VirtuaGrotesk-Regular.ttf");
        assert!(missing_coverage(bytes, "ABC 123\n").is_empty());
        let report = missing_coverage(bytes, "\u{10ffff}\u{10ffff}");
        assert!(
            report.contains("Missing 1 character(s): U+10FFFF"),
            "{report}"
        );
    }
}
