// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Font operations from a shell.
//!
//! A thin shell over Runebender's font modules, where the work lives.
//! Conventions match `font-ml`, so the two are driven the same
//! way: `--json` on every command, and exit codes that separate a
//! usage mistake from a real failure.
//! Parsing and dispatch stay here; command behavior lives in the named child modules.
//! Each command is a function from its clap arguments to a [`output::CommandResult`],
//! and [`output::emit`] is the one place that prints a result.

use std::path::{Path, PathBuf};

mod agent_commands;
mod font_commands;
mod graph_commands;
mod mcp;
mod mcp_install;
mod output;
mod proof_check;
mod theme;

use agent_commands::{AgentAction, agent_command};
use font_commands::{
    BoldenArgs, CollapseMetaballsArgs, CompileArgs, ComposeArgs, FeaturesArgs, InfoArgs, ProofArgs,
    ProposalAction, ProposeArgs, bolden, collapse_metaballs, compile, compose, features, info,
    proof, proposal_command, propose,
};
use graph_commands::{GraphAction, graph_command};
use mcp::{McpArgs, mcp_command};
use output::{CliError, CommandOutput, CommandResult, emit, exit, push_line};
use theme::{ThemeAction, theme_command};

use clap::{Parser, Subcommand};
use runebender::automation::agent;
use runebender::font::compose as font_compose;
use runebender::font::project::Project;
use runebender::font::proposal;
use runebender::font::variable::{GlyphLayerAddress, LayerId};
use runebender::workflows::graph;
use runebender::workflows::graph_run;
use serde_json::json;

#[derive(Parser)]
#[command(
    name = "runebender",
    version,
    subcommand_precedence_over_arg = true,
    about = "Runebender font editor and headless font tools",
    long_about = "Runebender font editor and headless font tools.\n\nOpen the editor with no \
                  arguments, or pass a UFO or designspace path. Subcommands run without \
                  opening a window.\n\nUse --json for machine-readable output. \
                  Exit codes: 0 ok, 2 usage, 3 not built, 4 failed."
)]
struct Cli {
    /// Machine-readable output.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Option<Command>,
    /// A UFO or designspace to open in the editor.
    #[arg(exclusive = true)]
    font: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect, create, or validate portable editor themes.
    Theme {
        #[command(subcommand)]
        action: ThemeAction,
    },
    /// Compile a UFO or Designspace with the same Rust pipeline as live preview.
    Compile(CompileArgs),
    /// Convert all live metaballs in a UFO to cubic outlines in a new UFO.
    CollapseMetaballs(CollapseMetaballsArgs),

    /// List local editor socket paths (a crashed editor may leave a stale entry).
    Sessions,
    /// What a font is: names, metrics, counts, and any proposals
    /// waiting in it.
    Info(InfoArgs),
    /// Draw a proof sheet as SVG, with metrics per glyph.
    Proof(ProofArgs),
    /// Rank spacing proof strings for one source and optionally write a review sheet.
    SpacingScan(font_commands::SpacingScanArgs),
    /// Compile a UFO or designspace and check its project-owned shaping proofs.
    CheckProof(proof_check::Args),
    /// Render SVG path data in font units (y up) with the editor’s Gray thickness palette.
    Thickness {
        /// File containing the shaped outline as SVG path data.
        outline: PathBuf,
        /// Stroke width at the center of the color scale, in font units.
        #[arg(long)]
        reference: f64,
        /// Units per em of the SVG path coordinates.
        #[arg(long, default_value_t = 1024)]
        units_per_em: u32,
        /// Transparent PNG to write.
        #[arg(long)]
        out: PathBuf,
    },
    /// Proposals: edits offered by a tool, waiting in the UFO as
    /// `com.runebender.proposal.<task>` layers.
    Proposal {
        #[command(subcommand)]
        action: ProposalAction,
    },
    /// The harness a language model works the font through: the
    /// prompt, the tool list, and one call at a time.
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// An MCP server over stdio: the same tool list as `agent`, for
    /// a chat client that speaks the Model Context Protocol. Disk edits
    /// remain proposals; live tools can apply explicitly authorized edits.
    Mcp(McpArgs),
    /// Derive precomposed glyphs from their base and marks through
    /// anchors, into the `com.runebender.proposal.compose` layer.
    Compose(ComposeArgs),
    /// Write `mark` and `mkmk` features from the font's anchors, the
    /// ones the editor shapes with, so a compiled font positions marks
    /// the same way.
    Features(FeaturesArgs),
    /// Graph: a workflow of tools as boxes and wires, in a
    /// `<name>.graph.json` file.
    Graph {
        #[command(subcommand)]
        action: GraphAction,
    },
    /// Run a font-ml task over the UFO. font-ml is its own program; it
    /// writes what it proposes into the UFO as a proposal layer, and
    /// this reports what arrived.
    Propose(ProposeArgs),
    /// Learn how much weight a heavier master adds, from glyphs drawn
    /// in both, and report what it would do to the rest.
    Bolden(BoldenArgs),
}

fn thickness(outline: &Path, reference: f64, units_per_em: u32, out: &Path) -> CommandResult {
    use crate::application::design::theme::Palette;
    if !reference.is_finite() || reference <= 0.0 {
        return Err(CliError::usage("Reference must be positive and finite"));
    }
    let data = std::fs::read_to_string(outline).map_err(CliError::failed)?;
    let path = kurbo::BezPath::from_svg(&data).map_err(CliError::usage)?;
    let mut field =
        runebender::analysis::thickness::glyph_thickness_field(&path, f64::from(units_per_em))
            .ok_or_else(|| {
                CliError::usage("Empty or oversized thickness field, or invalid units per em")
            })?;
    field.reference = reference;
    let rgba = Palette::load("gray").thickness_rgba(&field);
    image::save_buffer_with_format(
        out,
        &rgba,
        u32::try_from(field.width).map_err(CliError::failed)?,
        u32::try_from(field.height).map_err(CliError::failed)?,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(CliError::failed)?;
    Ok(CommandOutput::json(
        json!({"origin": [field.origin.x, field.origin.y], "cell": field.cell, "reference": reference}),
    ))
}

/// Startup either opens the editor or completes a headless command.
#[derive(Debug)]
pub(crate) enum Startup {
    Editor(Option<PathBuf>),
    Exit(std::process::ExitCode),
}

/// Reject mixed editor paths and headless commands without restricting global flags.
fn parse_args<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    use clap::CommandFactory as _;
    let cli = Cli::try_parse_from(args)?;
    if cli.font.is_some() && cli.command.is_some() {
        return Err(Cli::command().error(
            clap::error::ErrorKind::ArgumentConflict,
            "Use either an editor font path or a headless subcommand",
        ));
    }
    Ok(cli)
}

/// The exit status for an exit code.
fn exit_code(code: i32) -> std::process::ExitCode {
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}

/// Parse arguments and finish headless work before any window setup.
pub(crate) fn run() -> Startup {
    let cli = match parse_args(std::env::args_os()) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return Startup::Exit(exit_code(error.exit_code()));
        }
    };
    let json = cli.json;
    let Some(command) = cli.command else {
        if json {
            let error = CliError::usage("--json requires a headless subcommand");
            return Startup::Exit(exit_code(emit(true, Err(error))));
        }
        if let Some(path) = &cli.font
            && !path.exists()
        {
            let error = CliError::usage(format!("{}: font path does not exist", path.display()));
            return Startup::Exit(exit_code(emit(false, Err(error))));
        }
        return Startup::Editor(cli.font);
    };
    let code = match &command {
        Command::Theme { action } => emit(json, theme_command(action)),
        Command::Compile(args) => emit(json, compile(args)),
        Command::CollapseMetaballs(args) => emit(json, collapse_metaballs(args)),
        Command::Sessions => emit(true, sessions()),
        Command::Info(args) => emit(json, info(args)),
        Command::SpacingScan(args) => emit(json, font_commands::spacing_scan(args)),
        Command::Proof(args) => emit(json, proof(args)),
        Command::CheckProof(args) => emit(json, proof_check::check(args)),
        Command::Thickness {
            outline,
            reference,
            units_per_em,
            out,
        } => emit(json, thickness(outline, *reference, *units_per_em, out)),
        Command::Proposal { action } => emit(json, proposal_command(action)),
        Command::Agent { action } => agent_command(action),
        Command::Mcp(args) => mcp_command(args, json),
        Command::Compose(args) => emit(json, compose(args)),
        Command::Features(args) => emit(json, features(args)),
        Command::Graph { action } => graph_command(action, json),
        Command::Propose(args) => emit(json, propose(args)),
        Command::Bolden(args) => emit(json, bolden(args)),
    };
    Startup::Exit(exit_code(code))
}

/// `sessions`: the live editor sockets on this machine, always as JSON.
fn sessions() -> CommandResult {
    #[cfg(unix)]
    return Ok(CommandOutput::json(
        json!({"ok": true, "sessions": runebender::automation::live_socket::sessions()}),
    ));
    #[cfg(not(unix))]
    Err(CliError::failed("live sockets require Unix"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cli_parses() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;

    #[test]
    fn editor_paths_and_subcommands_are_unambiguous() {
        let empty = parse_args(["runebender"]).unwrap();
        assert!(empty.command.is_none());
        assert!(empty.font.is_none());
        let font = parse_args(["runebender", "My Font.designspace"]).unwrap();
        assert_eq!(font.font.as_deref(), Some(Path::new("My Font.designspace")));
        assert!(font.command.is_none());
        let info = parse_args(["runebender", "info", "Font.ufo", "--json"]).unwrap();
        assert!(matches!(info.command, Some(Command::Info(_))));
        assert!(info.font.is_none());
        assert!(info.json);
        let info = parse_args(["runebender", "--json", "info", "Font.ufo"]).unwrap();
        assert!(matches!(info.command, Some(Command::Info(_))));
        assert!(info.json);
        assert!(parse_args(["runebender", "info"]).is_err());
        assert!(parse_args(["runebender", "Font.ufo", "info", "Other.ufo"]).is_err());
    }
}
