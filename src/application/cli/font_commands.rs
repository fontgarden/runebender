// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Headless font commands and the project loading they share.

use super::*;

mod features;
mod inspection;
mod outlines;
mod proposals;

pub(super) use features::{ComposeArgs, FeaturesArgs, compose, features};
pub(super) use inspection::{InfoArgs, PROOF_COLUMNS, ProofArgs, info, proof, proof_content};
pub(super) use outlines::{BoldenArgs, CollapseMetaballsArgs, bolden, collapse_metaballs};
pub(super) use proposals::{
    ProposalAction, ProposalDiscardArgs, ProposalListArgs, ProposeArgs, find_font_ml,
    proposal_command, proposal_discard, proposal_list, propose,
};

/// Arguments of `compile`.
#[derive(clap::Args)]
pub(in crate::application::cli) struct CompileArgs {
    /// Editable source to read without modifying it.
    source: PathBuf,
    /// New TTF file to write.
    #[arg(long)]
    out: PathBuf,
}

/// Compile a single-source or variable project without overwriting an existing output.
///
/// A source that does not load and an output that cannot be created are usage errors.
/// A source that loads and does not compile is a failure.
pub(in crate::application::cli) fn compile(args: &CompileArgs) -> CommandResult {
    use std::io::Write as _;
    let project = Project::load(&args.source).map_err(CliError::usage)?;
    let compiled = project.compile().map_err(CliError::failed)?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args.out)
        .map_err(CliError::usage)?;
    file.write_all(&compiled.bytes).map_err(CliError::failed)?;
    let bytes = compiled.bytes.len();
    Ok(CommandOutput::new(
        json!({"ok":true,"output":args.out,"bytes":bytes}),
        format!("Compiled {} ({bytes} bytes)\n", args.out.display()),
    ))
}

/// Load one canonical source for a headless command.
fn open_project(path: &Path) -> Result<Project, CliError> {
    let project = Project::load(path)
        .map_err(|error| CliError::usage(format!("{}: {error}", path.display())))?;
    if project.document_sources().count() != 1 {
        return Err(CliError::usage(
            "this command requires a single source; open the variable project in the editor",
        ));
    }
    Ok(project)
}

/// Load a project that must hold exactly one source, and return that source.
///
/// `command` names the command in the message for a variable project.
fn open_single_source(
    path: &Path,
    command: &str,
) -> Result<(Project, runebender::font::variable::SourceId), CliError> {
    let project = Project::load(path)
        .map_err(|error| CliError::usage(format!("{}: {error}", path.display())))?;
    let mut sources = project.document_sources();
    let selected = sources
        .next()
        .ok_or_else(|| CliError::usage("the font has no source"))?
        .id();
    if sources.next().is_some() {
        return Err(CliError::usage(format!(
            "{command} requires one UFO source, not a variable project"
        )));
    }
    drop(sources);
    Ok((project, selected))
}

/// Save a canonical Project, reporting a write failure as such.
fn save_project(project: &mut Project) -> Result<(), CliError> {
    let path = project
        .document_sources()
        .next()
        .map(|source| source.path().to_path_buf())
        .unwrap_or_default();
    project
        .save()
        .map_err(|error| CliError::failed(format!("{}: {error}", path.display())))
}

fn codepoints(codepoints: impl Iterator<Item = char>) -> Vec<String> {
    codepoints
        .map(|c| format!("U+{:04X}", u32::from(c)))
        .collect()
}
