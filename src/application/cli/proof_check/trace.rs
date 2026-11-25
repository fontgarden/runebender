// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Inspect the exact compiled proof with the optional `HarfBuzz` command-line tracer.
//! Lookup numbers identify GSUB or GPOS entries in this binary; they do not identify FEA lines.

use std::process::Command;
use std::time::{Duration, Instant};

use runebender::font::compiler::proof::CompiledProofSnapshot;
use runebender::text::shape::script_runs;
use runebender::workflows::process::{self, ProcessCancellation, ProcessLimits, ProcessOutcome};
use serde_json::{Value, json};

use super::Case;

/// Trace the same directional script runs as the proof, retaining original UTF-8 offsets.
pub(super) fn run(case: &Case, snapshot: &CompiledProofSnapshot) -> Result<Value, String> {
    let font = snapshot.compiled_font();
    let mut runs = Vec::new();
    let started = Instant::now();
    let mut budget = 8 * 1024 * 1024;
    let direction = if case.right_to_left { "rtl" } else { "ltr" };
    let features = case
        .features
        .iter()
        .map(|(tag, &on)| format!("{tag}={}", u8::from(on)))
        .collect::<Vec<_>>()
        .join(",");
    let variations = case
        .location
        .iter()
        .map(|(tag, value)| format!("{tag}={value}"))
        .collect::<Vec<_>>()
        .join(",");

    let ranges = case
        .rendering
        .shaping_lines(&case.text)
        .flat_map(|(start, line)| {
            script_runs(line, case.right_to_left, case.script.as_deref())
                .into_iter()
                .map(move |range| (start + range.start)..(start + range.end))
        });
    for range in ranges {
        let text = &case.text[range.clone()];
        if text.is_empty() {
            runs.push(json!({"start_byte": range.start, "end_byte": range.end,
                "text": text, "events": [], "final": []}));
            continue;
        }
        // Unicode input also represents NUL and newlines without relying on argv text parsing.
        let unicodes = text
            .chars()
            .map(|ch| format!("{:X}", u32::from(ch)))
            .collect::<Vec<_>>()
            .join(",");
        let mut command = Command::new("hb-shape");
        command.args([
            "--font-file=-",
            "--trace",
            "--single-par",
            "--output-format=json",
            "--shapers=ot",
            "--utf8-clusters",
            "--cluster-level=1",
        ]);
        command
            .arg(format!("--unicodes={unicodes}"))
            .arg(format!("--direction={direction}"))
            .arg(format!(
                "--language={}",
                case.language.as_deref().unwrap_or("und")
            ));
        if let Some(script) = &case.script {
            command.arg(format!("--script={script}"));
        }
        if !features.is_empty() {
            command.arg(format!("--features={features}"));
        }
        if !variations.is_empty() {
            command.arg(format!("--variations={variations}"));
        }
        let remaining = Duration::from_secs(30)
            .checked_sub(started.elapsed())
            .ok_or("hb-shape exceeded its trace deadline")?;
        let output = process::run(
            &mut command,
            &font.bytes,
            ProcessLimits {
                deadline: remaining,
                stdin_bytes: 64 * 1024 * 1024,
                stdout_bytes: budget,
                stderr_bytes: budget,
            },
            &ProcessCancellation::default(),
            |_, _| {},
        );
        match output.outcome {
            ProcessOutcome::Exited { success: true, .. } => {}
            ProcessOutcome::SpawnFailed(error) => {
                return Err(format!(
                    "cannot start hb-shape for --trace-case: {error}. Install the HarfBuzz command-line tools."
                ));
            }
            ProcessOutcome::Exited { .. } => {
                return Err(format!(
                    "hb-shape: {}: {}",
                    output.outcome,
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            other => return Err(format!("hb-shape: {other}")),
        }
        budget = budget
            .checked_sub(output.stderr.len() + output.stdout.len())
            .ok_or("hb-shape exceeded the trace output limit")?;
        let events = parse_events(&output.stderr, range.start)?;
        let glyphs = parse_buffer(&output.stdout, range.start)?;
        runs.push(json!({
            "start_byte": range.start, "end_byte": range.end,
            "text": text, "events": events, "final": glyphs,
        }));
    }
    Ok(json!({
        "engine": "hb-shape", "font_sha256": snapshot.font_sha256(),
        "direction": direction, "runs": runs,
    }))
}

/// A `HarfBuzz` message and the state of the glyph or character buffer when it fired.
fn parse_events(stderr: &[u8], offset: usize) -> Result<Vec<Value>, String> {
    let lines = std::str::from_utf8(stderr).map_err(|e| format!("invalid hb-shape trace: {e}"))?;
    let mut events = Vec::new();
    for line in lines.lines().filter(|line| !line.trim().is_empty()) {
        let (message, buffer) = line
            .strip_prefix("trace: ")
            .and_then(|event| event.split_once("\tbuffer: "))
            .ok_or_else(|| format!("unrecognized hb-shape trace record: {line}"))?;
        events
            .push(json!({"message": message, "buffer": parse_buffer(buffer.as_bytes(), offset)?}));
    }
    Ok(events)
}

fn parse_buffer(data: &[u8], offset: usize) -> Result<Vec<Value>, String> {
    let mut buffer: Vec<Value> = serde_json::from_slice(data)
        .map_err(|error| format!("invalid hb-shape JSON buffer: {error}"))?;
    for glyph in &mut buffer {
        let cluster = glyph
            .get_mut("cl")
            .ok_or_else(|| "hb-shape buffer entry has no cluster".to_owned())?;
        let adjusted = cluster
            .as_u64()
            .and_then(|value| value.checked_add(offset as u64))
            .ok_or_else(|| "hb-shape returned an invalid cluster".to_owned())?;
        *cluster = adjusted.into();
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_intermediate_positions_and_global_byte_clusters() {
        let record = b"trace: start lookup 2 feature 'mark'\tbuffer: [{\"g\":\"gravecomb\",\"cl\":1,\"dy\":0}]\ntrace: end lookup 2 feature 'mark'\tbuffer: [{\"g\":\"gravecomb\",\"cl\":1,\"dy\":720}]\n";
        let events = parse_events(record, 4).unwrap();
        assert_eq!(events[0]["message"], "start lookup 2 feature 'mark'");
        assert_eq!(events[0]["buffer"][0]["cl"], 5);
        assert_eq!(events[1]["buffer"][0]["dy"], 720);
        assert!(parse_events(b"trace: end lookup\n", 0).is_err());
    }

    #[test]
    fn installed_harfbuzz_traces_the_midpoint_kerning_lookup() {
        match Command::new("hb-shape").arg("--version").output() {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            result => assert!(result.unwrap().status.success()),
        }
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core");
        let project = runebender::font::project::Project::load(&fixtures.join("Core.designspace"))
            .expect("load the synthetic variable font");
        use runebender::font::compiler::proof;
        let snapshot = proof::compile(proof::capture(&project).unwrap()).unwrap();
        let cases: Vec<Case> =
            serde_json::from_slice(&std::fs::read(fixtures.join("proofs.json")).unwrap()).unwrap();
        for case in &cases {
            let trace = run(case, &snapshot).unwrap();
            assert_eq!(trace["font_sha256"], snapshot.font_sha256());
            let run = &trace["runs"][0];
            let expected_kern = case.features.get("kern").copied().unwrap_or(true);
            let applied_kern = run["events"].as_array().unwrap().iter().any(|event| {
                event["message"].as_str().is_some_and(|message| {
                    message.starts_with("end lookup ") && message.contains("'kern'")
                })
            });
            assert_eq!(applied_kern, expected_kern);
            let glyphs = run["final"].as_array().unwrap();
            assert_eq!(glyphs.len(), case.expected.len());
            for (glyph, (name, cluster, advance, x_offset, y_offset)) in
                glyphs.iter().zip(&case.expected)
            {
                assert_eq!(glyph["g"].as_str(), Some(name.as_str()));
                assert_eq!(glyph["cl"], *cluster);
                assert_eq!(glyph["ax"].as_f64(), Some(*advance));
                assert_eq!(glyph["dx"].as_f64(), Some(*x_offset));
                assert_eq!(glyph["dy"].as_f64(), Some(*y_offset));
            }
        }
        let mut page = cases.into_iter().next().unwrap();
        page.text = "AV\n\nVA".into();
        page.rendering.lines = vec![
            proof::CompiledProofLine {
                pixels_per_em: 32.0,
                baseline_px: 40.0,
                align: proof::CompiledProofAlign::Start,
            };
            3
        ];
        let trace = run(&page, &snapshot).unwrap();
        assert_eq!(trace["runs"][1]["text"], "");
        assert_eq!(trace["runs"][1]["start_byte"], 3);
        assert_eq!(trace["runs"][2]["start_byte"], 4);
        let clusters: Vec<_> = trace["runs"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|run| run["final"].as_array().unwrap())
            .map(|glyph| glyph["cl"].as_u64().unwrap())
            .collect();
        assert_eq!(clusters, [0, 1, 4, 5]);
        let compiled = snapshot.compiled_font();
        let font = skrifa::FontRef::new(&compiled.bytes).unwrap();
        let actual = proof::shape_text(&snapshot, &page.recipe(&font).unwrap()).unwrap();
        assert_eq!(
            actual
                .iter()
                .map(|glyph| u64::from(glyph.cluster))
                .collect::<Vec<_>>(),
            clusters
        );
        // The non-page recipe shapes its newline codepoints in a single run.
        page.rendering.lines.clear();
        let trace = run(&page, &snapshot).unwrap();
        assert_eq!(trace["runs"].as_array().unwrap().len(), 1);
        let traced = trace["runs"][0]["final"].as_array().unwrap();
        let actual = proof::shape_text(&snapshot, &page.recipe(&font).unwrap()).unwrap();
        assert_eq!(traced.len(), actual.len());
        for (traced, glyph) in traced.iter().zip(actual) {
            assert_eq!(traced["cl"], glyph.cluster);
            assert_eq!(traced["g"].as_str(), glyph.glyph_name.as_deref());
        }
    }
}
