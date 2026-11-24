// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Outline analysis and metaball conversion commands.

use super::*;
use runebender::analysis::weight_debt;
use runebender::outline::embolden;

/// Arguments of `bolden`.
#[derive(clap::Args)]
pub(in crate::application::cli) struct BoldenArgs {
    /// The lighter master.
    #[arg(long)]
    from: PathBuf,
    /// The heavier master, part-drawn.
    #[arg(long)]
    to: PathBuf,
    /// Glyphs to learn from. Defaults to n,o,H,O.
    #[arg(long, value_delimiter = ',')]
    references: Option<Vec<String>>,
    /// Glyphs to report on. Defaults to every one still identical
    /// in both masters, which is the work not yet done.
    #[arg(long, value_delimiter = ',')]
    glyphs: Option<Vec<String>>,
    /// Stop after this many.
    #[arg(long, default_value = "40")]
    limit: usize,
    /// Score the learned offset against glyphs drawn in both
    /// masters instead of listing what is undrawn.
    #[arg(long)]
    check: bool,
}

/// What the reference glyphs say the heavier master should do.
///
/// Reports rather than writes. Seeing the offset and the list first is
/// the difference between a tool you can trust with a font and one you
/// run once and then undo.
/// The debt and the scores come from [`weight_debt`]; this function only orders and prints them.
pub(in crate::application::cli) fn bolden(args: &BoldenArgs) -> CommandResult {
    let light = open_project(&args.from)?;
    let heavy = open_project(&args.to)?;
    let light_master = (&light, light.source_id(0).expect("one source"));
    let heavy_master = (&heavy, heavy.source_id(0).expect("one source"));
    let light_layer =
        weight_debt::default_layer(light_master.0, light_master.1).expect("one source");
    let heavy_layer =
        weight_debt::default_layer(heavy_master.0, heavy_master.1).expect("one source");
    let default_refs = ["n", "o", "H", "O"].map(str::to_owned);
    let refs: &[String] = args.references.as_deref().unwrap_or(&default_refs);
    let pairs: Vec<_> = refs
        .iter()
        .filter_map(|n| {
            Some((
                light.document_layer(n, &light_layer)?,
                heavy.document_layer(n, &heavy_layer)?,
            ))
        })
        .collect();
    let offset = embolden::learn_layer_offset(&pairs).ok_or_else(|| {
        CliError::usage("no reference glyph is drawn and compatible in both masters")
    })?;
    let debt = || weight_debt::report(light_master, heavy_master).map_err(CliError::failed);
    if args.check {
        // Glyphs drawn in both masters, where the answer is already known.
        let names = match &args.glyphs {
            Some(list) => list.clone(),
            None => in_glyph_order(
                &light,
                debt()?.groups.into_iter().flat_map(|group| group.drawn),
            ),
        };
        let limited: Vec<String> = names.into_iter().take(args.limit).collect();
        let score = weight_debt::score_offset(light_master, heavy_master, &limited, offset)
            .ok_or_else(|| CliError::failed("no glyph is drawn in both masters"))?;
        let per_glyph: Vec<_> = score
            .glyphs
            .iter()
            .map(|glyph| {
                json!({ "glyph": glyph.glyph, "offset": glyph.error, "baseline": glyph.baseline_error })
            })
            .collect();
        return Ok(CommandOutput::new(
            json!({
                "ok": true, "glyphs": score.glyphs.len(),
                "offset_mae": score.mean_error,
                "baseline_mae": score.baseline_error,
                "beats_baseline": score.wins,
                "per_glyph": per_glyph,
            }),
            format!(
                "{} glyphs drawn in both: offset {:.1}, baseline {:.1}, offset wins on {}\n",
                score.glyphs.len(),
                score.mean_error,
                score.baseline_error,
                score.wins
            ),
        ));
    }
    // What is left to do: glyphs whose heavier master still matches
    // the lighter one point for point.
    let todo: Vec<String> = match &args.glyphs {
        Some(list) => list.clone(),
        None => in_glyph_order(
            &light,
            debt()?.groups.into_iter().flat_map(|group| group.pending),
        ),
    };
    let rows: Vec<_> = todo
        .iter()
        .take(args.limit)
        .filter_map(|name| {
            let glyph = light.document_layer(name, &light_layer)?;
            let original = weight_debt::flat_points(glyph);
            let predicted = embolden::layer_point_moves(glyph, offset, 0.0);
            let moved = original
                .iter()
                .zip(&predicted)
                .filter(|(a, b)| a != b)
                .count();
            Some((name, moved, original.len()))
        })
        .collect();
    let mut text = String::new();
    push_line!(
        text,
        "learned from {} reference glyphs: push out {:.1} horizontally, {:.1} vertically",
        pairs.len(),
        offset.x,
        offset.y
    );
    push_line!(
        text,
        "{} glyphs still undrawn in the heavier master",
        todo.len()
    );
    for (name, moved, points) in &rows {
        push_line!(text, "  {name:<22} {moved}/{points} points would move");
    }
    if todo.len() > rows.len() {
        push_line!(text, "  ... and {} more", todo.len() - rows.len());
    }
    let value = json!({
        "ok": true,
        "offset": { "x": offset.x, "y": offset.y },
        "references": refs,
        "pending": todo.len(),
        "glyphs": rows.iter().map(|(n, m, p)| json!({
            "glyph": n, "pointsMoved": m, "points": p,
        })).collect::<Vec<_>>(),
    });
    Ok(CommandOutput::new(value, text))
}

/// Glyph names in the glyph order of `project`.
///
/// [`weight_debt::report`] groups glyphs by script; the command lists them in font order.
fn in_glyph_order(project: &Project, names: impl Iterator<Item = String>) -> Vec<String> {
    let order: std::collections::HashMap<&str, usize> = project
        .glyph_names()
        .enumerate()
        .map(|(index, name)| (name, index))
        .collect();
    let mut names: Vec<String> = names.collect();
    names.sort_by_key(|name| order.get(name.as_str()).copied().unwrap_or(usize::MAX));
    names
}

/// Arguments of `collapse-metaballs`.
#[derive(clap::Args)]
pub(in crate::application::cli) struct CollapseMetaballsArgs {
    /// Input UFO; never modified.
    source: PathBuf,
    /// New output UFO directory. Must not already exist.
    #[arg(long)]
    out: PathBuf,
    /// Sampling grid spacing in font units; smaller captures finer details.
    #[arg(long, default_value = "2")]
    resolution: f64,
    /// Cubic fitting accuracy relative to the sampled boundary, in font units.
    #[arg(long, default_value = "0.25")]
    accuracy: f64,
}

/// Convert every metaball group in a single source and save the result as a new UFO.
pub(in crate::application::cli) fn collapse_metaballs(
    args: &CollapseMetaballsArgs,
) -> CommandResult {
    use runebender::outline::metaballs::OutlineOptions;
    let out = &args.out;
    if out.exists() {
        return Err(CliError::usage(
            "output already exists; choose a new UFO path",
        ));
    }
    let mut project = open_project(&args.source)?;
    let source_id = project.source_id(0).expect("one source");
    let layer = project
        .document_source(source_id)
        .expect("one source")
        .default_layer();
    let names = project.glyph_names().map(str::to_owned).collect::<Vec<_>>();
    let mut count = 0;
    for glyph in names {
        let address = GlyphLayerAddress {
            glyph,
            layer: layer.clone(),
        };
        let Ok(mut transaction) = project.begin_document_layer_transaction(&address) else {
            continue;
        };
        let converted = transaction
            .draft_mut()
            .collapse_metaballs(
                None,
                OutlineOptions {
                    resolution: args.resolution,
                    accuracy: args.accuracy,
                },
            )
            .map_err(CliError::failed)?;
        if converted == 0 {
            continue;
        }
        project
            .commit_document_layer_transaction(transaction)
            .map_err(CliError::failed)?;
        count += converted;
    }
    project.save_as(out).map_err(CliError::failed)?;
    Ok(CommandOutput::new(
        json!({"ok": true, "groups_converted": count, "output": out}),
        format!(
            "Converted {count} metaball groups to cubic outlines in {}\n",
            out.display()
        ),
    ))
}
