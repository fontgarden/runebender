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

/// Arguments of `spacing-scan`.
#[derive(clap::Args)]
pub(super) struct SpacingScanArgs {
    /// UFO or designspace to inspect.
    font: PathBuf,
    /// Stable source identity, required for a family.
    #[arg(long)]
    source: Option<usize>,
    /// Number of ranked candidates to return.
    #[arg(long, default_value_t = 20)]
    limit: usize,
    /// Number of ranked candidates to skip.
    #[arg(long, default_value_t = 0)]
    offset: usize,
    /// Create this new directory with report.json and proof.svg.
    #[arg(long)]
    out: Option<PathBuf>,
}

pub(super) fn spacing_scan(args: &SpacingScanArgs) -> CommandResult {
    use runebender::analysis::{spacing, spacing_sheet};
    use runebender::font::variable::SourceId;

    let project = Project::load(&args.font).map_err(CliError::usage)?;
    let source = match args.source {
        Some(id) => SourceId(id),
        None if project.document_sources().count() == 1 => {
            project.document_sources().next().expect("one source").id()
        }
        None => return Err(CliError::usage("select a family source with --source ID")),
    };
    let report =
        spacing::scan_page(&project, source, args.limit, args.offset).map_err(CliError::usage)?;
    let display = serde_json::to_string_pretty(&report).map_err(CliError::failed)? + "\n";
    if let Some(out) = &args.out {
        let svg = spacing_sheet::render(&project, source, &report).map_err(CliError::failed)?;
        std::fs::create_dir(out).map_err(CliError::usage)?;
        let write = |name: &str, contents: &str| -> std::io::Result<()> {
            use std::io::Write as _;
            let path = out.join(name);
            let mut file = std::fs::File::create_new(&path)?;
            if let Err(error) = file.write_all(contents.as_bytes()) {
                drop(file);
                let _ = std::fs::remove_file(path);
                return Err(error);
            }
            Ok(())
        };
        let result = write("proof.svg", &svg).and_then(|()| {
            write("report.json", &display).inspect_err(|_| {
                let _ = std::fs::remove_file(out.join("proof.svg"));
            })
        });
        if let Err(error) = result {
            let _ = std::fs::remove_dir(out);
            return Err(CliError::failed(error));
        }
    }
    Ok(CommandOutput::new(report, display))
}

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
