// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Saved graph commands and external node discovery.

use runebender::workflows::process::{self, ProcessCancellation, ProcessLimits, ProcessOutcome};

use super::font_commands::find_font_ml;
use super::*;

#[derive(clap::Subcommand)]
pub(super) enum GraphAction {
    /// Read a file and report every problem. Exit 0 when it would run.
    Check {
        /// The `.graph.json` file.
        file: PathBuf,
        /// The font-ml binary, for the tasks it declares. Defaults to
        /// `$RUNEBENDER_FONT_ML`, then `font-ml` on PATH.
        #[arg(long)]
        tool: Option<PathBuf>,
    },
    /// Every node type: the engine's, plus what font-ml declares.
    Types {
        /// The font-ml binary.
        #[arg(long)]
        tool: Option<PathBuf>,
    },
    /// Run a file against a font: every node in order, skipping what
    /// has not changed since the last run. Progress on stderr, one
    /// JSON report on stdout with --json.
    Run(GraphRunArgs),
    /// The JSON Schema for the file.
    Schema,
}

/// Arguments of `graph run`.
#[derive(clap::Args)]
pub(super) struct GraphRunArgs {
    /// The `.graph.json` file.
    pub(super) file: PathBuf,
    /// The designspace or UFO the Font node stands for.
    #[arg(long)]
    pub(super) font: PathBuf,
    /// The master the Font node gives, by style name. The first
    /// when not given.
    #[arg(long)]
    pub(super) master: Option<String>,
    /// The glyphs the Font node gives. Every drawn glyph when not
    /// given.
    #[arg(long, value_delimiter = ',')]
    pub(super) glyphs: Option<Vec<String>>,
    /// The font-ml binary.
    #[arg(long)]
    pub(super) tool: Option<PathBuf>,
    /// Where models live. Defaults to `$RUNEBENDER_MODELS`, then
    /// `~/Runebender/models`.
    #[arg(long)]
    pub(super) models: Option<PathBuf>,
    /// Run every node, cached or not.
    #[arg(long)]
    pub(super) force: bool,
    /// Keep no cache and read none.
    #[arg(long)]
    pub(super) no_cache: bool,
    /// Reject workflow nodes that can write foreground data.
    #[arg(long)]
    pub(super) proposal_only: bool,
}

/// `graph`: one of its four actions, printed.
///
/// `graph schema` prints JSON with or without `--json`.
pub(super) fn graph_command(action: &GraphAction, json: bool) -> i32 {
    match action {
        GraphAction::Check { file, tool } => emit(json, graph_check(file, tool.as_deref())),
        GraphAction::Types { tool } => emit(json, graph_types(tool.as_deref())),
        GraphAction::Run(args) => emit(json, graph_run(args, &mut print_run_event)),
        // One line, like every other JSON this prints, so a
        // caller reads the last line and has it all.
        GraphAction::Schema => emit(true, Ok(CommandOutput::json(graph::NodeGraph::schema()))),
    }
}

fn node_registry(tool: Option<&Path>) -> (graph::Registry, Option<String>) {
    let mut registry = graph::Registry::core();
    let Some(font_ml) = find_font_ml(tool) else {
        return (registry, None);
    };
    let output = process::run(
        std::process::Command::new(&font_ml).args(["tasks", "--json"]),
        &[],
        ProcessLimits {
            deadline: std::time::Duration::from_secs(2),
            ..ProcessLimits::default()
        },
        &ProcessCancellation::default(),
        |_, _| {},
    );
    if !matches!(output.outcome, ProcessOutcome::Exited { success: true, .. }) {
        return (registry, None);
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return (registry, None);
    };
    registry.add_tool("font-ml", &value);
    (registry, Some(font_ml.display().to_string()))
}

fn graph_types(tool: Option<&Path>) -> CommandResult {
    let (registry, font_ml) = node_registry(tool);
    let mut text = String::new();
    for t in &registry.types {
        let ins: Vec<String> = t
            .inputs
            .iter()
            .map(|p| format!("{}:{}", p.name, p.kind))
            .collect();
        let outs: Vec<String> = t
            .outputs
            .iter()
            .map(|p| format!("{}:{}", p.name, p.kind))
            .collect();
        push_line!(
            text,
            "{:<18} {:<10} ({}) -> ({}){}",
            t.name,
            t.title,
            ins.join(", "),
            outs.join(", "),
            if t.implemented { "" } else { "  [not built]" }
        );
    }
    let diagnostics = if font_ml.is_none() {
        "font-ml not found: only core types listed\n".to_owned()
    } else {
        String::new()
    };
    Ok(CommandOutput::new(
        json!({ "ok": true, "tool": font_ml, "types": registry.types }),
        text,
    )
    .with_diagnostics(diagnostics))
}

/// Problems are printed on standard error, and a file with problems exits with a usage code.
fn graph_check(file: &Path, tool: Option<&Path>) -> CommandResult {
    let graph = graph::NodeGraph::load(file).map_err(CliError::usage)?;
    let (registry, font_ml) = node_registry(tool);
    let problems = graph.validate(&registry);
    let order = graph.order().ok();
    let mut diagnostics = String::new();
    for p in &problems {
        push_line!(diagnostics, "{p}");
    }
    let mut text = String::new();
    if problems.is_empty() {
        let order: Vec<String> = order
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(|id| graph.node(*id))
            .map(|n| format!("{}:{}", n.id, n.type_name))
            .collect();
        push_line!(
            text,
            "{} nodes, {} links, runs: {}",
            graph.nodes.len(),
            graph.links.len(),
            order.join(" ")
        );
    }
    let code = if problems.is_empty() {
        exit::OK
    } else {
        exit::USAGE
    };
    let value = json!({
        "ok": problems.is_empty(),
        "file": file,
        "tool": font_ml,
        "nodes": graph.nodes.len(),
        "links": graph.links.len(),
        "order": order,
        "problems": problems,
    });
    Ok(CommandOutput::new(value, text)
        .with_diagnostics(diagnostics)
        .with_code(code))
}

/// Progress of `graph run` on standard error, as each node starts and ends.
fn print_run_event(event: graph_run::Event) {
    match event {
        graph_run::Event::Start {
            id,
            type_name,
            index,
            total,
        } => eprintln!("node {index}/{total} {id} {type_name}"),
        graph_run::Event::Progress {
            done, total, label, ..
        } => eprintln!("progress {done}/{total} {label}"),
        graph_run::Event::End {
            id,
            status,
            seconds,
            error,
        } => match error {
            Some(e) => eprintln!("node {id} failed: {e}"),
            None => eprintln!("node {id} {status:?} {seconds:.1}s"),
        },
    }
}

/// Run a saved graph; `on_event` receives progress while it runs.
///
/// A run that finishes with a failed node exits with the failure code.
pub(super) fn graph_run(
    args: &GraphRunArgs,
    on_event: &mut dyn FnMut(graph_run::Event),
) -> CommandResult {
    let (file, font) = (&args.file, &args.font);
    let graph = graph::NodeGraph::load(file).map_err(CliError::usage)?;
    if !font.exists() {
        return Err(CliError::usage(format!("{}: not found", font.display())));
    }
    let (registry, _) = node_registry(args.tool.as_deref());
    if args.proposal_only {
        graph_run::validate_proposal_workflow(&graph).map_err(CliError::usage)?;
    }
    let problems = graph.validate(&registry);
    if !problems.is_empty() {
        let text: Vec<String> = problems.iter().map(ToString::to_string).collect();
        return Err(CliError::usage(format!(
            "{} will not run:\n{}",
            file.display(),
            text.join("\n")
        )));
    }
    let mut tools = std::collections::BTreeMap::new();
    if let Some(font_ml) = find_font_ml(args.tool.as_deref()) {
        tools.insert("font-ml".to_string(), font_ml);
    }
    let mut ctx = graph_run::RunContext {
        font,
        master: args.master.as_deref(),
        glyphs: args.glyphs.clone().unwrap_or_default(),
        tools,
        models_dir: args.models.clone().or_else(graph_run::default_models_dir),
        device: None,
        force: args.force,
        cache: (!args.no_cache).then(|| graph_run::cache_path(file)),
        cancellation: ProcessCancellation::default(),
        process_limits: ProcessLimits::default(),
        on_event,
    };
    let report = graph_run::run(&graph, &registry, &mut ctx);
    let mut text = String::new();
    for n in &report.nodes {
        let note = match n.status {
            graph_run::Status::Failed => n
                .report
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("failed")
                .to_string(),
            _ => n
                .outputs
                .iter()
                .map(|(k, v)| match v {
                    graph_run::RunValue::Layer { name, .. } => format!("{k}={name}"),
                    graph_run::RunValue::Rows { rows } => format!("{k}={} rows", rows.len()),
                    graph_run::RunValue::Path { path } => format!("{k}={}", path.display()),
                    _ => String::new(),
                })
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
        };
        push_line!(
            text,
            "{:<3} {:<18} {:<8} {note}",
            n.id,
            n.type_name,
            format!("{:?}", n.status).to_lowercase()
        );
    }
    let code = if report.ok { exit::OK } else { exit::FAILED };
    let value = serde_json::to_value(
        runebender::automation::agent_graph::results::DiskGraphRunResult {
            ok: report.ok,
            file: file.to_owned(),
            font: font.to_owned(),
            nodes: report.nodes,
        },
    )
    .expect("typed disk graph result serializes");
    Ok(CommandOutput::new(value, text).with_code(code))
}
