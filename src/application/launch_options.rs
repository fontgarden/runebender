// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Launch options: the `RUNEBENDER_*` environment variables of the editor, parsed once.
//!
//! [`LaunchOptions::from_env`] is the only place where the editor reads these variables.
//! The rest of the application reads the typed fields of [`LaunchOptions::current`].
//! [`VARIABLES`] names each variable with its help text, and a test compares that table with the
//! table in `docs/UI-DESIGN.md`.

use crate::application::editor::tools::review::ReviewMode;
use crate::application::editor::tools::review_reading::Sample;
use crate::application::pieces::PreviewView;
use crate::application::widgets::frame_stats;
use crate::application::workspace::Workspace;
use std::ffi::OsString;
use std::sync::{Arc, OnceLock};

/// One environment variable that the editor reads at launch.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "only the documentation test reads the value syntax and the help text"
    )
)]
pub(crate) struct Variable {
    /// The name of the variable.
    pub(crate) name: &'static str,
    /// The value syntax that the documentation shows after the name.
    pub(crate) value: &'static str,
    /// What the variable does, as one line of documentation.
    pub(crate) help: &'static str,
}

const fn variable(name: &'static str, value: &'static str, help: &'static str) -> Variable {
    Variable { name, value, help }
}

/// Every variable that [`LaunchOptions`] holds, in the order of the documentation.
pub(crate) const VARIABLES: &[Variable] = &[
    variable(
        "RUNEBENDER_SCREENSHOT",
        "<path>",
        "Render one frame to this PNG file and exit without a window.",
    ),
    variable(
        "RUNEBENDER_SIZE",
        "1100x720",
        "Logical size of the capture; the default is 1100 by 720.",
    ),
    variable(
        "RUNEBENDER_SCALE",
        "2",
        "Device-pixel scale of the capture at the same logical layout.",
    ),
    variable(
        "RUNEBENDER_GLYPH",
        "<name>",
        "Open this glyph in the editor.",
    ),
    variable(
        "RUNEBENDER_SELECTED",
        "<name>",
        "In a capture, select this glyph in the grid.",
    ),
    variable(
        "RUNEBENDER_SELECTALL",
        "1",
        "Select all points in the open glyph.",
    ),
    variable(
        "RUNEBENDER_EXPAND",
        "<key>[,<key>]",
        "In a capture, expand the panel sections with these collapse keys.",
    ),
    variable(
        "RUNEBENDER_COMPONENT",
        "<n>",
        "Open the open glyph's component at index `n` in place, inside its composite.",
    ),
    variable(
        "RUNEBENDER_PREVIEW_TEXT",
        "<text>",
        "Set the text of the proof strip.",
    ),
    variable(
        "RUNEBENDER_PREVIEW",
        "<pieces|model>",
        "In a capture, show this view of the proof strip; `model` waits for the first drawing of the trained font.",
    ),
    variable(
        "RUNEBENDER_NEURAL_STATUS",
        "<text>",
        "In a capture, show this training status in the Neural section.",
    ),
    variable(
        "RUNEBENDER_PROOF_VIEW",
        "<1|toggle|before|waterfall|text>",
        "Open the proof view: `1` opens Compare, `toggle` and `before` open Toggle on the right-hand or left-hand revision, `waterfall` and `text` open those pages.",
    ),
    variable(
        "RUNEBENDER_PROOF_DARK",
        "1",
        "Open the proof page in dark mode.",
    ),
    variable(
        "RUNEBENDER_PROOF_TEXT",
        "<text>",
        "Set a custom sample of at most 4096 characters for the reading proof.",
    ),
    variable(
        "RUNEBENDER_PROOF_MENU",
        "1",
        "Open the sample menu of the reading proof.",
    ),
    variable(
        "RUNEBENDER_PROOF_SAMPLE",
        "1",
        "With `RUNEBENDER_GLYPH`, create and select an in-memory sample proposal for the proof view.",
    ),
    variable(
        "RUNEBENDER_LABEL_SAMPLE",
        "<n>",
        "With `RUNEBENDER_GLYPH`, select this sample of the label tool.",
    ),
    variable(
        "RUNEBENDER_GRAPH_SCRIPT",
        "<index>",
        "In a capture of the Graph view, add a code node with this example script.",
    ),
    variable(
        "RUNEBENDER_GRAPH_EXPANDED",
        "1",
        "In a capture of the Graph view, show the first proof image expanded.",
    ),
    variable(
        "RUNEBENDER_FRAME_STATS",
        "1",
        "In a window, render every frame and report the frame cadence on stderr.",
    ),
];

/// Where one headless capture goes and how large it is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CaptureOptions {
    /// The PNG file to write.
    pub(crate) path: String,
    /// The logical size of the frame.
    pub(crate) size: (u32, u32),
    /// The device-pixel scale at that logical size.
    pub(crate) scale: f64,
}

/// The proof view that `RUNEBENDER_PROOF_VIEW` opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProofView {
    /// `1`: both revisions beside one another.
    Compare,
    /// `toggle`: one revision at a time, on the right-hand revision.
    Toggle,
    /// `before`: one revision at a time, on the left-hand revision.
    Before,
    /// `waterfall`: the sample at descending sizes.
    Waterfall,
    /// `text`: the reading proof.
    Text,
}

/// The values of the `RUNEBENDER_*` variables at launch.
pub(crate) struct LaunchOptions {
    /// Render one frame to a file and exit, instead of opening a window.
    pub(crate) capture: Option<CaptureOptions>,
    /// Render every frame of the window and report the cadence on stderr.
    pub(crate) frame_stats: bool,
    /// The glyph that the editor opens at launch.
    pub(crate) glyph: Option<String>,
    /// Create a sample proposal for the open glyph, so the proof view has one to compare.
    #[cfg(unix)]
    pub(crate) proof_sample: bool,
    /// The index of the component of the open glyph to select and to open in place.
    pub(crate) component: Option<usize>,
    /// The sample of the label tool to select in the open glyph.
    pub(crate) label_sample: Option<usize>,
    /// Select all points of the open glyph.
    pub(crate) select_all: bool,
    /// The proof view to open.
    pub(crate) proof_view: Option<ProofView>,
    /// Show the proof page in dark mode.
    pub(crate) proof_dark: bool,
    /// The custom sample of the reading proof.
    pub(crate) proof_text: Option<String>,
    /// Open the sample menu of the reading proof.
    pub(crate) proof_menu: bool,
    /// The text of the proof strip.
    pub(crate) preview_text: Option<String>,
    /// In a capture, the glyph to select in the grid.
    pub(crate) selected: Option<String>,
    /// In a capture, the view of the proof strip.
    pub(crate) preview: Option<PreviewView>,
    /// In a capture, the training status of the Neural section.
    pub(crate) neural_status: Option<String>,
    /// In a capture, the collapse keys of the panel sections to expand.
    pub(crate) expand: Vec<String>,
    /// In a capture of the Graph view, the example script of a new code node.
    #[cfg(unix)]
    pub(crate) graph_script: Option<usize>,
    /// In a capture of the Graph view, show the first proof image expanded.
    #[cfg(unix)]
    pub(crate) graph_expanded: bool,
}

/// A source of variable values: the process environment, or a table in a test.
struct Variables<'a>(&'a dyn Fn(&str) -> Option<OsString>);

impl Variables<'_> {
    /// The value as `std::env::var_os` returns it.
    fn os(&self, name: &str) -> Option<OsString> {
        debug_assert!(
            VARIABLES.iter().any(|variable| variable.name == name),
            "{name} has no row in VARIABLES"
        );
        (self.0)(name)
    }

    /// The value as `std::env::var` returns it: a value that is not Unicode counts as unset.
    fn text(&self, name: &str) -> Option<String> {
        self.os(name)?.into_string().ok()
    }

    /// Whether the variable is set to any value, Unicode or not.
    fn is_set(&self, name: &str) -> bool {
        self.os(name).is_some()
    }
}

impl LaunchOptions {
    /// The options of this process, read from the environment on the first call.
    pub(crate) fn current() -> &'static Self {
        static CURRENT: OnceLock<LaunchOptions> = OnceLock::new();
        CURRENT.get_or_init(Self::from_env)
    }

    /// Parse the options from the process environment.
    pub(crate) fn from_env() -> Self {
        Self::from_variables(&Variables(&|name| std::env::var_os(name)))
    }

    fn from_variables(variables: &Variables<'_>) -> Self {
        let capture = variables
            .text("RUNEBENDER_SCREENSHOT")
            .map(|path| CaptureOptions {
                path,
                size: variables
                    .text("RUNEBENDER_SIZE")
                    .and_then(|spec| {
                        let (width, height) = spec.split_once('x')?;
                        Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
                    })
                    .unwrap_or((1100, 720)),
                scale: variables
                    .text("RUNEBENDER_SCALE")
                    .and_then(|value| value.parse::<f64>().ok())
                    .filter(|value| value.is_finite() && *value > 0.0)
                    .unwrap_or(1.0),
            });
        Self {
            capture,
            frame_stats: variables.is_set(frame_stats::ENV),
            glyph: variables.text("RUNEBENDER_GLYPH"),
            #[cfg(unix)]
            proof_sample: variables.text("RUNEBENDER_PROOF_SAMPLE").as_deref() == Some("1"),
            component: variables
                .text("RUNEBENDER_COMPONENT")
                .and_then(|value| value.parse().ok()),
            label_sample: variables
                .text("RUNEBENDER_LABEL_SAMPLE")
                .and_then(|value| value.parse().ok()),
            select_all: variables.text("RUNEBENDER_SELECTALL").is_some(),
            proof_view: match variables.text("RUNEBENDER_PROOF_VIEW").as_deref() {
                Some("1") => Some(ProofView::Compare),
                Some("toggle") => Some(ProofView::Toggle),
                Some("before") => Some(ProofView::Before),
                Some("waterfall") => Some(ProofView::Waterfall),
                Some("text") => Some(ProofView::Text),
                _ => None,
            },
            proof_dark: variables.text("RUNEBENDER_PROOF_DARK").as_deref() == Some("1"),
            proof_text: variables.text("RUNEBENDER_PROOF_TEXT"),
            proof_menu: variables.text("RUNEBENDER_PROOF_MENU").as_deref() == Some("1"),
            preview_text: variables.text("RUNEBENDER_PREVIEW_TEXT"),
            selected: variables.text("RUNEBENDER_SELECTED"),
            preview: variables
                .text("RUNEBENDER_PREVIEW")
                .map(|view| match view.as_str() {
                    "pieces" => PreviewView::Pieces,
                    "model" => PreviewView::Model,
                    _ => PreviewView::Outline,
                }),
            neural_status: variables.text("RUNEBENDER_NEURAL_STATUS"),
            expand: variables
                .text("RUNEBENDER_EXPAND")
                .map(|keys| keys.split(',').map(|key| key.trim().to_owned()).collect())
                .unwrap_or_default(),
            #[cfg(unix)]
            graph_script: variables
                .text("RUNEBENDER_GRAPH_SCRIPT")
                .and_then(|value| value.parse().ok()),
            #[cfg(unix)]
            graph_expanded: variables.is_set("RUNEBENDER_GRAPH_EXPANDED"),
        }
    }

    /// Set up the first workspace of a launch: the open glyph, its selection and the proof view.
    ///
    /// The launch path calls this once, in a window and in a capture.
    pub(crate) fn apply(&self, workspace: &mut Workspace) {
        if let Some(name) = &self.glyph
            && let Some(index) = workspace.font.index_of(name)
        {
            workspace.open_glyph(index);
            if workspace.proof_view {
                workspace.pin_review_before();
            }
            #[cfg(unix)]
            if self.proof_sample {
                proof_sample(workspace);
            }
            if let Some(id) = self.component.and_then(|n| {
                let layer = workspace.session.current_layer()?;
                layer.components().nth(n).map(|component| component.id())
            }) {
                workspace.enter_component(id);
            }
            if let Some(sample) = self.label_sample {
                Arc::make_mut(&mut workspace.session).select_sample(Some(sample));
            }
        }
        if self.select_all {
            let mut session = (*workspace.session).clone();
            session.select_all();
            workspace.selected_points = session.point_count();
            workspace.session = Arc::new(session);
            workspace.refresh_coord_bufs();
        }
        // The review mode can be captured without changing the user's running editor.
        if let Some(view @ (ProofView::Toggle | ProofView::Before)) = self.proof_view {
            workspace.review.mode = ReviewMode::Toggle;
            workspace.review.show_before = view == ProofView::Before;
        }
        workspace.review.dark = self.proof_dark;
        match self.proof_view {
            Some(ProofView::Waterfall) => workspace.review.mode = ReviewMode::Waterfall,
            Some(ProofView::Text) => workspace.review.mode = ReviewMode::Text,
            _ => {}
        }
        if let Some(text) = &self.proof_text {
            workspace.review.reading.custom = text.chars().take(4096).collect();
            workspace.review.reading.sample = Sample::Custom;
        }
        workspace.review.reading.sample_menu_open = self.proof_menu;
        workspace.refresh_proof_files();
    }

    /// Set up the parts of a capture that a window leaves alone, after [`Self::apply`].
    pub(crate) fn apply_capture(&self, workspace: &mut Workspace) {
        if let Some(name) = &self.selected
            && let Some(index) = workspace.font.index_of(name)
        {
            workspace.grid_select(index, false, false);
        }
        if let Some(text) = &self.preview_text {
            workspace.preview_text = text.clone();
        }
        // The model view of the proof strip waits for the worker's first drawing.
        if let Some(view) = self.preview {
            workspace.preview_view = view;
            if workspace.preview_view == PreviewView::Model {
                workspace.refresh_model();
                for _ in 0..400 {
                    workspace.model_pump();
                    if workspace.model.render.is_some() || workspace.model.error.is_some() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        if let Some(status) = &self.neural_status {
            workspace.train.status = status.clone();
        }
        for key in &self.expand {
            workspace.collapsed.remove(key.as_str());
        }
        // In the Graph view, wait for the specimen's first render.
        #[cfg(unix)]
        if matches!(workspace.mode, crate::application::workspace::Mode::Graph)
            && workspace.live_graph.is_some()
        {
            if let Some(example) = self.graph_script {
                crate::application::editor::tools::graph::controls::add_live_specimen_script(
                    workspace, None, example,
                );
            }
            // A capture shows the graph rendered, as if the designer pressed Run.
            workspace.graph.auto_run = true;
            crate::application::editor::tools::graph::controls::live_autorun_tick(workspace);
            for _ in 0..1200 {
                workspace.live_graph_pump();
                workspace.sync_live_graph_presentation();
                if !workspace.live_graph_need_pump() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            // Let the finished run settle so the nodes show it as current.
            for _ in 0..20 {
                workspace.live_graph_pump();
                workspace.sync_live_graph_presentation();
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            if self.graph_expanded {
                workspace.graph.expanded = workspace.live_graph.as_ref().and_then(|state| {
                    let graph = state.session.snapshot().graph;
                    graph
                        .nodes
                        .iter()
                        .find(|node| node.type_name == "live.proof")
                        .map(|node| node.id)
                });
            }
        }
    }
}

/// Propose the open glyph 64 units wider through the live tools, as a
/// model would, so the proof view has something to compare.
#[cfg(unix)]
fn proof_sample(app: &mut Workspace) {
    use runebender::automation::agent::ToolCall;
    let Some(source) = app.font.project.source_id(app.font.active()) else {
        return;
    };
    let glyph = app.session.glyph_name.clone();
    let read = app.call_live(&ToolCall {
        name: "read_glyph".into(),
        arguments: serde_json::json!({"glyph": glyph, "source": source.0}),
    });
    let x_min = read["advance"].as_f64().unwrap_or(0.0) / 2.0;
    let proposed = app.call_live(&ToolCall {
        name: "propose_edits".into(),
        arguments: serde_json::json!({
            "source": source.0, "task": "wider-sample",
            "reason": "A sample proposal for the proof view",
            "edits": [{"glyph": glyph, "expected_revision": read["revision"],
                "operations": [
                    {"op": "transform_points", "x_min": x_min, "dx": 64},
                    {"op": "adjust_width", "delta": 64}
                ]}]
        }),
    });
    eprintln!("proof sample proposal: {proposed}");
    app.refresh_proposals();
    app.select_review_proposal(Some("wider-sample".into()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;

    /// Parse the options from name and value pairs instead of the environment.
    fn options(pairs: &[(&str, &str)]) -> LaunchOptions {
        LaunchOptions::from_variables(&Variables(&|name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }))
    }

    fn documentation() -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/UI-DESIGN.md");
        std::fs::read_to_string(path).expect("docs/UI-DESIGN.md is readable")
    }

    /// The table rows under one heading of the documentation.
    fn table_rows(documentation: &str, heading: &str) -> Vec<String> {
        documentation
            .lines()
            .skip_while(|line| *line != heading)
            .skip(1)
            .take_while(|line| !line.starts_with('#'))
            .filter(|line| line.starts_with("| `RUNEBENDER_"))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn the_variable_table_matches_the_documentation() {
        let expected: Vec<String> = VARIABLES
            .iter()
            .map(|variable| {
                format!(
                    "| `{}={}` | {} |",
                    variable.name,
                    variable.value.replace('|', "\\|"),
                    variable.help
                )
            })
            .collect();
        let documented = table_rows(&documentation(), "### Launch variables");
        assert_eq!(
            documented, expected,
            "the Launch variables table in docs/UI-DESIGN.md must repeat VARIABLES row for row"
        );
    }

    #[test]
    fn variable_names_are_unique() {
        let names: BTreeSet<_> = VARIABLES.iter().map(|variable| variable.name).collect();
        assert_eq!(names.len(), VARIABLES.len());
    }

    #[test]
    fn every_variable_in_the_source_is_documented() {
        fn collect(directory: &Path, names: &mut BTreeSet<String>) {
            for entry in std::fs::read_dir(directory).expect("the source tree is readable") {
                let path = entry.expect("a directory entry is readable").path();
                if path.is_dir() {
                    collect(&path, names);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    let text = std::fs::read_to_string(&path).expect("a source file is readable");
                    for (start, _) in text.match_indices("RUNEBENDER_") {
                        let name: String = text[start..]
                            .chars()
                            .take_while(|c| {
                                c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_'
                            })
                            .collect();
                        // A name that ends with an underscore is a prefix in prose, not a variable.
                        if !name.ends_with('_') {
                            names.insert(name);
                        }
                    }
                }
            }
        }

        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut names = BTreeSet::new();
        for directory in ["src", "benches"] {
            collect(&root.join(directory), &mut names);
        }
        let documentation = documentation();
        let documented: BTreeSet<String> = [
            "### Launch variables",
            "### Variables that other code reads",
        ]
        .into_iter()
        .flat_map(|heading| table_rows(&documentation, heading))
        .filter_map(|row| {
            let name = row.strip_prefix("| `")?;
            let end = name.find(['=', '`'])?;
            Some(name[..end].to_owned())
        })
        .collect();
        let missing: Vec<_> = names.difference(&documented).collect();
        assert!(
            missing.is_empty(),
            "docs/UI-DESIGN.md has no row for {missing:?}"
        );
        let stale: Vec<_> = documented.difference(&names).collect();
        assert!(
            stale.is_empty(),
            "docs/UI-DESIGN.md documents {stale:?}, which the source does not name"
        );
    }

    #[test]
    fn no_variable_gives_a_window_and_no_capture() {
        let options = options(&[]);
        assert_eq!(options.capture, None);
        assert!(!options.frame_stats);
    }

    #[test]
    fn a_capture_reads_its_size_and_scale() {
        let capture = options(&[("RUNEBENDER_SCREENSHOT", "/tmp/frame.png")]).capture;
        assert_eq!(
            capture,
            Some(CaptureOptions {
                path: "/tmp/frame.png".into(),
                size: (1100, 720),
                scale: 1.0,
            })
        );
        let capture = options(&[
            ("RUNEBENDER_SCREENSHOT", "/tmp/frame.png"),
            ("RUNEBENDER_SIZE", " 1500 x 2200 "),
            ("RUNEBENDER_SCALE", "2"),
        ])
        .capture
        .expect("the capture path is set");
        assert_eq!(capture.size, (1500, 2200));
        assert_eq!(capture.scale, 2.0);
        // A size or scale that does not parse falls back to the default.
        let capture = options(&[
            ("RUNEBENDER_SCREENSHOT", "/tmp/frame.png"),
            ("RUNEBENDER_SIZE", "wide"),
            ("RUNEBENDER_SCALE", "-1"),
        ])
        .capture
        .expect("the capture path is set");
        assert_eq!(capture.size, (1100, 720));
        assert_eq!(capture.scale, 1.0);
    }

    #[cfg(unix)]
    #[test]
    fn every_row_of_the_table_is_parsed() {
        let read = std::cell::RefCell::new(BTreeSet::new());
        LaunchOptions::from_variables(&Variables(&|name| {
            read.borrow_mut().insert(name.to_owned());
            Some(OsString::from("1"))
        }));
        let rows: BTreeSet<String> = VARIABLES
            .iter()
            .map(|variable| variable.name.to_owned())
            .collect();
        assert_eq!(read.into_inner(), rows);
    }

    #[test]
    fn the_proof_view_names_its_page() {
        let view = |value| options(&[("RUNEBENDER_PROOF_VIEW", value)]).proof_view;
        assert_eq!(view("1"), Some(ProofView::Compare));
        assert_eq!(view("toggle"), Some(ProofView::Toggle));
        assert_eq!(view("before"), Some(ProofView::Before));
        assert_eq!(view("waterfall"), Some(ProofView::Waterfall));
        assert_eq!(view("text"), Some(ProofView::Text));
        assert_eq!(view("yes"), None);
        assert_eq!(options(&[]).proof_view, None);
    }

    #[test]
    fn switches_compare_their_value_or_only_their_presence() {
        // RUNEBENDER_PROOF_DARK needs the value 1; RUNEBENDER_SELECTALL needs any Unicode value.
        assert!(!options(&[("RUNEBENDER_PROOF_DARK", "true")]).proof_dark);
        assert!(options(&[("RUNEBENDER_PROOF_DARK", "1")]).proof_dark);
        assert!(options(&[("RUNEBENDER_SELECTALL", "")]).select_all);
        assert!(!options(&[]).select_all);
    }

    #[test]
    fn expanded_sections_are_trimmed_keys() {
        let options = options(&[("RUNEBENDER_EXPAND", "Kerning, Groups")]);
        assert_eq!(options.expand, ["Kerning", "Groups"]);
    }

    #[test]
    fn frame_statistics_need_only_a_set_variable() {
        assert!(options(&[("RUNEBENDER_FRAME_STATS", "")]).frame_stats);
    }
}
