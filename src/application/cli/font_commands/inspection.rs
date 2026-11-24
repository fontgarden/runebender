// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Headless source inspection, proof generation, and MCP proof content.

use super::*;

/// Arguments of `info`.
#[derive(clap::Args)]
pub(in crate::application::cli) struct InfoArgs {
    /// The UFO.
    pub(in crate::application::cli) source: PathBuf,
    /// List every glyph with its codepoints.
    #[arg(long)]
    pub(in crate::application::cli) glyphs: bool,
}

/// What a font is, for a person or a program about to work on it.
pub(in crate::application::cli) fn info(args: &InfoArgs) -> CommandResult {
    let source = &args.source;
    let project = open_project(source)?;
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let default_layer = project
        .document_source(source_id)
        .expect("one source")
        .default_layer();
    let glyph_names = project
        .glyph_names()
        .filter(|name| project.document_layer(name, &default_layer).is_some())
        .collect::<Vec<_>>();
    let drawn = glyph_names
        .iter()
        .filter(|name| {
            project
                .document_layer(name, &default_layer)
                .is_some_and(|layer| {
                    layer.contours().next().is_some() || layer.components().next().is_some()
                })
        })
        .count();
    let proposals = proposal::list_project(&project, source_id);
    let layers = project
        .document_source_layer_names(source_id)
        .expect("one source retains layer structure");
    let info = project
        .document_font_info(source_id)
        .expect("one source retains canonical font information");
    let metrics = info.metrics.resolved();
    let metadata = project
        .document_font_metadata(source_id)
        .expect("one source retains canonical metadata");
    let glyph_codepoints = |name: &str| {
        let layer = project
            .document_layer(name, &default_layer)
            .expect("collected default-layer glyph");
        codepoints(layer.codepoints())
    };
    let mut value = json!({
        "ok": true,
        "source": source,
        "family": info.names.family_name,
        "style": info.names.style_name,
        "unitsPerEm": metrics.units_per_em,
        "ascender": metrics.ascender,
        "descender": metrics.descender,
        "xHeight": info.metrics.x_height,
        "capHeight": info.metrics.cap_height,
        "glyphs": glyph_names.len(),
        "drawn": drawn,
        "layers": layers,
        "kerningPairs": metadata.kerning_pairs().count(),
        "proposals": proposals,
    });
    let mut text = String::new();
    push_line!(
        text,
        "{} {}",
        info.names.family_name.as_deref().unwrap_or("(no family)"),
        info.names.style_name.as_deref().unwrap_or("")
    );
    push_line!(
        text,
        "{} upm, ascender {}, descender {}",
        metrics.units_per_em,
        metrics.ascender,
        metrics.descender
    );
    push_line!(
        text,
        "{} glyphs, {drawn} drawn, layers: {}",
        glyph_names.len(),
        layers.join(", ")
    );
    for p in &proposals {
        push_line!(
            text,
            "proposal {}: {} glyphs ({} compatible, {} not, {} missing)",
            p.task,
            p.glyphs.len(),
            p.compatible.len(),
            p.incompatible.len(),
            p.missing.len()
        );
    }
    if args.glyphs {
        value["glyphList"] = glyph_names
            .iter()
            .map(|name| json!({ "name": name, "codepoints": glyph_codepoints(name) }))
            .collect();
        for name in &glyph_names {
            push_line!(text, "  {name:<24} {}", glyph_codepoints(name).join(" "));
        }
    }
    Ok(CommandOutput::new(value, text))
}

/// Glyphs per row of a proof sheet when the caller names no other count.
pub(in crate::application::cli) const PROOF_COLUMNS: usize = 10;

/// Arguments of `proof`.
#[derive(clap::Args)]
pub(in crate::application::cli) struct ProofArgs {
    /// The UFO.
    pub(in crate::application::cli) source: PathBuf,
    /// Where to write the SVG. Defaults to proof.svg next to the UFO.
    #[arg(long)]
    pub(in crate::application::cli) out: Option<PathBuf>,
    /// Glyphs to draw. Defaults to every drawn glyph.
    #[arg(long, value_delimiter = ',')]
    pub(in crate::application::cli) glyphs: Option<Vec<String>>,
    /// Glyphs per row.
    #[arg(long, default_value_t = PROOF_COLUMNS)]
    pub(in crate::application::cli) columns: usize,
    /// Optional UFO layer to proof.
    #[arg(long)]
    pub(in crate::application::cli) layer: Option<String>,
}

/// A proof sheet: every glyph in a grid with its metric lines, as
/// SVG, and the numbers a reviewer wants next to it.
pub(in crate::application::cli) fn proof(args: &ProofArgs) -> CommandResult {
    let source = &args.source;
    let project = open_project(source)?;
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let default_layer = project
        .document_source(source_id)
        .expect("one source")
        .default_layer();
    let requested_layer = args.layer.as_deref().map(|name| LayerId {
        source: source_id,
        name: name.into(),
    });
    let names: Vec<String> = match (&args.glyphs, &requested_layer) {
        (Some(list), _) => list.clone(),
        (None, Some(requested)) => {
            if !project
                .document_source_layer_names(source_id)
                .is_some_and(|names| names.iter().any(|name| *name == requested.name))
            {
                return Err(CliError::usage("no such layer"));
            }
            project
                .glyph_names()
                .filter(|name| project.document_layer(name, requested).is_some())
                .map(str::to_owned)
                .collect()
        }
        (None, None) => project
            .glyph_names()
            .filter(|name| {
                project
                    .document_layer_path(&GlyphLayerAddress {
                        glyph: (*name).to_owned(),
                        layer: default_layer.clone(),
                    })
                    .is_ok_and(|path| !path.is_empty())
            })
            .map(str::to_owned)
            .collect(),
    };
    if names.is_empty() {
        return Err(CliError::usage("no glyph to draw"));
    }
    let sheet = runebender::formats::svg::proof_sheet_project(
        &project,
        source_id,
        args.layer.as_deref(),
        &names,
        args.columns,
    )
    .map_err(CliError::usage)?;
    let out = args.out.clone().unwrap_or_else(|| {
        source
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("proof.svg")
    });
    std::fs::write(&out, sheet.svg)
        .map_err(|error| CliError::failed(format!("{}: {error}", out.display())))?;
    let mut text = String::new();
    push_line!(text, "{} glyphs → {}", names.len(), out.display());
    for m in &sheet.metrics {
        push_line!(
            text,
            "  {:<24} advance {:>5}  lsb {:>5}  rsb {:>5}",
            m["glyph"].as_str().unwrap_or(""),
            m["advance"],
            m["lsb"],
            m["rsb"]
        );
    }
    Ok(CommandOutput::new(
        json!({ "ok": true, "svg": out, "glyphs": sheet.metrics }),
        text,
    ))
}

/// Metadata and compatibility content derived from the same proof result.
/// Image bytes are emitted once, outside the JSON metadata.
pub(in crate::application::cli) struct ProofContent {
    pub(in crate::application::cli) metadata: serde_json::Value,
    pub(in crate::application::cli) content: Vec<serde_json::Value>,
}

/// Return an actual MCP image alongside proof metadata, without external resources.
pub(in crate::application::cli) fn proof_content(mut value: serde_json::Value) -> ProofContent {
    use base64::Engine as _;
    let mut content = Vec::new();
    if let Some(png) = value
        .as_object_mut()
        .and_then(|object| object.remove("png_base64"))
    {
        content.push(serde_json::json!({"type":"image", "mimeType":"image/png", "data":png}));
    } else if let Some(scene) = value.get("scene") {
        let rendered = runebender::formats::designbot::render(scene, false);
        match rendered {
            Ok(png) => {
                content.push(serde_json::json!({"type":"image", "mimeType":"image/png",
                    "data":base64::engine::general_purpose::STANDARD.encode(png)}));
                value.as_object_mut().unwrap().remove("svg_content");
                value.as_object_mut().unwrap().remove("scene");
            }
            Err(error) => value["image_error"] = serde_json::json!(error),
        }
    }
    content.insert(
        0,
        serde_json::json!({"type":"text", "text":value.to_string()}),
    );
    ProofContent {
        metadata: value,
        content,
    }
}
