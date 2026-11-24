// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Feature generation and composition commands.

use super::*;

/// Arguments of `features`.
#[derive(clap::Args)]
pub(in crate::application::cli) struct FeaturesArgs {
    /// The UFO.
    source: PathBuf,
    /// Write `features.generated.fea` into the UFO and add its
    /// include line to `features.fea`. Without this, print the
    /// text.
    #[arg(long)]
    write: bool,
    /// Generate the Arabic joining and ligature features of the glyph
    /// names, and the `ccmp` decompositions of empty letters into a
    /// skeleton and marks, instead of mark features. They go in the delimited block
    /// that the generator owns inside `features.fea`; `--write` writes
    /// that file, and without it the block is printed.
    #[arg(long)]
    joining: bool,
}

/// Report generated mark features or write an include beside `features.fea`.
pub(in crate::application::cli) fn features(args: &FeaturesArgs) -> CommandResult {
    use runebender::text::features;
    let (source, write) = (&args.source, args.write);
    if write
        && !source
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ufo"))
    {
        return Err(CliError::usage("features --write requires a UFO source"));
    }
    let (project, source_id) = open_single_source(source, "features")?;
    if args.joining {
        return joining_features(&project, source_id, source, write);
    }
    let generated = features::generate_project(&project, source_id)
        .ok_or_else(|| CliError::failed("the source has no canonical feature inputs"))?;
    let own_mark = features::defines_mark_features(
        project
            .document_feature_text(source_id)
            .expect("the selected source has canonical feature text"),
    );
    let written = if write {
        Some(features::write(source, &generated, true).map_err(CliError::failed)?)
    } else {
        None
    };
    let text = match &written {
        Some((path, included)) => format!(
            "{}: {} classes, {} marks, {} bases, {} stacked; include line {}\n",
            path.display(),
            generated.classes.len(),
            generated.marks,
            generated.bases,
            generated.stacked,
            if *included {
                "present"
            } else if own_mark {
                "not added: features.fea defines mark features"
            } else {
                "not added"
            }
        ),
        None => generated.fea.clone(),
    };
    let value = json!({
        "ok": true,
        "classes": generated.classes,
        "marks": generated.marks,
        "bases": generated.bases,
        "stacked": generated.stacked,
        "empty": generated.is_empty(),
        "features_fea_defines_mark": own_mark,
        "written": written.as_ref().map(|(p, _)| p),
        "included": written.as_ref().map(|(_, i)| *i),
        "fea": if write { serde_json::Value::Null } else { json!(generated.fea) },
    });
    Ok(CommandOutput::new(value, text))
}

/// Generate the Arabic joining block, and write it into `features.fea` when asked.
fn joining_features(
    project: &Project,
    source_id: runebender::font::variable::SourceId,
    source: &Path,
    write: bool,
) -> CommandResult {
    use runebender::text::arabic_features;
    let feature_text = project
        .document_feature_text(source_id)
        .expect("the selected source has canonical feature text");
    let block = arabic_features::generate_project(project, source_id, feature_text);
    let updated = block.apply_to(feature_text).map_err(CliError::failed)?;
    let written = if write {
        let path = source.join("features.fea");
        std::fs::write(&path, &updated)
            .map_err(|error| CliError::failed(format!("{}: {error}", path.display())))?;
        Some(path)
    } else {
        None
    };
    let mut text = match &written {
        Some(path) => format!("{}: {}\n", path.display(), block.summary()),
        None => block.text(),
    };
    for note in &block.notes {
        push_line!(text, "note: {note}");
    }
    let features: Vec<serde_json::Value> = block
        .features
        .iter()
        .map(|feature| json!({ "tag": feature.tag, "rules": feature.rules.len() }))
        .collect();
    let value = json!({
        "ok": true,
        "features": features,
        "rules": block.rule_count(),
        "kept": block.kept,
        "notes": block.notes,
        "empty": block.is_empty(),
        "changed": updated != feature_text,
        "written": written,
        "fea": if write { serde_json::Value::Null } else { json!(block.text()) },
    });
    Ok(CommandOutput::new(value, text))
}

/// Arguments of `compose`.
#[derive(clap::Args)]
pub(in crate::application::cli) struct ComposeArgs {
    /// The UFO.
    source: PathBuf,
    /// Which glyphs. Defaults to every glyph that has a recipe.
    #[arg(long, value_delimiter = ',')]
    glyphs: Option<Vec<String>>,
    /// Write the proposal layer. Without this, only report.
    #[arg(long)]
    write: bool,
}

/// Report composite recipes, or write them as the compose proposal.
pub(in crate::application::cli) fn compose(args: &ComposeArgs) -> CommandResult {
    let (source, write) = (&args.source, args.write);
    let (mut project, source_id) = open_single_source(source, "compose")?;
    let compose_error =
        |error: &dyn std::fmt::Display| CliError::failed(format!("compose: {error}"));
    let plan = font_compose::plan_project(&project, source_id, args.glyphs.as_deref())
        .map_err(|error| compose_error(&error))?;
    let report = if write && !plan.replacements.is_empty() {
        proposal::write_composition_project(&mut project, source_id, plan)
            .map_err(|error| compose_error(&error))?
    } else {
        plan.report
    };
    if write && report.proposal.is_some() {
        project
            .save()
            .map_err(|error| CliError::failed(format!("{}: {error}", source.display())))?;
    }
    let mut text = String::new();
    for d in &report.derived {
        let parts: Vec<String> = d
            .components
            .iter()
            .map(|(n, x, y)| format!("{n}@{x:.0},{y:.0}"))
            .collect();
        push_line!(
            text,
            "{:<28} {:<10} {}{}",
            d.glyph,
            format!("{:?}", d.recipe.source).to_lowercase(),
            parts.join(" + "),
            if d.up_to_date { "  (up to date)" } else { "" }
        );
    }
    for (g, why) in &report.skipped {
        push_line!(text, "{g:<28} skipped: {why}");
    }
    match &report.proposal {
        Some(p) => push_line!(text, "proposal {}: {} glyphs", p.task, p.glyphs.len()),
        None if write => push_line!(text, "nothing to propose"),
        None => {}
    }
    Ok(CommandOutput::new(
        json!({
            "ok": true,
            "derived": report.derived,
            "proposed": report.proposed(),
            "skipped": report.skipped,
            "proposal": report.proposal,
        }),
        text,
    ))
}
