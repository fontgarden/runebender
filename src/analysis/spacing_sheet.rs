// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Unkerned outline proofs of the Latin contexts in a spacing scan page.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use kurbo::{Rect, Shape as _};
use serde_json::Value;

use crate::font::project::Project;
use crate::font::variable::{GlyphLayerAddress, SourceId};

fn label<'a>(row: &'a Value, field: &str) -> Result<&'a str, String> {
    row[field]
        .as_str()
        .filter(|s| {
            s.len() <= 96
                && s.chars()
                    .all(|c| !c.is_control() && !matches!(c, '\u{fffe}' | '\u{ffff}'))
        })
        .ok_or_else(|| format!("invalid proof label {field}"))
}

fn metric(row: &Value, field: &str) -> Result<f64, String> {
    row[field]
        .as_f64()
        .ok_or_else(|| format!("missing proof metric {field}"))
}

/// Render a scan page as SVG using the selected source's resolved outlines and exact advances.
///
/// Each row uses the same scale. Repeated glyphs reference one path definition.
pub fn render(project: &Project, source: SourceId, report: &Value) -> Result<String, String> {
    let master = project
        .document_source(source)
        .ok_or("unknown proof source")?;
    let rows = report["rows"].as_array().ok_or("missing proof rows")?;
    let upm = report["units_per_em"]
        .as_f64()
        .filter(|value| *value > 0.0)
        .ok_or("invalid proof units per em")?;
    if report["source_id"].as_u64() != u64::try_from(source.0).ok() || rows.len() > 100 {
        return Err("invalid source or page size for spacing proof".into());
    }
    let layer = master.default_layer();
    let mut defs = String::new();
    let mut glyphs = BTreeMap::new();
    let mut page_bounds = Rect::ZERO;
    let mut body = String::new();
    for row in rows {
        let text = label(row, "text")?;
        if text.is_empty() || text.len() > 32 || !text.bytes().all(|b| b.is_ascii_alphabetic()) {
            return Err("invalid Latin proof context".into());
        }
        let mut pen = 0.0;
        for ch in text.chars() {
            if let std::collections::btree_map::Entry::Vacant(entry) = glyphs.entry(ch) {
                let name = ch.to_string();
                let view = project
                    .document_layer(&name, &layer)
                    .ok_or_else(|| format!("missing proof glyph {ch}"))?;
                let path = project
                    .document_layer_path(&GlyphLayerAddress {
                        glyph: name,
                        layer: layer.clone(),
                    })
                    .map_err(|err| err.to_string())?;
                if path.elements().len() > 32768 {
                    return Err(format!("spacing proof outline {ch} is too large"));
                }
                let bounds = path.bounding_box();
                if !view.width().is_finite() || !bounds.is_finite() {
                    return Err(format!("invalid proof geometry for {ch}"));
                }
                writeln!(defs, r#"<path id="g{ch}" d="{}"/>"#, path.to_svg()).unwrap();
                if defs.len() > 2_000_000 {
                    return Err("spacing proof exceeds the outline size limit".into());
                }
                entry.insert((view.width(), bounds));
            }
            let (advance, bounds) = glyphs[&ch];
            page_bounds = page_bounds.union(bounds + kurbo::Vec2::new(pen, 0.0));
            pen += advance;
        }
    }
    let scale = (64.0 / upm)
        .min(540.0 / page_bounds.width())
        .min(65.0 / page_bounds.height());
    for (i, row) in rows.iter().enumerate() {
        let top = 56 + i * 92;
        let id = quick_xml::escape::escape(label(row, "id")?);
        let glyph = quick_xml::escape::escape(label(row, "glyph")?);
        let side = quick_xml::escape::escape(label(row, "side")?);
        let reference = quick_xml::escape::escape(label(row, "reference")?);
        let bearing = metric(row, "bearing")?;
        let control = metric(row, "reference_bearing")?;
        let score = metric(row, "score")?;
        let id_y = top + 29;
        let metrics_y = top + 54;
        writeln!(body, r##"<g><title>{id}</title>
<path d="M20 {top}h880" stroke="#ccc"/>
<text x="20" y="{id_y}">{id}</text>
<text x="20" y="{metrics_y}" font-size="14">{glyph} {side} {bearing:.1} · {reference} {control:.1} · score {score:.1}</text>"##).unwrap();
        let x = 350.0 - page_bounds.x0 * scale;
        let y = top as f64 + 12.0 + page_bounds.y1 * scale;
        writeln!(
            body,
            r#"<g transform="translate({x} {y}) scale({scale} -{scale})">"#
        )
        .unwrap();
        let mut pen = 0.0;
        for ch in label(row, "text")?.chars() {
            writeln!(body, r##"<use href="#g{ch}" x="{pen}"/>"##).unwrap();
            pen += glyphs[&ch].0;
        }
        body.push_str("</g></g>\n");
    }
    if rows.is_empty() {
        body.push_str(r#"<text x="20" y="82">No candidates on this page.</text>"#);
    }
    let height = 56 + rows.len().max(1) * 92;
    let name: String = master
        .name()
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '\u{fffe}' | '\u{ffff}'))
        .take(64)
        .collect();
    let name = quick_xml::escape::escape(&name);
    Ok(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="920" height="{height}" viewBox="0 0 920 {height}">
<title>Unkerned spacing proof · {name}</title>
<rect width="100%" height="100%" fill="white"/>
<defs>{defs}</defs>
<g fill="#171717" font-family="sans-serif" font-size="18">
<text x="20" y="33">Unkerned spacing · {name}</text>
<text x="550" y="33" font-size="14">Sidebearings and score in font units</text>
{body}</g></svg>"##
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn proof_reuses_source_outlines_and_places_each_at_the_unkerned_advance() {
        let project = Project::load(&crate::testing::fonts::regular_ufo()).unwrap();
        let source = project.source_id(0).unwrap();
        let entry = project
            .document_source_glyph_entry(source, "H")
            .unwrap()
            .unwrap();
        let report = json!({
            "source_id": source.0, "units_per_em": 1000,
            "rows": [{
                "id": "<H&>", "glyph": "H", "side": "left", "bearing": 40,
                "reference": "H", "reference_bearing": 40, "score": 0, "text": "HH"
            }]
        });
        let svg = render(&project, source, &report).unwrap();
        assert_eq!(svg.matches(r#"<path id="gH""#).count(), 1);
        assert!(svg.contains(&entry.outline().to_svg()));
        assert!(svg.contains(&format!(r##"<use href="#gH" x="{}"/>"##, entry.advance())));
        assert!(svg.contains("&lt;H&amp;&gt;"));
    }
}
