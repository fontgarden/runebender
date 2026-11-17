// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Proposal listing, installation, removal, and font-ml invocation.

use super::*;

#[derive(clap::Subcommand)]
pub(in crate::application::cli) enum ProposalAction {
    /// Every proposal in the UFO, with what it changes.
    List(ProposalListArgs),
    /// Copy a proposal over the foreground and save. Each glyph is
    /// one undo step in an editor that has the font open.
    Install(ProposalInstallArgs),
    /// Drop a proposal and save.
    Discard(ProposalDiscardArgs),
}

/// Arguments of `proposal list`.
#[derive(clap::Args)]
pub(in crate::application::cli) struct ProposalListArgs {
    /// The UFO.
    pub(in crate::application::cli) source: PathBuf,
}

/// Arguments of `proposal install`.
#[derive(clap::Args)]
pub(in crate::application::cli) struct ProposalInstallArgs {
    /// The UFO.
    source: PathBuf,
    /// The task whose proposal to install.
    #[arg(long)]
    task: String,
    /// Only these glyphs. Defaults to every glyph proposed.
    #[arg(long, value_delimiter = ',')]
    glyphs: Option<Vec<String>>,
    /// Install a glyph even when it changes point structure.
    #[arg(long)]
    any_structure: bool,
}

/// Arguments of `proposal discard`.
#[derive(clap::Args)]
pub(in crate::application::cli) struct ProposalDiscardArgs {
    /// The UFO.
    pub(in crate::application::cli) source: PathBuf,
    /// The task whose proposal to drop.
    #[arg(long)]
    pub(in crate::application::cli) task: String,
}

/// `proposal`: one of its three actions.
pub(in crate::application::cli) fn proposal_command(action: &ProposalAction) -> CommandResult {
    match action {
        ProposalAction::List(args) => proposal_list(args),
        ProposalAction::Install(args) => proposal_install(args),
        ProposalAction::Discard(args) => proposal_discard(args),
    }
}

/// The error of a proposal operation.
///
/// A missing proposal, glyph or layer name is a usage error.
/// A project operation that fails after the proposal was found is a failure.
fn proposal_error(error: &proposal::ProposalError) -> CliError {
    match error {
        proposal::ProposalError::Project { .. } => CliError::failed_typed(error),
        proposal::ProposalError::NoProposal { .. }
        | proposal::ProposalError::NoSuchGlyph { .. }
        | proposal::ProposalError::Incompatible { .. }
        | proposal::ProposalError::BadLayerName { .. } => CliError::usage_typed(error),
    }
}

pub(in crate::application::cli) fn proposal_list(args: &ProposalListArgs) -> CommandResult {
    let project = open_project(&args.source)?;
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let list = proposal::list_project(&project, source_id);
    let mut text = String::new();
    if list.is_empty() {
        push_line!(text, "no proposals");
    }
    for p in &list {
        push_line!(
            text,
            "{}: {} glyphs, {} compatible, {} change structure, {} missing",
            p.task,
            p.glyphs.len(),
            p.compatible.len(),
            p.incompatible.len(),
            p.missing.len()
        );
        for (name, why) in &p.incompatible {
            push_line!(text, "  {name}: {why}");
        }
    }
    Ok(CommandOutput::new(
        json!({ "ok": true, "proposals": list }),
        text,
    ))
}

fn proposal_install(args: &ProposalInstallArgs) -> CommandResult {
    let mut project = open_project(&args.source)?;
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let done = proposal::install_project(
        &mut project,
        source_id,
        &args.task,
        args.glyphs.as_deref(),
        !args.any_structure,
    )
    .map_err(|error| proposal_error(&error))?
    .installed;
    save_project(&mut project)?;
    let mut text = String::new();
    push_line!(
        text,
        "{}: installed {} glyphs, skipped {}",
        done.task,
        done.installed.len(),
        done.skipped.len()
    );
    for (name, why) in &done.skipped {
        push_line!(text, "  {name}: {why}");
    }
    Ok(CommandOutput::new(
        json!({ "ok": true, "installed": done }),
        text,
    ))
}

pub(in crate::application::cli) fn proposal_discard(args: &ProposalDiscardArgs) -> CommandResult {
    let mut project = open_project(&args.source)?;
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let task = &args.task;
    let count = proposal::discard_project(&mut project, source_id, task)
        .map_err(|error| proposal_error(&error))?;
    save_project(&mut project)?;
    Ok(CommandOutput::new(
        json!({ "ok": true, "task": task, "discarded": count }),
        format!("{task}: dropped {count} proposed glyphs\n"),
    ))
}

/// Find font-ml from the flag, environment, or PATH.
pub(in crate::application::cli) fn find_font_ml(tool: Option<&Path>) -> Option<PathBuf> {
    if let Some(t) = tool {
        return Some(t.to_path_buf());
    }
    if let Some(t) = std::env::var_os("RUNEBENDER_FONT_ML").filter(|t| !t.is_empty()) {
        return Some(PathBuf::from(t));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("font-ml"))
        .find(|candidate| candidate.is_file())
}

/// What font-ml says it can do: each task name with whether it is
/// built. None when the tool does not answer, in which case the run
/// itself will say.
fn known_tasks(font_ml: &Path) -> Option<Vec<(String, bool)>> {
    let output = std::process::Command::new(font_ml)
        .arg("tasks")
        .arg("--json")
        .output()
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let tasks = value.get("tasks")?.as_array()?;
    Some(
        tasks
            .iter()
            .filter_map(|t| {
                Some((
                    t.get("name")?.as_str()?.to_string(),
                    t.get("implemented")?.as_bool().unwrap_or(false),
                ))
            })
            .collect(),
    )
}

/// Arguments of `propose`.
#[derive(clap::Args)]
pub(in crate::application::cli) struct ProposeArgs {
    /// The task, as `font-ml tasks` lists it.
    pub(in crate::application::cli) task: String,
    /// The UFO.
    pub(in crate::application::cli) source: PathBuf,
    /// A model directory to pass along.
    #[arg(long)]
    pub(in crate::application::cli) model: Option<PathBuf>,
    /// Glyphs to pass along. Defaults to the task's own choice.
    #[arg(long, value_delimiter = ',')]
    pub(in crate::application::cli) glyphs: Option<Vec<String>>,
    /// The font-ml binary. Defaults to `$RUNEBENDER_FONT_ML`, then
    /// `font-ml` on PATH.
    #[arg(long)]
    pub(in crate::application::cli) tool: Option<PathBuf>,
    /// Anything after `--` goes to font-ml as it is.
    #[arg(last = true)]
    pub(in crate::application::cli) rest: Vec<String>,
}

/// Runs a font-ml task and reports the proposal it left behind.
///
/// font-ml is a separate program on purpose: it carries the model
/// runtime, and this crate does not. The seam is the UFO on disk and
/// the JSON font-ml prints: the task runs with `--write`, so what it
/// predicts lands in the UFO as a proposal layer and nothing touches
/// the foreground. Its exit codes are passed through, so a caller
/// that branches on them sees the same answers either way.
pub(in crate::application::cli) fn propose(args: &ProposeArgs) -> CommandResult {
    let (task, source) = (&args.task, &args.source);
    if !source.is_dir() {
        return Err(CliError::usage(format!(
            "{}: not a UFO directory",
            source.display()
        )));
    }
    let font_ml = find_font_ml(args.tool.as_deref()).ok_or_else(|| {
        CliError::not_built(
            "font-ml is not installed: set RUNEBENDER_FONT_ML, pass --tool, or put \
             font-ml on PATH (cargo install --git https://github.com/eliheuer/font-ml)",
        )
    })?;
    // The tool says what it can do; ask it before asking it to do
    // something, so an unknown task is a usage error with the list.
    if let Some(known) = known_tasks(&font_ml) {
        if !known.iter().any(|(name, _)| name == task) {
            let names: Vec<&str> = known.iter().map(|(n, _)| n.as_str()).collect();
            return Err(CliError::usage(format!(
                "unknown task {task}; font-ml knows: {}",
                names.join(", ")
            )));
        }
        if known.iter().any(|(name, built)| name == task && !built) {
            return Err(CliError::not_built(format!(
                "{task} is a task font-ml names but has not built yet"
            )));
        }
    }
    let mut cmd = std::process::Command::new(&font_ml);
    cmd.arg("run").arg(task).arg("--source").arg(source);
    if let Some(m) = &args.model {
        cmd.arg("--model").arg(m);
    }
    for g in args.glyphs.iter().flatten() {
        cmd.arg("--glyph").arg(g);
    }
    cmd.args(&args.rest).arg("--write").arg("--json");
    let output = cmd.output().map_err(|error| {
        CliError::failed(format!("could not run {}: {error}", font_ml.display()))
    })?;
    let code = output.status.code().unwrap_or(exit::FAILED);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let tool_report: serde_json::Value = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str(line).ok())
        .unwrap_or_else(|| json!({ "raw": stdout.trim() }));
    let arrived = Project::load(source).ok().and_then(|project| {
        let source = project.source_id(0)?;
        proposal::find_project(&project, source, task).ok()
    });
    let mut text = stdout.to_string();
    match &arrived {
        Some(p) => push_line!(
            text,
            "proposal {}: {} glyphs waiting ({} compatible)",
            p.task,
            p.glyphs.len(),
            p.compatible.len()
        ),
        None => push_line!(text, "no proposal layer written for {task}"),
    }
    let diagnostics = if code == exit::OK {
        String::new()
    } else {
        String::from_utf8_lossy(&output.stderr).into_owned()
    };
    let value = json!({
        "ok": code == exit::OK,
        "tool": font_ml,
        "exit": code,
        "report": tool_report,
        "proposal": arrived,
    });
    Ok(CommandOutput::new(value, text)
        .with_diagnostics(diagnostics)
        .with_code(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project operation that fails after the proposal was found is not the caller's mistake.
    #[test]
    fn a_failed_project_operation_is_a_failure_and_a_missing_proposal_is_usage() {
        let failed = proposal_error(&proposal::ProposalError::Project {
            reason: "the layer disappeared".into(),
        });
        assert_eq!(failed.code(), exit::FAILED);
        assert_eq!(failed.document()["error"]["kind"], "project");
        let missing = proposal_error(&proposal::ProposalError::NoProposal {
            task: "bolden".into(),
        });
        assert_eq!(missing.code(), exit::USAGE);
    }
}
