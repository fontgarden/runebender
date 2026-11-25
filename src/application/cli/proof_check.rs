// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Check shaped words against a font project's approved glyphs and positions.
//! The editor and this command render with the same compiled-proof implementation.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use runebender::font::compiler::proof::{
    self, CompiledProofGlyph, CompiledProofRecipe, CompiledProofRendering, CompiledProofSnapshot,
};
use runebender::font::project::Project;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use skrifa::{MetadataProvider as _, Tag};

use super::output::{CliError, CommandOutput, CommandResult, exit, push_line};

mod trace;

#[derive(clap::Args)]
pub(super) struct Args {
    /// UFO or designspace to compile.
    source: PathBuf,
    /// JSON file containing the approved proof cases.
    manifest: PathBuf,
    /// New directory for numbered PNGs, in manifest order.
    #[arg(long)]
    images: Option<PathBuf>,
    /// Follow the lookup steps for one named case using `hb-shape`.
    #[arg(long, value_name = "NAME")]
    trace_case: Option<String>,
}

/// One glyph in paint order: name, UTF-8 cluster, advance, x offset and y offset.
type Placement = (String, u32, f64, f64, f64);

fn placement(glyph: CompiledProofGlyph) -> Result<Placement, CliError> {
    let name = glyph
        .glyph_name
        .ok_or_else(|| CliError::failed(format!("unnamed glyph {}", glyph.glyph_id)))?;
    Ok((
        name,
        glyph.cluster,
        glyph.x_advance,
        glyph.x_offset,
        glyph.y_offset,
    ))
}

/// Both the terminal summary and the JSON output describe this same comparison.
#[derive(Serialize)]
struct CaseReport<'a> {
    name: &'a str,
    text: &'a str,
    location: &'a BTreeMap<String, f32>,
    ok: bool,
    expected: &'a [Placement],
    actual: Vec<Placement>,
    pixels_match: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace: Option<Value>,
}

impl CaseReport<'_> {
    fn write_summary(&self, out: &mut String) {
        if self.ok {
            push_line!(out, "PASS {}", self.name);
        } else {
            push_line!(
                out,
                "FAIL {} ({})\n  expected {}\n  actual   {}",
                self.name,
                self.text,
                json!(self.expected),
                json!(self.actual)
            );
            if self.pixels_match == Some(false) {
                push_line!(out, "  rendered image differs from the reference");
            }
        }
        if let Some(trace) = &self.trace {
            push_line!(out, "{trace:#}");
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    text: String,
    #[serde(default)]
    location: BTreeMap<String, f32>,
    #[serde(default)]
    right_to_left: bool,
    #[serde(default)]
    features: BTreeMap<String, bool>,
    script: Option<String>,
    language: Option<String>,
    #[serde(default)]
    rendering: CompiledProofRendering,
    expected: Vec<Placement>,
    reference_png: Option<PathBuf>,
}

impl Case {
    fn recipe(&self, font: &skrifa::FontRef<'_>) -> Result<CompiledProofRecipe, String> {
        let axes = font.axes();
        let mut settings = Vec::new();
        for (name, &value) in &self.location {
            let axis = name
                .parse::<Tag>()
                .ok()
                .and_then(|tag| axes.get_by_tag(tag))
                .ok_or_else(|| format!("unknown axis {name:?}"))?;
            if !(axis.min_value()..=axis.max_value()).contains(&value) {
                return Err(format!("{name}={value} is outside the font's range"));
            }
            settings.push((axis.tag(), value));
        }
        let normalized = axes.location(settings);
        let recipe = CompiledProofRecipe {
            text: self.text.clone(),
            normalized_location: normalized.coords().iter().map(|v| v.to_f64()).collect(),
            right_to_left: self.right_to_left,
            features: self.features.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            script: self.script.clone(),
            language: self.language.clone(),
            rendering: self.rendering.clone(),
            target: None,
            specimen_script: None,
        };
        recipe.validate()?;
        Ok(recipe)
    }

    /// Render only for a pixel comparison or a requested image artifact.
    fn measure(
        &self,
        snapshot: &CompiledProofSnapshot,
        recipe: CompiledProofRecipe,
        manifest: &Path,
        images: Option<&Path>,
        number: usize,
    ) -> Result<(Vec<CompiledProofGlyph>, Option<bool>), CliError> {
        let failure = |e: String| CliError::failed(format!("{}: {e}", self.name));
        if images.is_none() && self.reference_png.is_none() {
            let glyphs = proof::shape_text(snapshot, &recipe).map_err(failure)?;
            return Ok((glyphs, None));
        }

        let rendered = proof::prove(snapshot, recipe).map_err(failure)?;
        if let Some(dir) = images {
            fs::write(dir.join(format!("{number}.png")), &rendered.png)
                .map_err(CliError::failed)?;
        }
        let Some(reference) = &self.reference_png else {
            return Ok((rendered.glyphs, None));
        };
        let path = manifest.with_file_name(reference);
        let bytes =
            fs::read(&path).map_err(|e| CliError::usage(format!("{}: {e}", path.display())))?;
        let pixels = |png: &[u8]| {
            image::load_from_memory_with_format(png, image::ImageFormat::Png)
                .map(|image| image.into_rgba8())
        };
        let expected =
            pixels(&bytes).map_err(|e| CliError::usage(format!("{}: {e}", path.display())))?;
        let matches = pixels(&rendered.png).map_err(CliError::failed)? == expected;
        if let Some(dir) = images {
            fs::write(dir.join(format!("{number}.reference.png")), bytes)
                .map_err(CliError::failed)?;
        }
        Ok((rendered.glyphs, Some(matches)))
    }
}

pub(super) fn check(args: &Args) -> CommandResult {
    let input = fs::read(&args.manifest).map_err(CliError::usage)?;
    let cases: Vec<Case> = serde_json::from_slice(&input).map_err(CliError::usage)?;
    if cases.is_empty() {
        return Err(CliError::usage("the proof manifest is empty"));
    }
    if let Some(name) = &args.trace_case
        && cases.iter().filter(|case| case.name == *name).count() != 1
    {
        return Err(CliError::usage(format!(
            "expected exactly one proof named {name:?}"
        )));
    }
    if !args.source.extension().is_some_and(|ext| {
        ext.eq_ignore_ascii_case("ufo") || ext.eq_ignore_ascii_case("designspace")
    }) {
        return Err(CliError::usage("check-proof requires a UFO or designspace"));
    }
    let project = Project::load(&args.source).map_err(CliError::usage)?;
    let snapshot = proof::capture(&project)
        .and_then(proof::compile)
        .map_err(CliError::failed)?;
    let compiled = snapshot.compiled_font();
    let font = skrifa::FontRef::new(&compiled.bytes).map_err(CliError::failed)?;
    if let Some(dir) = &args.images {
        fs::create_dir(dir)
            .map_err(|error| CliError::usage(format!("{}: {error}", dir.display())))?;
    }
    let mut reports = Vec::with_capacity(cases.len());
    let mut summary = String::new();
    for (index, case) in cases.iter().enumerate() {
        let recipe = case
            .recipe(&font)
            .map_err(|e| CliError::usage(format!("{}: {e}", case.name)))?;
        let (glyphs, pixels_match) = case.measure(
            &snapshot,
            recipe,
            &args.manifest,
            args.images.as_deref(),
            index + 1,
        )?;
        let actual = glyphs
            .into_iter()
            .map(placement)
            .collect::<Result<Vec<_>, _>>()?;
        let trace = if args.trace_case.as_ref() == Some(&case.name) {
            Some(trace::run(case, &snapshot).map_err(CliError::failed)?)
        } else {
            None
        };
        let report = CaseReport {
            name: &case.name,
            text: &case.text,
            location: &case.location,
            ok: actual == case.expected && pixels_match != Some(false),
            expected: &case.expected,
            actual,
            pixels_match,
            trace,
        };
        report.write_summary(&mut summary);
        reports.push(report);
    }
    let passed = reports.iter().all(|report| report.ok);
    let code = if passed { exit::OK } else { exit::FAILED };
    Ok(CommandOutput::new(json!({"ok": passed, "cases": reports}), summary).with_code(code))
}
