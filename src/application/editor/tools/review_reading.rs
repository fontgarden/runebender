// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Captured fonts and presentation-only controls for selectable reading proofs.
//! Compilation owns immutable engine inputs. Neither sample edits nor proof axes mutate the font.

use crate::application::platform::reference_fonts::References;
use crate::application::workspace::Workspace;
use runebender::font::compiler::{CompiledFont, proof};
use runebender::text::proof_samples::{self, ProofSample};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// The text of the reading proof.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Sample {
    /// The character of the glyph alone.
    Glyph,
    /// The first proof sample of the script of the glyph.
    #[default]
    First,
    /// The proof sample with this key, or the first one when it is not for this glyph.
    Named(String),
    /// Every encoded character of the font.
    Alphabet,
    /// The text that the designer types.
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
            sample: Sample::First,
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
    /// The character of the open glyph: its own, or that of its base glyph for an unencoded
    /// form such as `beh-ar.init`, which shaping then produces in context.
    pub(crate) fn proof_character(&self) -> Option<char> {
        let glyph = self
            .font
            .glyphs
            .iter()
            .find(|g| g.name == self.session.glyph_name)?;
        glyph.codepoint.or_else(|| {
            let base = glyph.name.split('.').next()?;
            self.font
                .glyphs
                .iter()
                .find(|candidate| candidate.name == base)
                .and_then(|candidate| candidate.codepoint)
        })
    }
    /// The proof samples for the script of the open glyph, in the order of the sample file.
    pub(crate) fn glyph_proof_samples(&self) -> Vec<&ProofSample> {
        proof_samples::matching(&self.proof_samples.samples, self.proof_character()).collect()
    }
    /// The sample with `key` when it is for the open glyph, else the first sample for it.
    pub(crate) fn glyph_proof_sample(&self, key: Option<&str>) -> Option<&ProofSample> {
        let samples = self.glyph_proof_samples();
        key.and_then(|key| samples.iter().find(|sample| sample.key == key))
            .or_else(|| samples.first())
            .copied()
    }
    /// The text of a proof sample for the open glyph.
    pub(crate) fn proof_sample_text(&self, sample: &ProofSample) -> String {
        sample.text_for(self.proof_character()).unwrap_or_default()
    }
    /// The proof sample of the reading proof, when it shows one.
    pub(crate) fn review_proof_sample(&self) -> Option<&ProofSample> {
        match &self.review.reading.sample {
            Sample::First => self.glyph_proof_sample(None),
            Sample::Named(key) => self.glyph_proof_sample(Some(key)),
            Sample::Glyph | Sample::Alphabet | Sample::Custom => None,
        }
    }
    pub(crate) fn review_sample_text(&self) -> String {
        match &self.review.reading.sample {
            Sample::Glyph => self.proof_character().map(String::from).unwrap_or_default(),
            Sample::First | Sample::Named(_) => self
                .review_proof_sample()
                .map(|sample| self.proof_sample_text(sample))
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
    /// Read the reference fonts and the proof samples of the project again when they changed.
    /// The reference fonts are read only for the proof view.
    pub(crate) fn refresh_proof_files(&mut self) {
        if self.untitled {
            return;
        }
        if self.proof_view {
            self.review
                .reading
                .references
                .refresh(self.font.document_source());
        }
        self.proof_samples.refresh(self.font.document_source());
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
    fn the_reading_proof_opens_on_the_first_sample_of_the_script_of_the_glyph() {
        let mut app = Workspace::untitled().expect("blank font");
        let index = app.font.index_of("A").expect("blank font has A");
        app.open_glyph(index);
        assert_eq!(app.review_sample_text(), "AAA nAn oAo HAH OAO");
        // A Hebrew sample is not for a Latin glyph: the first Latin sample shows instead.
        app.select_review_sample(Sample::Named("hebrew-words".into()));
        assert_eq!(
            app.review_proof_sample().map(|s| s.key.as_str()),
            Some("latin-glyph-spacing")
        );
        app.select_review_sample(Sample::Named("latin-words".into()));
        assert_eq!(app.review_sample_text(), "Hamburgefontsiv");
        app.select_review_sample(Sample::Glyph);
        assert_eq!(app.review_sample_text(), "A");
    }

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
