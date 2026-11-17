// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runebender's own `com.runebender.*` lib keys and public glyph metadata.
//!
//! Masks, HOI intermediates, and the keys of foreign and proposal sources.
//! Each key has one reader and one writer here, so the on-disk format
//! is defined in one place.

use std::collections::HashSet;

/// Font lib key recording which original foreign-source glyphs were imported.
pub const FOREIGN_GLYPHS_KEY: &str = "com.runebender.foreignGlyphs";

/// Read original glyph import status, independently of the current glyph inventory.
pub fn read_foreign_glyphs(font: &norad::Font) -> Option<std::collections::HashMap<&str, bool>> {
    font.lib
        .get(FOREIGN_GLYPHS_KEY)?
        .as_dictionary()?
        .iter()
        .map(|(name, imported)| Some((name.as_str(), imported.as_boolean()?)))
        .collect()
}

/// Record which original glyphs were imported, including an empty inventory.
pub fn write_foreign_glyphs<'a>(
    lib: &mut plist::Dictionary,
    imported: impl IntoIterator<Item = &'a str>,
    original: impl IntoIterator<Item = &'a str>,
) {
    let imported: HashSet<_> = imported.into_iter().collect();
    lib.insert(
        FOREIGN_GLYPHS_KEY.into(),
        plist::Value::Dictionary(
            original
                .into_iter()
                .map(|name| {
                    (
                        name.to_owned(),
                        plist::Value::Boolean(imported.contains(name)),
                    )
                })
                .collect(),
        ),
    );
}

/// Component object-lib key holding its original Fontra payload.
pub const FONTRA_COMPONENT_KEY: &str = "com.runebender.fontraComponent";

/// Read preserved Fontra data only while this remains the same component base.
pub fn read_fontra_component(component: &norad::Component) -> Option<serde_json::Value> {
    let text = component.lib()?.get(FONTRA_COMPONENT_KEY)?.as_string()?;
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    (value["name"].as_str() == Some(component.base.as_str())).then_some(value)
}

/// Bind unsupported Fontra data to the component's own stable UFO identity.
pub fn write_fontra_component(component: &mut norad::Component, value: &serde_json::Value) {
    let mut lib = component.lib().cloned().unwrap_or_default();
    lib.insert(FONTRA_COMPONENT_KEY.into(), value.to_string().into());
    component.replace_lib(lib);
}

/// Original Python Babelfont layer identity retained when importing into UFO.
pub const BABELFONT_LAYER_KEY: &str = "com.runebender.babelfontLayer";

/// Read the source id, layer id and background status of an imported layer.
pub fn read_babelfont_layer(glyph: &norad::Glyph) -> Option<(&str, Option<&str>, bool)> {
    let value = glyph.lib.get(BABELFONT_LAYER_KEY)?.as_dictionary()?;
    Some((
        value.get("source")?.as_string()?,
        value.get("id").and_then(plist::Value::as_string),
        value.get("background")?.as_boolean()?,
    ))
}

/// Preserve the original identity without treating it as an editable UFO identifier.
pub fn write_babelfont_layer(
    glyph: &mut norad::Glyph,
    source: &str,
    id: Option<&str>,
    background: bool,
) {
    let mut value = plist::Dictionary::new();
    value.insert("source".into(), source.into());
    if let Some(id) = id {
        value.insert("id".into(), id.into());
    }
    value.insert("background".into(), background.into());
    glyph.lib.insert(BABELFONT_LAYER_KEY.into(), value.into());
}

/// Proposal glyph lib key recording its foreground revision and design intent.
pub const PROPOSAL_BASE_KEY: &str = "com.runebender.proposalBase";

/// Read a proposal's base revision. `None` means a legacy, unguarded proposal.
/// A malformed record returns an empty revision so installation fails closed.
pub fn read_proposal_base(glyph: &norad::Glyph) -> Option<&str> {
    glyph.lib.get(PROPOSAL_BASE_KEY).map(|value| {
        value
            .as_dictionary()
            .and_then(|d| d.get("revision"))
            .and_then(plist::Value::as_string)
            .unwrap_or("")
    })
}

/// Record the original glyph revision and the reason for a proposed edit.
pub fn write_proposal_base(glyph: &mut norad::Glyph, revision: &str, reason: &str) {
    let mut record = plist::Dictionary::new();
    record.insert("revision".into(), revision.into());
    record.insert("reason".into(), reason.into());
    glyph.lib.insert(PROPOSAL_BASE_KEY.into(), record.into());
}

/// Glyph lib key for the contour indices marked as masks.
///
/// A mask is a shape that cuts away from the rest of the glyph.
/// Previews subtract masks live. Bake Masks makes the subtraction
/// real, because external compilers only see baked outlines.
pub const MASKS_KEY: &str = "com.runebender.masks";

/// Reads the mask contour indices from the glyph lib. Negative or non-integer entries are skipped.
pub fn read_masks(glyph: &norad::Glyph) -> HashSet<usize> {
    glyph
        .lib
        .get(MASKS_KEY)
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|v| v.as_signed_integer())
                .filter_map(|i| usize::try_from(i).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Writes the mask contour indices to the glyph lib in sorted order. An empty set removes the key.
pub fn write_masks(glyph: &mut norad::Glyph, masks: &HashSet<usize>) {
    if masks.is_empty() {
        glyph.lib.remove(MASKS_KEY);
        return;
    }
    let mut sorted: Vec<usize> = masks.iter().copied().collect();
    sorted.sort_unstable();
    glyph.lib.insert(
        MASKS_KEY.into(),
        plist::Value::Array(
            sorted
                .into_iter()
                .map(|i| plist::Value::Integer((i as u64).into()))
                .collect(),
        ),
    );
}

/// UFO-standard ordered glyph names.
pub const GLYPH_ORDER_KEY: &str = "public.glyphOrder";

/// UFO-standard glyph name -> production name mapping.
///
/// ufo2ft and fontc consume it at compile time.
pub const PSNAMES_KEY: &str = "public.postscriptNames";

/// Looks up the production name for `glyph` in `public.postscriptNames`, if set.
pub fn read_production_name(font: &norad::Font, glyph: &str) -> Option<String> {
    match font.lib.get(PSNAMES_KEY)? {
        plist::Value::Dictionary(d) => d.get(glyph)?.as_string().map(str::to_string),
        _ => None,
    }
}

/// Rename references without rebuilding lib values or changing production names.
///
/// Malformed containers and affected production values reject the operation, as do
/// occupied destination entries. Unrelated opaque values, including non-string
/// glyph-order entries, are retained verbatim.
pub(crate) fn rename_glyph_lib_references(
    lib: &mut norad::Plist,
    old: &str,
    new: &str,
) -> Result<bool, String> {
    if old == new {
        return Ok(false);
    }
    update_glyph_lib_references(lib, old, Some(new))
}

/// Remove only the named glyph's order entries and production-name mapping.
pub(crate) fn remove_glyph_lib_references(
    lib: &mut norad::Plist,
    name: &str,
) -> Result<bool, String> {
    update_glyph_lib_references(lib, name, None)
}

fn update_glyph_lib_references(
    lib: &mut norad::Plist,
    old: &str,
    new: Option<&str>,
) -> Result<bool, String> {
    // Validate both keys before changing either: even direct codec callers get
    // an unchanged lib on error.
    let has_order_entry = match lib.get(GLYPH_ORDER_KEY) {
        None => false,
        Some(plist::Value::Array(order)) => {
            let present = order.iter().any(|value| value.as_string() == Some(old));
            if present
                && new.is_some_and(|new| order.iter().any(|value| value.as_string() == Some(new)))
            {
                return Err(format!(
                    "{GLYPH_ORDER_KEY} already contains the destination glyph"
                ));
            }
            present
        }
        Some(_) => return Err(format!("{GLYPH_ORDER_KEY} must be an array")),
    };
    let has_production_name = match lib.get(PSNAMES_KEY) {
        None => false,
        Some(plist::Value::Dictionary(names)) => {
            if let Some(value) = names.get(old) {
                if value.as_string().is_none() {
                    return Err(format!("{PSNAMES_KEY} entry for {old:?} must be a string"));
                }
                if new.is_some_and(|new| names.contains_key(new)) {
                    return Err(format!(
                        "{PSNAMES_KEY} already contains the destination glyph"
                    ));
                }
                true
            } else {
                false
            }
        }
        Some(_) => return Err(format!("{PSNAMES_KEY} must be a dictionary")),
    };

    if has_order_entry && let Some(plist::Value::Array(order)) = lib.get_mut(GLYPH_ORDER_KEY) {
        if let Some(new) = new {
            for value in order {
                if value.as_string() == Some(old) {
                    *value = plist::Value::String(new.to_owned());
                }
            }
        } else {
            order.retain(|value| value.as_string() != Some(old));
        }
    }
    if has_production_name && let Some(plist::Value::Dictionary(names)) = lib.get_mut(PSNAMES_KEY) {
        if let Some(value) = names.remove(old)
            && let Some(new) = new
        {
            names.insert(new.to_owned(), value);
        }
        if names.is_empty() {
            lib.remove(PSNAMES_KEY);
        }
    }
    Ok(has_order_entry || has_production_name)
}

/// Glyph lib key for per-node HOI intermediate points.
///
/// An intermediate point makes the node's interpolation path curve
/// through it at the axis middle. This is the "Intermediate Point"
/// in Glyphs.
///
/// Points are stored on the axis-min master's glyph, in absolute
/// design coordinates, keyed `"contour,point"`. The key is the
/// source of truth for re-editing; compilers consume the baked
/// brace layers.
pub use crate::font::model::hoi::HOI_INTERMEDIATE_KEY;

/// Reads HOI intermediate points from the glyph lib, keyed by `(contour, point)` index, as absolute design coordinates.
pub fn read_hoi_intermediates(
    glyph: &norad::Glyph,
) -> std::collections::HashMap<(usize, usize), (f64, f64)> {
    glyph
        .lib
        .get(HOI_INTERMEDIATE_KEY)
        .and_then(|value| crate::font::model::hoi::HoiIntermediates::from_plist(value).ok())
        .map(|value| value.points().collect())
        .unwrap_or_default()
}

/// Writes HOI intermediate points to the glyph lib. An empty map removes the key.
pub fn write_hoi_intermediates(
    glyph: &mut norad::Glyph,
    map: &std::collections::HashMap<(usize, usize), (f64, f64)>,
) {
    crate::font::model::hoi::HoiIntermediates::from_points(
        map.iter().map(|(index, point)| (*index, *point)),
    )
    .expect("HOI source points must be finite")
    .write_to_lib(&mut glyph.lib);
}

/// Quadratic through Q at the middle: position at `t` between `a`
/// and `b` when the path must pass through `q` at t = 0.5.
pub fn hoi_quad_at(a: (f64, f64), b: (f64, f64), q: (f64, f64), t: f64) -> (f64, f64) {
    // Control C with (1-t)²A + 2(1-t)tC + t²B passing Q at 0.5:
    // Q = A/4 + C/2 + B/4  =>  C = 2Q - (A+B)/2.
    let c = (2.0 * q.0 - (a.0 + b.0) / 2.0, 2.0 * q.1 - (a.1 + b.1) / 2.0);
    let u = 1.0 - t;
    (
        u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0,
        u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_names_read_from_lib() {
        let mut font = norad::Font::new();
        assert_eq!(read_production_name(&font, "uni0627"), None);
        let mut dict = plist::Dictionary::new();
        dict.insert("alef-ar".into(), plist::Value::String("uni0627".into()));
        font.lib
            .insert(PSNAMES_KEY.into(), plist::Value::Dictionary(dict));
        assert_eq!(
            read_production_name(&font, "alef-ar").as_deref(),
            Some("uni0627")
        );
        assert_eq!(read_production_name(&font, "beh-ar"), None);
    }
    /// Saved filters and annotations were written by earlier editors.
    /// Nothing reads them now, but a font that carries them keeps them through a save.
    #[test]
    fn retired_saved_filter_and_annotation_keys_survive_a_save() {
        use crate::font::project::Project;
        use crate::font::variable::SourceId;

        let path = std::env::temp_dir().join(format!(
            "runebender-retired-lib-keys-{}.ufo",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        let mut filter = plist::Dictionary::new();
        filter.insert("name".into(), plist::Value::String("wide".into()));
        filter.insert("query".into(), plist::Value::String("w>600".into()));
        let filters = plist::Value::Array(vec![plist::Value::Dictionary(filter)]);
        let mut note = plist::Dictionary::new();
        note.insert("kind".into(), plist::Value::String("note".into()));
        note.insert("x".into(), plist::Value::Real(-5.0));
        note.insert("y".into(), plist::Value::Real(700.0));
        note.insert("text".into(), plist::Value::String("fix this join".into()));
        let notes = plist::Value::Array(vec![plist::Value::Dictionary(note)]);
        let mut font = norad::Font::new();
        font.lib
            .insert("com.runebender.savedFilters".into(), filters.clone());
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.0;
        glyph
            .lib
            .insert("com.runebender.annotations".into(), notes.clone());
        font.default_layer_mut().insert_glyph(glyph);
        font.save(&path).unwrap();

        let mut project = Project::load(&path).unwrap();
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        project
            .edit_document_layer("A", &layer, |draft| {
                draft.set_width(612.0)?;
                Ok(())
            })
            .unwrap();
        project.save().unwrap();

        let saved = norad::Font::load(&path).unwrap();
        let glyph = saved.get_glyph("A").unwrap();
        assert_eq!(glyph.width, 612.0);
        assert_eq!(saved.lib.get("com.runebender.savedFilters"), Some(&filters));
        assert_eq!(glyph.lib.get("com.runebender.annotations"), Some(&notes));
        std::fs::remove_dir_all(&path).unwrap();
    }
    #[test]
    fn masks_roundtrip() {
        use norad::{Contour, ContourPoint, PointType};
        let square = |x0: f64, y0: f64, x1: f64, y1: f64| {
            Contour::new(
                [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
                    .iter()
                    .map(|&(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None))
                    .collect(),
                None,
            )
        };
        let mut glyph = norad::Glyph::new("mask-test");
        glyph.contours = vec![
            square(0.0, 0.0, 100.0, 100.0),
            square(60.0, 20.0, 140.0, 80.0),
        ];
        let mut masks = HashSet::new();
        masks.insert(1_usize);
        write_masks(&mut glyph, &masks);
        assert_eq!(read_masks(&glyph), masks);
        write_masks(&mut glyph, &HashSet::new());
        assert!(read_masks(&glyph).is_empty());
    }
    #[test]
    fn hoi_quad_passes_through_the_intermediate() {
        let a = (0.0, 0.0);
        let b = (100.0, 0.0);
        let q = (50.0, 40.0);
        assert_eq!(hoi_quad_at(a, b, q, 0.0), a);
        assert_eq!(hoi_quad_at(a, b, q, 1.0), b);
        assert_eq!(hoi_quad_at(a, b, q, 0.5), q);
        // Quarter stop, worked by hand: control C = (50, 80).
        let (x, y) = hoi_quad_at(a, b, q, 0.25);
        assert!((x - 25.0).abs() < 1e-9 && (y - 30.0).abs() < 1e-9);
    }
    #[test]
    fn hoi_intermediates_roundtrip_the_lib_key() {
        let mut glyph = norad::Glyph::new("hoi-store");
        let mut map = std::collections::HashMap::new();
        map.insert((0_usize, 3_usize), (166.0, 73.0));
        map.insert((2, 0), (-12.0, 400.0));
        write_hoi_intermediates(&mut glyph, &map);
        assert_eq!(read_hoi_intermediates(&glyph), map);
        // Empty map clears the key.
        write_hoi_intermediates(&mut glyph, &std::collections::HashMap::new());
        assert!(glyph.lib.get(HOI_INTERMEDIATE_KEY).is_none());
    }
}
