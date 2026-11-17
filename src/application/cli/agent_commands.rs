// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Single agent tool calls and their source-font adapters.
//!
//! A disk tool that is also a command calls the command function in process and returns its
//! JSON document, so the reach of a model is exactly the reach of the command line.

use super::font_commands::{
    PROOF_COLUMNS, ProposalDiscardArgs, ProposalListArgs, proposal_discard, proposal_list,
};
use super::graph_commands::{GraphRunArgs, graph_run};
use super::output::document;
use super::*;

#[derive(clap::Subcommand)]
pub(super) enum AgentAction {
    /// Open a font as an unsaved headless editor session for scripts and MCP (Unix only).
    Serve {
        /// Designspace or UFO to load into memory. This host never saves source files.
        #[arg(long)]
        font: PathBuf,
        /// Glyph to select for state checks and ordinary undo/redo.
        #[arg(long)]
        glyph: String,
        /// Maximum lifetime; keep stdin open. All unsaved edits disappear on exit.
        #[arg(long, default_value_t = 3600, value_parser = clap::value_parser!(u64).range(1..=86400))]
        duration_seconds: u64,
    },
    /// Serve a synthetic unsaved application fixture for headless live-client tests (Unix only).
    Fixture {
        /// Maximum lifetime; keep stdin open and send newline-delimited control JSON.
        #[arg(long, default_value_t = 300, value_parser = clap::value_parser!(u64).range(1..=3600))]
        duration_seconds: u64,
    },
    /// The system prompt and every tool, as JSON.
    Tools {
        /// Include versioned result contracts and effect metadata alongside legacy tool schemas.
        #[arg(long)]
        contracts: bool,
    },
    /// Run one tool call and print its result as JSON.
    Call(AgentCallArgs),
}

/// Arguments of `agent call`.
#[derive(clap::Args)]
pub(super) struct AgentCallArgs {
    /// The tool name.
    name: String,
    /// The designspace or UFO the model is working on.
    #[arg(long, required_unless_present = "session", conflicts_with = "session")]
    font: Option<PathBuf>,
    /// The live editor socket. Never reads or writes source files.
    #[arg(long)]
    session: Option<PathBuf>,
    /// The arguments, as a JSON object.
    #[arg(long, default_value = "{}")]
    args: String,
    /// Read arguments from a JSON file, or - for stdin.
    #[arg(long, conflicts_with = "args")]
    args_file: Option<PathBuf>,
    /// The font-ml binary, for propose.
    #[arg(long)]
    tool: Option<PathBuf>,
}

/// `agent`: one of its four actions, printed as JSON with or without `--json`.
pub(super) fn agent_command(action: &AgentAction) -> i32 {
    // A served session prints nothing of its own when it ends normally.
    let served = |result: Result<(), CliError>| {
        result.map_or_else(|error| emit(true, Err(error)), |()| exit::OK)
    };
    match action {
        AgentAction::Serve {
            font,
            glyph,
            duration_seconds,
        } => served(agent_serve(font, glyph, *duration_seconds)),
        AgentAction::Fixture { duration_seconds } => served(agent_fixture(*duration_seconds)),
        AgentAction::Tools { contracts } => emit(true, Ok(agent_tools(*contracts))),
        AgentAction::Call(args) => emit(true, agent_call(args)),
    }
}

/// `agent serve`: a headless live session until the deadline or the end of standard input.
fn agent_serve(font: &Path, glyph: &str, duration_seconds: u64) -> Result<(), CliError> {
    #[cfg(unix)]
    return crate::application::platform::live_host::serve_font(
        font,
        glyph,
        std::time::Duration::from_secs(duration_seconds),
    )
    .map_err(CliError::failed);
    #[cfg(not(unix))]
    {
        let _ = (font, glyph, duration_seconds);
        Err(CliError::usage("headless live sessions require Unix"))
    }
}

/// `agent fixture`: the synthetic live fixture until the deadline or the end of standard input.
fn agent_fixture(duration_seconds: u64) -> Result<(), CliError> {
    #[cfg(unix)]
    return crate::application::platform::live_fixture::serve(std::time::Duration::from_secs(
        duration_seconds,
    ))
    .map_err(CliError::failed);
    #[cfg(not(unix))]
    {
        let _ = duration_seconds;
        Err(CliError::usage("live fixtures require Unix"))
    }
}

/// `agent tools`: the system prompt and the tool list of the disk or live surface.
fn agent_tools(contracts: bool) -> CommandOutput {
    let live = std::env::var_os("RUNEBENDER_LIVE_SESSION").is_some();
    let tools = if live {
        runebender::automation::live::tools()
    } else {
        agent::tools()
    };
    let prompt = if live {
        runebender::automation::live::system_prompt(&tools)
    } else {
        agent::system_prompt(&tools)
    };
    let mut result = json!({ "ok": true, "prompt": prompt, "tools": tools });
    if contracts {
        use runebender::automation::tool_contracts::{self, ToolSurface};
        let surface = if live {
            ToolSurface::Live
        } else {
            ToolSurface::Disk
        };
        result["contracts"] = json!(
            tools
                .into_iter()
                .map(|tool| tool_contracts::describe(tool, surface))
                .collect::<Vec<_>>()
        );
    }
    CommandOutput::json(result)
}

/// The UFO a font path stands for: the UFO itself, or the first
/// master of a designspace.
fn font_master(font: &Path, master: Option<usize>) -> Result<PathBuf, String> {
    let project = Project::load(font)?;
    let sources = project.document_sources().collect::<Vec<_>>();
    let index = match master {
        Some(index) => index,
        None if sources.len() == 1 => 0,
        None => return Err("master is required for a family; call project_info first".into()),
    };
    sources
        .get(index)
        .map(|source| source.path().to_path_buf())
        .ok_or_else(|| format!("no master at index {index}"))
}

fn project_info(font: &Path) -> serde_json::Value {
    match Project::load(font) {
        Ok(project) => {
            json!({"ok": true, "project": font, "masters": project.document_sources().enumerate()
            .map(|(index, source)| json!({"index": index, "name": source.name(), "source": source.path()}))
            .collect::<Vec<_>>()})
        }
        Err(e) => json!({"ok": false, "error": e}),
    }
}

/// One glyph as the model reads it.
fn read_glyph(source: &Path, name: &str, layer: Option<&str>) -> serde_json::Value {
    let project = match Project::load(source) {
        Ok(project) => project,
        Err(error) => return json!({ "ok": false, "error": error }),
    };
    let Some(source_id) = project.document_sources().next().map(|source| source.id()) else {
        return json!({ "ok": false, "error": "the font has no source" });
    };
    runebender::analysis::glyph::read_project_glyph(&project, source_id, name, layer)
}

/// The glyph report against the whole font, so compatibility with the other
/// masters is included: the project is loaded from `font`, which may be a
/// designspace, and the selected `source` is found among its sources.
fn analyze_glyph(font: &Path, source: &Path, name: &str, layer: Option<&str>) -> serde_json::Value {
    let project = match Project::load(font).or_else(|_| Project::load(source)) {
        Ok(project) => project,
        Err(error) => return json!({ "ok": false, "error": error }),
    };
    let wanted = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let source_id = project
        .document_sources()
        .map(|candidate| candidate.id())
        .find(|id| {
            project
                .document_source_path(*id)
                .and_then(|path| std::fs::canonicalize(path).ok())
                .is_some_and(|path| path == wanted)
        })
        .or_else(|| {
            project
                .document_sources()
                .next()
                .map(|candidate| candidate.id())
        });
    let Some(source_id) = source_id else {
        return json!({ "ok": false, "error": "the font has no source" });
    };
    // `U+0035` or `5` finds `five`, as on the live surface.
    let name = project
        .resolve_glyph_name(name)
        .unwrap_or_else(|| name.to_owned());
    match runebender::analysis::report::analyze_project_glyph(&project, source_id, &name, layer) {
        Ok(value) => value,
        Err(error) => json!({ "ok": false, "error": error }),
    }
}

/// Searches the documentation folders for passages that match.
///
/// Roots: `$RUNEBENDER_DOCS` (colon-separated) and
/// `~/.runebender/docs`. Every `.md`, `.txt` and `.html` file is split
/// into paragraphs; a paragraph scores one per query word it holds,
/// and the top five come back with their file. Plain and offline: a
/// model that reads the spec beats one that remembers it.
fn docs_search(query: &str) -> serde_json::Value {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        return json!({ "ok": false, "error": "give a few words to look for" });
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(extra) = std::env::var_os("RUNEBENDER_DOCS") {
        roots.extend(std::env::split_paths(&extra));
    }
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".runebender").join("docs"));
    }
    let mut hits: Vec<(usize, String, String)> = Vec::new();
    let mut stack: Vec<PathBuf> = roots.iter().filter(|r| r.is_dir()).cloned().collect();
    let mut files = 0;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "md" | "txt" | "html" | "mdx") {
                continue;
            }
            let Ok(mut text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if ext == "html" {
                text = strip_html(&text);
            }
            files += 1;
            for para in text.split("\n\n") {
                let lower = para.to_lowercase();
                let score = words.iter().filter(|w| lower.contains(w.as_str())).count();
                if score > 0 {
                    let snippet: String = para.chars().take(600).collect();
                    hits.push((
                        score,
                        path.display().to_string(),
                        snippet.trim().to_string(),
                    ));
                }
            }
        }
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    hits.truncate(5);
    json!({
        "ok": true,
        "files_searched": files,
        "roots": roots,
        "passages": hits.iter().map(|(score, file, text)| json!({ "score": score, "file": file, "text": text })).collect::<Vec<_>>(),
        "note": if files == 0 { "No documentation found. Put .md or .txt files under ~/.runebender/docs or set RUNEBENDER_DOCS." } else { "" },
    })
}

/// HTML as text: tags dropped, block ends as paragraph breaks, the
/// few entities a spec page uses decoded. Enough for a search hit to
/// read as prose.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut tag = String::new();
    let mut skip_depth = 0_usize;
    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let lower = tag.to_lowercase();
                let name = lower
                    .trim_start_matches('/')
                    .split(|c: char| c.is_whitespace() || c == '/')
                    .next()
                    .unwrap_or("");
                if matches!(name, "script" | "style" | "nav" | "header" | "footer") {
                    if lower.starts_with('/') {
                        skip_depth = skip_depth.saturating_sub(1);
                    } else {
                        skip_depth += 1;
                    }
                } else if matches!(
                    name,
                    "p" | "div" | "tr" | "li" | "h1" | "h2" | "h3" | "h4" | "pre" | "table" | "br"
                ) && !lower.starts_with('/')
                {
                    out.push_str("\n\n");
                } else if matches!(name, "td" | "th") && !lower.starts_with('/') {
                    out.push(' ');
                }
            }
            _ if in_tag => tag.push(ch),
            _ if skip_depth > 0 => {}
            _ => out.push(ch),
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// `agent call`: one tool, mapped onto the command it already is.
///
/// Arguments that cannot be read are a usage error; a tool result that is not ok is a failure.
fn agent_call(call: &AgentCallArgs) -> CommandResult {
    let input = match &call.args_file {
        Some(path) if path.as_os_str() == "-" => std::io::read_to_string(std::io::stdin()),
        Some(path) => std::fs::read_to_string(path),
        None => Ok(call.args.clone()),
    }
    .map_err(CliError::usage)?;
    let args: serde_json::Value = serde_json::from_str(&input)
        .map_err(|error| CliError::usage(format!("arguments: {error}")))?;
    let name = &call.name;
    let result = dispatch_call(
        name,
        call.font.as_deref(),
        call.session.as_deref(),
        &args,
        call.tool.as_deref(),
    );
    let ok = result.get("ok").and_then(|v| v.as_bool()).unwrap_or(true);
    let value = json!(agent::ToolResult {
        name: name.to_string(),
        ok,
        result
    });
    Ok(CommandOutput::json(value).with_code(if ok { exit::OK } else { exit::FAILED }))
}

/// Selects the explicitly requested transport; a failed live connection never uses disk.
pub(super) fn dispatch_call(
    name: &str,
    font: Option<&Path>,
    session: Option<&Path>,
    args: &serde_json::Value,
    tool: Option<&Path>,
) -> serde_json::Value {
    let inherited = std::env::var_os("RUNEBENDER_LIVE_SESSION").map(PathBuf::from);
    if let Some(session) = session.or(inherited.as_deref()) {
        #[cfg(unix)]
        return runebender::automation::live_socket::call(
            session,
            &agent::ToolCall {
                name: name.into(),
                arguments: args.clone(),
            },
        )
        .unwrap_or_else(|e| json!({"ok": false, "error": e.to_string()}));
        #[cfg(not(unix))]
        return json!({"ok": false, "error": format!("live sockets unsupported: {}", session.display())});
    }
    match font {
        Some(font) => agent_call_value(name, font, args, tool),
        None => json!({"ok": false, "error": "font or session required"}),
    }
}

/// Runs one tool call and returns what it gave back, as JSON with an
/// `ok` field. Every tool is a command of this binary, run through it.
fn agent_call_value(
    name: &str,
    font: &Path,
    args: &serde_json::Value,
    tool: Option<&Path>,
) -> serde_json::Value {
    if !args.is_object() {
        return json!({"ok": false, "error": "arguments must be an object"});
    }
    if name == "project_info" {
        return project_info(font);
    }
    if name == "docs" {
        return docs_search(args.get("query").and_then(|v| v.as_str()).unwrap_or(""));
    }
    if name == "design_context" {
        let wanted = args.get("layer").and_then(|v| v.as_str());
        let files: Vec<_> = runebender::automation::context::load(Some(font))
            .into_iter()
            .filter(|file| wanted.is_none_or(|layer| file.layer.name() == layer))
            .collect();
        return json!({ "ok": true,
            "precedence": "A later layer wins where it disagrees with an earlier one: font over personal over default.",
            "files": runebender::automation::context::to_json(&files) });
    }
    let master = match args.get("master") {
        None => None,
        Some(value) => match value.as_u64().and_then(|v| usize::try_from(v).ok()) {
            Some(index) => Some(index),
            None => return json!({"ok": false, "error": "master must be a nonnegative integer"}),
        },
    };
    if args.get("glyphs").is_some_and(|v| {
        !v.as_array()
            .is_some_and(|a| a.iter().all(|n| n.is_string()))
    }) {
        return json!({"ok": false, "error": "glyphs must be an array of names"});
    }
    if args.get("layer").is_some_and(|v| !v.is_string()) {
        return json!({"ok": false, "error": "layer must be a string"});
    }
    let source = match font_master(font, master) {
        Ok(s) => s,
        Err(e) => return json!({ "ok": false, "error": e }),
    };
    let mut result = agent_call_source(name, font, &source, args, tool);
    if let Some(object) = result.as_object_mut() {
        object.insert("source".into(), json!(source));
        object.insert("master".into(), json!(master.unwrap_or(0)));
    }
    result
}

fn agent_call_source(
    name: &str,
    font: &Path,
    source: &Path,
    args: &serde_json::Value,
    tool: Option<&Path>,
) -> serde_json::Value {
    let glyphs = agent::glyph_list(args);
    let glyphs = (!glyphs.is_empty()).then_some(glyphs);
    let text = |key: &str| args.get(key).and_then(|v| v.as_str()).map(str::to_string);
    match name {
        "font_info" => document(info(&InfoArgs {
            source: source.to_owned(),
            glyphs: false,
        })),
        "read_glyph" => match text("glyph") {
            Some(g) => read_glyph(source, &g, args.get("layer").and_then(|v| v.as_str())),
            None => json!({ "ok": false, "error": "glyph is required" }),
        },
        "analyze_glyph" => match text("glyph") {
            Some(g) => analyze_glyph(font, source, &g, args.get("layer").and_then(|v| v.as_str())),
            None => json!({ "ok": false, "error": "glyph is required" }),
        },
        "proof" => {
            let out = std::env::temp_dir().join(format!(
                "runebender-proof-{}-{}.svg",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let mut result = document(proof(&ProofArgs {
                source: source.to_owned(),
                out: Some(out.clone()),
                glyphs,
                columns: PROOF_COLUMNS,
                layer: text("layer"),
            }));
            if result["ok"] == true
                && let Ok(svg) = std::fs::read_to_string(&out)
            {
                result["svg_content"] = json!(svg);
            }
            // The page travels in the result; the scratch file has no other reader.
            let _ = std::fs::remove_file(&out);
            result
        }
        "propose_edits" => {
            let mut batch = args.clone();
            batch
                .as_object_mut()
                .expect("object validated")
                .remove("master");
            match serde_json::from_value::<runebender::font::edit_batch::EditBatch>(batch) {
                Ok(batch) => {
                    match runebender::formats::proposal_ufo::save_proposal(source, &batch) {
                        Ok(summary) => json!({"ok": true, "proposal": summary}),
                        Err(e) => json!({"ok": false, "error": e}),
                    }
                }
                Err(e) => json!({"ok": false, "error": e.to_string()}),
            }
        }
        "propose" => match (text("task"), text("model")) {
            (Some(task), Some(model)) => {
                let model_dir = graph_run::installed(None, false)
                    .into_iter()
                    .find(|(n, _)| *n == model)
                    .map(|(_, p)| p)
                    .unwrap_or_else(|| PathBuf::from(&model));
                let mut v = document(propose(&ProposeArgs {
                    task,
                    source: source.to_owned(),
                    model: Some(model_dir),
                    glyphs,
                    tool: tool.map(Path::to_path_buf),
                    rest: Vec::new(),
                }));
                // The per-point deltas are for a tool, not a model
                // reading prose; without them the result is a few
                // hundred characters instead of thousands.
                if let Some(rows) = v
                    .get_mut("report")
                    .and_then(|r| r.get_mut("glyphs"))
                    .and_then(|g| g.as_array_mut())
                {
                    for row in rows {
                        if let Some(o) = row.as_object_mut() {
                            o.remove("deltas");
                        }
                    }
                }
                if let Some(r) = v.get_mut("report").and_then(|r| r.as_object_mut()) {
                    r.remove("deltas");
                }
                v
            }
            _ => json!({ "ok": false, "error": "task and model are required" }),
        },
        "graph_run" => match text("file") {
            // Progress has no reader here; the result holds every node.
            Some(file) => document(graph_run(
                &GraphRunArgs {
                    file: file.into(),
                    font: source.to_owned(),
                    master: None,
                    glyphs,
                    tool: tool.map(Path::to_path_buf),
                    models: None,
                    force: false,
                    no_cache: false,
                    proposal_only: true,
                },
                &mut |_| {},
            )),
            None => json!({ "ok": false, "error": "file is required" }),
        },
        "proposal_list" => document(proposal_list(&ProposalListArgs {
            source: source.to_owned(),
        })),
        "proposal_discard" => match text("task") {
            Some(task) => document(proposal_discard(&ProposalDiscardArgs {
                source: source.to_owned(),
                task,
            })),
            None => json!({ "ok": false, "error": "task is required" }),
        },
        other => json!({ "ok": false, "error": format!("no tool named {other}") }),
    }
}
