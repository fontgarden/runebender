// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Live graph node types, starter graphs and the specimen proof recipes.
//!
//! A run forks the current font by feeding `live.font` into a `live.python` node, which
//! produces a detached font version; Apply installs it as a separate user action.

use super::graph::{Kind, NodeCachePolicy, NodeEffects, NodeExecution, NodeGraph, NodeType, Port};
use crate::font::variable::SourceId;
use crate::formats::specimen_script::ScriptLanguage;
use serde_json::{Value, json};

/// Default compiled specimen recipe used by both comparison branches.
pub fn default_proof_recipe() -> Value {
    json!({
        "text": "Hamburgefontsiv",
        "normalized_location": [],
        "right_to_left": false,
        "features": [],
        "script": null,
        "language": null
    })
}

/// The proof recipe a run uses for `node`: its own `recipe` value, or the default, with the
/// code of a specimen script wired into its `code` input.
pub fn effective_proof_recipe(graph: &NodeGraph, node: u32) -> Value {
    let mut recipe = graph
        .node(node)
        .and_then(|node| node.values.get("recipe"))
        .cloned()
        .unwrap_or_else(default_proof_recipe);
    let script = graph
        .link_into(node, "code")
        .and_then(|link| graph.node(link.from()))
        .filter(|source| source.type_name == SPECIMEN_SCRIPT);
    if let (Some(script), Some(fields)) = (script, recipe.as_object_mut()) {
        let language =
            ScriptLanguage::from_name(script.values.get("language").and_then(Value::as_str));
        let code = script
            .values
            .get("code")
            .and_then(Value::as_str)
            .map_or_else(|| language.starter().to_owned(), str::to_owned);
        fields.insert(
            "specimen_script".into(),
            json!({ "language": language.name(), "code": code }),
        );
    }
    recipe
}

/// Node type holding designer drawing code that a proof renders with the font.
pub const SPECIMEN_SCRIPT: &str = "live.specimen_script";

/// Node types executed against an open editor, rather than the disk runner.
pub fn types() -> Vec<NodeType> {
    let port = |name: &str, input: bool| Port {
        name: name.into(),
        kind: Kind::FontVersion,
        required: input,
        default: None,
        help: "An in-memory font version.".into(),
    };
    [
        ("live.font", "Current font", false, true),
        ("live.python", "Python recipe", true, true),
        ("live.proof", "DesignBot proof", true, false),
    ]
    .into_iter()
    .map(|(name, title, input, output)| NodeType {
        name: name.into(),
        title: title.into(),
        help: "Runs in the live editor. Inputs are preserved; applying to the root is explicit."
            .into(),
        implemented: true,
        execution: NodeExecution::new(
            NodeCachePolicy::Never,
            if name == "live.python" {
                NodeEffects::TrustedProcess
            } else {
                NodeEffects::LiveDocument
            },
        ),
        inputs: {
            let mut ports = if input {
                vec![port("font", true)]
            } else {
                vec![]
            };
            if name == "live.font" {
                ports.push(Port {
                    name: "source".into(),
                    kind: Kind::Number,
                    required: false,
                    default: None,
                    help: "The stable source.".into(),
                });
            }
            if name == "live.python" {
                ports.extend([
                    Port {
                        name: "code".into(),
                        kind: Kind::Text,
                        required: true,
                        default: None,
                        help: "Exact Python source submitted to the shared recipe runner.".into(),
                    },
                    Port {
                        name: "parameters".into(),
                        kind: Kind::Parameters,
                        required: false,
                        default: Some(json!({})),
                        help: "Structured parameters included in the immutable recipe input."
                            .into(),
                    },
                ]);
            }
            if name == "live.proof" {
                ports.push(Port {
                    name: "code".into(),
                    kind: Kind::Text,
                    required: false,
                    default: None,
                    help: "A specimen script; when connected, its code draws the whole proof."
                        .into(),
                });
                ports.push(Port {
                    name: "recipe".into(),
                    kind: Kind::Parameters,
                    required: false,
                    default: Some(default_proof_recipe()),
                    help: "Structured specimen settings shared by both comparison proofs.".into(),
                });
            }
            ports
        },
        outputs: if output {
            vec![port("font", false)]
        } else {
            vec![]
        },
    })
    .chain(std::iter::once(specimen_script_type()))
    .collect()
}

/// A code node holding a `DesignBot` script. Its code output goes into a proof's `code` input, and
/// that proof renders the script with the font it reads.
fn specimen_script_type() -> NodeType {
    NodeType {
        name: SPECIMEN_SCRIPT.into(),
        title: "Specimen script".into(),
        help: "A DesignBot script that a proof renders with its font.".into(),
        implemented: true,
        execution: NodeExecution::new(NodeCachePolicy::Never, NodeEffects::TrustedProcess),
        inputs: vec![Port {
            name: "code".into(),
            kind: Kind::Text,
            required: false,
            default: Some(json!(ScriptLanguage::Designbot.starter())),
            help: "DesignBot Rust. FONT_PATH and FONT_FAMILY name the font being proofed.".into(),
        }],
        outputs: vec![Port {
            name: "code".into(),
            kind: Kind::Text,
            required: false,
            default: None,
            help: "The script, for a proof's code input.".into(),
        }],
    }
}

/// Create the first supported Python comparison graph.
///
/// One captured base feeds an unchanged proof and a Python-derived proof.
/// The Python code remains empty until the user or agent authors it, and opening
/// the graph never executes it.
pub fn comparison_starter(source_id: SourceId) -> NodeGraph {
    let mut graph = NodeGraph::default();
    let source = graph.add("live.font", [32.0, 32.0]);
    graph
        .node_mut(source)
        .unwrap()
        .values
        .insert("source".into(), json!(source_id.0));
    let unchanged = graph.add("live.proof", [640.0, 32.0]);
    graph
        .node_mut(unchanged)
        .unwrap()
        .values
        .insert("recipe".into(), default_proof_recipe());
    graph.connect(source, "font", unchanged, "font");
    let python = graph.add("live.python", [336.0, 32.0]);
    graph
        .node_mut(python)
        .unwrap()
        .values
        .insert("code".into(), json!(""));
    graph
        .node_mut(python)
        .unwrap()
        .values
        .insert("parameters".into(), json!({}));
    graph.connect(source, "font", python, "font");
    let changed = graph.add("live.proof", [928.0, 32.0]);
    graph
        .node_mut(changed)
        .unwrap()
        .values
        .insert("recipe".into(), default_proof_recipe());
    graph.connect(python, "font", changed, "font");
    graph
}

/// Specimen paper: near black, so light type reads as it would on a dark
/// screen and the image stands out on the canvas.
pub const SPECIMEN_PAPER: [u8; 3] = [15, 15, 15];
/// Specimen ink: an off-white that does not glare on the dark paper.
pub const SPECIMEN_INK: [u8; 3] = [235, 235, 235];
/// How many pages [`specimen_recipes`] makes.
pub const SPECIMEN_PAGES: usize = 2;

const PAGE_SIZE: f64 = 2048.0;
const PAGE_MARGIN: f64 = 144.0;
/// Room left on each line for edits that widen it. A line that grows past
/// the page is clipped, not failed.
const PAGE_SLACK: f64 = 1.06;

/// The default specimen pages, top to bottom: a poster of the character set
/// and a waterfall of one sentence. Both are 2048 pixels wide, light type
/// on dark paper, sized from `advance_em`, which measures a line's advance
/// in ems.
pub fn specimen_recipes(
    advance_em: impl Fn(&str) -> f64,
    normalized_location: &[f64],
) -> Vec<Value> {
    vec![
        poster_recipe(&advance_em, normalized_location),
        waterfall_recipe(&advance_em, normalized_location),
    ]
}

/// The largest size at which every line fits the page width.
fn fit_size(advance_em: &dyn Fn(&str) -> f64, lines: &[&str], width: f64) -> f64 {
    let widest = lines
        .iter()
        .map(|line| advance_em(line))
        .fold(0.5, f64::max);
    width / (widest * PAGE_SLACK)
}

/// One display word over the alphabet and figures, centered on a square page.
fn poster_recipe(advance_em: &dyn Fn(&str) -> f64, normalized_location: &[f64]) -> Value {
    // Seven short lines, so the set fills the square page in both directions.
    const BODY: [&str; 7] = [
        "ABCDEFGHI",
        "JKLMNOPQR",
        "STUVWXYZ",
        "abcdefghi",
        "jklmnopqr",
        "stuvwxyz",
        "0123456789",
    ];
    // Lines sit 1.15 em apart; a baseline is 0.95 em below its line's top,
    // which clears accents above and descenders below.
    const LEADING: f64 = 1.15;
    const DROP: f64 = 0.95;
    let room = PAGE_SIZE - 2.0 * PAGE_MARGIN;
    let lines_height = |size: f64| BODY.len() as f64 * size * LEADING;
    let size = fit_size(advance_em, &BODY, room).min(room / lines_height(1.0));
    let lines: Vec<_> = BODY
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let top = PAGE_MARGIN + size * LEADING * index as f64;
            (*text, size, top + size * DROP)
        })
        .collect();
    page_recipe(&lines, PAGE_SIZE, "start", normalized_location)
}

/// One sentence from the largest size that fits down to text sizes.
fn waterfall_recipe(advance_em: &dyn Fn(&str) -> f64, normalized_location: &[f64]) -> Value {
    const TEXT: &str = "The quick brown fox jumps over the lazy dog";
    const LEADING: f64 = 1.3;
    let mut size = fit_size(advance_em, &[TEXT], PAGE_SIZE - 2.0 * PAGE_MARGIN).min(200.0);
    let mut top = PAGE_MARGIN;
    let mut lines = Vec::new();
    // The first line always shows, however wide the font. A baseline sits
    // 0.8 em below its line's top, about where the cap height lands.
    while lines.is_empty() || (size >= 16.0 && lines.len() < 12) {
        lines.push((TEXT, size, top + size * 0.8));
        top += size * LEADING;
        size *= 0.8;
    }
    let height = (top + PAGE_MARGIN).min(PAGE_SIZE);
    page_recipe(&lines, height, "start", normalized_location)
}

/// A page recipe from `(text, pixels per em, baseline from the top)` lines.
fn page_recipe(
    lines: &[(&str, f64, f64)],
    height: f64,
    align: &str,
    normalized_location: &[f64],
) -> Value {
    let height = height.round().clamp(128.0, PAGE_SIZE);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "both sizes are whole numbers clamped to 128..=2048"
    )]
    let (width_px, height_px) = (PAGE_SIZE as u32, height as u32);
    let text = lines
        .iter()
        .map(|(text, ..)| *text)
        .collect::<Vec<_>>()
        .join("\n");
    let settings = lines
        .iter()
        .map(|&(_, size, baseline)| {
            json!({
                "pixels_per_em": size.clamp(4.0, 512.0).round(),
                "baseline_px": (height - baseline).clamp(0.0, height).round(),
                "align": align,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "text": text,
        "normalized_location": normalized_location,
        "right_to_left": false,
        "features": [],
        "script": null,
        "language": null,
        "rendering": {
            "width_px": width_px,
            "height_px": height_px,
            "pixels_per_em": 160.0,
            "margin_px": PAGE_MARGIN,
            "first_baseline_px": 180.0,
            "line_height_px": 180.0,
            "background_rgb": SPECIMEN_PAPER,
            "ink_rgb": SPECIMEN_INK,
            "lines": settings,
        },
    })
}

/// Whether a proof recipe is a waterfall: one text repeated down a page.
pub fn is_waterfall(recipe: &Value) -> bool {
    let lines = recipe["text"].as_str().map(|text| text.split('\n'));
    lines.is_some_and(|mut lines| {
        let first = lines.next();
        let mut rest = lines.peekable();
        rest.peek().is_some() && rest.all(|line| Some(line) == first)
    })
}

/// The proof's raster size in pixels, when its recipe sets one.
pub fn recipe_raster(recipe: &Value) -> Option<[f64; 2]> {
    let rendering = recipe.get("rendering")?;
    Some([
        rendering.get("width_px")?.as_f64()?,
        rendering.get("height_px")?.as_f64()?,
    ])
}

/// The default live graph: the current font feeding one proof per specimen page.
///
/// `recipes` come from [`specimen_recipes`], top to bottom.
/// Script examples are added explicitly: their cold compilation must not delay ordinary proofs.
pub fn specimen_starter(source_id: SourceId, recipes: Vec<Value>) -> NodeGraph {
    // A proof node shows its image at the standard content width, 240 px,
    // inside a header, two port rows, and padding: 80 px in all
    // (see `ui::graph::node_box_with_content`). Then a gap.
    const CONTENT_WIDTH: f64 = 240.0;
    const NODE_CHROME: f32 = 80.0;
    const GAP: f32 = 32.0;
    let mut graph = NodeGraph::default();
    let source = graph.add("live.font", [32.0, 32.0]);
    graph
        .node_mut(source)
        .unwrap()
        .values
        .insert("source".into(), json!(source_id.0));
    let mut y = 32.0_f32;
    for recipe in recipes {
        let height = recipe_raster(&recipe).map_or(144.0, |[w, h]| CONTENT_WIDTH * h / w.max(1.0));
        let proof = graph.add("live.proof", [336.0, y]);
        graph
            .node_mut(proof)
            .unwrap()
            .values
            .insert("recipe".into(), recipe);
        graph.connect(source, "font", proof, "font");
        #[allow(
            clippy::cast_possible_truncation,
            reason = "canvas positions are small finite numbers"
        )]
        {
            y += (height as f32) + NODE_CHROME + GAP;
        }
    }
    graph
}

/// Whether the graph holds a Python comparison: a recipe fed by the
/// font, with a proof of its result.
pub fn has_comparison(graph: &NodeGraph) -> bool {
    let Some(source) = graph.nodes.iter().find(|n| n.type_name == "live.font") else {
        return false;
    };
    graph.nodes.iter().any(|python| {
        python.type_name == "live.python"
            && graph
                .link_into(python.id, "font")
                .is_some_and(|link| link.from() == source.id)
            && graph.links.iter().any(|link| {
                link.from() == python.id
                    && graph
                        .node(link.to())
                        .is_some_and(|n| n.type_name == "live.proof")
            })
    })
}

/// Add the Python comparison beside whatever the graph already shows:
/// an unchanged proof of the font, an empty Python recipe, and a proof
/// of its result, both proofs rendering `recipe`. Returns the Python node.
///
/// The pair is placed below the lowest node, so a specimen keeps its place.
pub fn add_comparison(graph: &mut NodeGraph, recipe: &Value) -> Option<u32> {
    let source = graph.nodes.iter().find(|n| n.type_name == "live.font")?.id;
    let below = graph
        .nodes
        .iter()
        .map(|node| node.pos[1])
        .fold(32.0_f32, f32::max);
    let y = below + 320.0;
    let unchanged = graph.add("live.proof", [640.0, y]);
    graph
        .node_mut(unchanged)?
        .values
        .insert("recipe".into(), recipe.clone());
    graph.connect(source, "font", unchanged, "font");
    let python = graph.add("live.python", [336.0, y]);
    let node = graph.node_mut(python)?;
    node.values.insert("code".into(), json!(""));
    node.values.insert("parameters".into(), json!({}));
    graph.connect(source, "font", python, "font");
    let changed = graph.add("live.proof", [928.0, y]);
    graph
        .node_mut(changed)?
        .values
        .insert("recipe".into(), recipe.clone());
    graph.connect(python, "font", changed, "font");
    Some(python)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_specimen_script_wired_into_a_proof_runs_as_its_recipe() {
        let mut graph = specimen_starter(SourceId(0), vec![default_proof_recipe()]);
        let proof = graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.proof")
            .unwrap()
            .id;
        assert!(
            effective_proof_recipe(&graph, proof)
                .get("specimen_script")
                .is_none()
        );

        let script = graph.add(SPECIMEN_SCRIPT, [0.0, 600.0]);
        graph.connect(script, "code", proof, "code");
        let recipe = effective_proof_recipe(&graph, proof);
        assert_eq!(recipe["specimen_script"]["language"], "designbot");
        assert_eq!(
            recipe["specimen_script"]["code"],
            ScriptLanguage::Designbot.starter()
        );
        let parsed: crate::font::compiler::proof::CompiledProofRecipe =
            serde_json::from_value(recipe).unwrap();
        assert!(parsed.validate().is_ok());

        graph
            .node_mut(script)
            .unwrap()
            .values
            .insert("language".into(), json!("drawbot"));
        graph
            .node_mut(script)
            .unwrap()
            .values
            .insert("code".into(), json!("rect(0, 0, 10, 10)"));
        let recipe = effective_proof_recipe(&graph, proof);
        assert_eq!(recipe["specimen_script"]["language"], "drawbot");
        assert_eq!(recipe["specimen_script"]["code"], "rect(0, 0, 10, 10)");
    }
    #[test]
    fn live_outputs_cannot_connect_to_disk_tasks() {
        let mut g = comparison_starter(SourceId(0));
        let target = g.add("core.layer", [0.0, 0.0]);
        g.connect(1, "font", target, "source");
        assert!(
            g.validate(&super::super::graph::Registry::core())
                .iter()
                .any(|p| matches!(p, super::super::graph::Problem::KindMismatch { .. }))
        );
    }

    #[test]
    fn specimen_pages_are_valid_dark_proofs_whose_lines_fit() {
        let advance = |text: &str| text.chars().count() as f64 * 0.55;
        let recipes = specimen_recipes(advance, &[]);
        assert_eq!(recipes.len(), SPECIMEN_PAGES);
        for recipe in &recipes {
            let parsed: crate::font::compiler::proof::CompiledProofRecipe =
                serde_json::from_value(recipe.clone()).unwrap();
            parsed.validate().unwrap();
            let rendering = &parsed.rendering;
            assert_eq!(rendering.background_rgb, SPECIMEN_PAPER);
            assert_eq!(rendering.width_px, 2048);
            for (text, line) in parsed.text.split('\n').zip(&rendering.lines) {
                assert!(
                    advance(text) * line.pixels_per_em <= 2048.0 - 2.0 * PAGE_MARGIN,
                    "{text} fits the page"
                );
            }
        }
        assert!(!is_waterfall(&recipes[0]));
        assert!(is_waterfall(&recipes[1]));
    }

    #[test]
    fn a_very_wide_font_still_gets_a_valid_page() {
        for recipe in specimen_recipes(|text| text.chars().count() as f64 * 3.0, &[]) {
            let parsed: crate::font::compiler::proof::CompiledProofRecipe =
                serde_json::from_value(recipe).unwrap();
            parsed.validate().unwrap();
        }
    }
}
