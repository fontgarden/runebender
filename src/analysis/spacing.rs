// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Review basic Latin sidebearings against similarly shaped sides of H, O, n and o.

use std::collections::BTreeMap;

use kurbo::Shape as _;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::font::project::Project;
use crate::font::variable::{GlyphLayerAddress, SourceId};

// Only compare sides whose overall outline resembles the corresponding side of a control.
// An open aperture, diagonal or overhanging crossbar needs a different optical judgement.
const GROUPS: &[(&str, &str, &str)] = &[
    ("H", "left", "BDEFIKLMNPR"),
    ("H", "right", "IN"),
    ("O", "left", "CGQ"),
    ("O", "right", "DQ"),
    ("n", "left", "bhiklmpr"),
    ("n", "right", "hm"),
    ("o", "left", "cdeq"),
    ("o", "right", "bp"),
];

/// Return the first page of ranked spacing review rows for a source's default layer.
pub fn scan(project: &Project, source: SourceId, limit: usize) -> Result<Value, String> {
    scan_page(project, source, limit, 0)
}

/// Return a bounded page ordered by absolute bearing difference, then glyph and side.
///
/// The snapshot covers the scanned source geometry and comparison settings across all pages.
/// Bearings and scores are in font units. A difference below one percent of the em is omitted.
pub fn scan_page(
    project: &Project,
    source: SourceId,
    limit: usize,
    offset: usize,
) -> Result<Value, String> {
    if !(1..=100).contains(&limit) {
        return Err("spacing scan limit must be between 1 and 100".into());
    }
    let layer_id = project
        .document_source(source)
        .ok_or_else(|| format!("unknown source {}", source.0))?
        .default_layer();
    let upm = project
        .document_font_info(source)
        .and_then(|info| info.metrics.units_per_em)
        .filter(|value| *value > 0.0)
        .unwrap_or(1000.0);
    let threshold = upm * 0.01;
    let mut digest = Sha256::new();
    digest.update(format!("spacing-v1/{}/{upm:?}/{GROUPS:?}", source.0));
    let mut bearings = BTreeMap::new();
    for name in ('A'..='Z').chain('a'..='z') {
        if !GROUPS
            .iter()
            .any(|(reference, _, members)| reference.starts_with(name) || members.contains(name))
        {
            continue;
        }
        let address = GlyphLayerAddress {
            glyph: name.to_string(),
            layer: layer_id.clone(),
        };
        let Some(layer) = project.document_layer(&address.glyph, &layer_id) else {
            continue;
        };
        let advance = layer.width();
        if advance <= 0.0 || !advance.is_finite() {
            continue;
        }
        if layer.codepoints().next().is_some_and(|cp| cp != name) {
            continue;
        }
        let path = project
            .document_layer_path(&address)
            .map_err(|error| format!("could not resolve spacing outline for {name}: {error}"))?;
        if path.is_empty() {
            continue;
        }
        let bounds = path.bounding_box();
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 || !bounds.is_finite() {
            continue;
        }
        digest.update(format!("/{name}/{advance:?}/{}", path.to_svg()));
        bearings.insert(address.glyph, [bounds.x0, advance - bounds.x1]);
    }

    let mut rows = Vec::new();
    for &(reference, side, members) in GROUPS {
        let index = usize::from(side == "right");
        let Some(reference_sides) = bearings.get(reference) else {
            continue;
        };
        let reference_bearing = reference_sides[index];
        for glyph in members.chars().map(|ch| ch.to_string()) {
            let Some(sides) = bearings.get(&glyph) else {
                continue;
            };
            let bearing = sides[index];
            let score = (bearing - reference_bearing).abs();
            if score < threshold {
                continue;
            }
            let row = json!({
                "id": format!("{}:{glyph}:{side}", source.0),
                "glyph": glyph, "side": side, "bearing": bearing,
                "reference": reference, "reference_bearing": reference_bearing, "score": score,
                "text": format!("{reference}{reference}{glyph}{reference}{reference}"),
            });
            rows.push((score, glyph, side, row));
        }
    }
    rows.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.cmp(&b.1))
            .then(a.2.cmp(b.2))
    });
    let total = rows.len();
    for (_, _, _, row) in &rows {
        digest.update(row.to_string());
    }
    let end = offset.saturating_add(limit).min(total);
    let page: Vec<_> = rows
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|r| r.3)
        .collect();
    Ok(json!({
        "source_id": source.0, "snapshot": format!("{:x}", digest.finalize()),
        "total_candidates": total, "offset": offset, "next_offset": (end < total).then_some(end),
        "threshold": threshold, "units_per_em": upm, "rows": page,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::project::SourceInput;

    fn fixture(height: f64) -> Project {
        let mut font = norad::Font::new();
        for (name, left) in [("H", 50.0), ("I", 100.0), ("L", 80.0)] {
            let mut glyph = norad::Glyph::new(name);
            glyph.width = 600.0;
            let mut contour = norad::Contour::default();
            for (x, y) in [(left, 0.0), (500.0, 0.0), (500.0, height), (left, height)] {
                contour.points.push(norad::ContourPoint::new(
                    x,
                    y,
                    norad::PointType::Line,
                    false,
                    None,
                    None,
                ));
            }
            glyph.contours.push(contour);
            font.default_layer_mut().insert_glyph(glyph);
        }
        Project::from_source(SourceInput::from_font(font, "Spacing.ufo".into())).unwrap()
    }

    #[test]
    fn ranking_and_snapshot_are_independent_of_paging() {
        let project = fixture(700.0);
        let first = scan(&project, SourceId(0), 1).unwrap();
        let second = scan_page(&project, SourceId(0), 1, 1).unwrap();
        assert_eq!(first["total_candidates"], 2);
        assert_eq!(first["rows"][0]["id"], "0:I:left");
        assert_eq!(first["rows"][0]["score"], 50.0);
        assert_eq!(first["next_offset"], 1);
        assert_eq!(second["rows"][0]["id"], "0:L:left");
        assert!(second["next_offset"].is_null());
        assert_eq!(first["snapshot"], second["snapshot"]);
        let changed = scan(&fixture(701.0), SourceId(0), 1).unwrap();
        assert_eq!(first["rows"], changed["rows"]);
        assert_ne!(first["snapshot"], changed["snapshot"]);
        assert!(scan(&project, SourceId(99), 1).is_err());
        assert!(scan(&project, SourceId(0), 0).is_err());
    }
}
