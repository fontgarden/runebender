// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Glyphs write-back: merge a saved UFO working copy into the original `.glyphs` file or
//! `.glyphspackage` folder.
//!
//! Each master maps to the working-copy UFO that [`super::glyphs_import`] named for it.
//! A master layer is replaced only when its drawing differs from what the import made of it,
//! so unedited layers keep Glyphs data the import does not read, such as hints and node user
//! data. New glyphs are added and deliberately deleted imported glyphs are removed; glyphs
//! skipped by the original import remain untouched, tracked separately from the working set.
//! Unicode values, kerning groups and kerning follow the working copy when they changed.
//! The rest of the file is kept as glyphslib reads it; a Glyphs 2 file is written as Glyphs 3.
//!
//! # Known gaps
//!
//! Basic alpha support; see also the shared gaps in [`super::foreign`] and what the import
//! does not read in [`super::glyphs_import`].
//!
//! - glyphslib lays out the file differently from Glyphs (arrays on one line, for
//!   example), so the first save of a single `.glyphs` file changes nearly every line in a
//!   diff. A package keeps the original bytes of every file Runebender did not change.
//! - An edited master layer is rebuilt from the UFO glyph. Its hints, node user data, path
//!   attributes and anchor details (locked, orientation, user data) are lost, and its paths
//!   are written before its components whatever the original order was.
//! - Component transforms are decomposed into position, angle, scale and horizontal slant
//!   only; vertical slant and smart-component settings of an edited component are lost.
//! - Kerning groups come from the first master. Vertical kerning is not read or written.
//! - Right-to-left kerning is written back to `kerningRTL`.
//!   A pair keeps the table it came from; a new pair goes to `kerningRTL` when both of its
//!   sides are right-to-left glyphs of the original file, by the rule of
//!   [`super::glyphs_import`], or groups with such a glyph, and to `kerningLTR` otherwise.
//!   A glyph that is new in the working copy is a left-to-right glyph.
//! - Feature code, master metrics, names and other font info edited in Runebender are not
//!   written back, because the import does not read them.
//! - Glyph metadata (category, export flag, production name) is not written back.
//! - A new master layer copies the first layer in the file as a template.
//! - Written files have not yet been opened in Glyphs itself.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use glyphslib::Font as GlyphsFont;
use glyphslib::common::NodeType;
use glyphslib::glyphs3::{
    Anchor, Component, Glyph, Glyphs3, Layer, Node, Path as GlyphsPath, Shape,
};

use super::foreign::{WorkingSource, same_drawing};
use super::glyphs_import::{
    is_backup_layer, kern_name, layer_to_norad, master_ufo_name, rtl_kern_name, rtl_kerning_glyphs,
    ufo_kerning,
};

/// Write `original` with the working copy's edits merged in, at `stage`.
pub fn write_back(original: &Path, sources: &[WorkingSource], stage: &Path) -> Result<(), String> {
    let loaded = GlyphsFont::load(original)
        .map_err(|error| format!("{}: {error}", original.display()))?
        .upgrade();
    let GlyphsFont::Glyphs3(mut font) = loaded else {
        return Err("Glyphs upgrade did not yield a Glyphs 3 font".into());
    };
    let changes = merge(&mut font, sources)?;
    GlyphsFont::Glyphs3(font)
        .save(stage)
        .map_err(|error| format!("{}: {error}", stage.display()))?;
    end_with_newlines(stage)?;
    if original.is_dir() {
        restore_unchanged_package_files(original, stage, &changes)?;
    }
    Ok(())
}

/// Glyphs ends every file with a newline and glyphslib does not; add it.
fn end_with_newlines(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        for entry in
            std::fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))?
        {
            end_with_newlines(
                &entry
                    .map_err(|error| format!("{}: {error}", path.display()))?
                    .path(),
            )?;
        }
        return Ok(());
    }
    let mut text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !text.ends_with('\n') {
        text.push('\n');
        std::fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}

/// What a merge changed, so a package write can keep the original bytes of everything else.
#[derive(Debug, Default)]
struct Changes {
    /// Glyphs added or edited.
    glyphs: HashSet<String>,
    /// Whether glyphs were added or removed, which changes `order.plist`.
    glyph_set: bool,
    /// Whether kerning changed, which lives in `fontinfo.plist`.
    kerning: bool,
}

/// glyphslib writes every file of a package in its own layout; put back the original file
/// wherever Runebender changed nothing, so a version-controlled package shows only real edits.
fn restore_unchanged_package_files(
    original: &Path,
    stage: &Path,
    changes: &Changes,
) -> Result<(), String> {
    let mut keep = vec!["UIState.plist"];
    if !changes.glyph_set {
        keep.push("order.plist");
    }
    if !changes.kerning {
        keep.push("fontinfo.plist");
    }
    for name in keep {
        let from = original.join(name);
        if from.exists() {
            std::fs::copy(&from, stage.join(name))
                .map_err(|error| format!("{}: {error}", from.display()))?;
        }
    }
    let Ok(entries) = std::fs::read_dir(original.join("glyphs")) else {
        return Ok(());
    };
    for entry in entries {
        let path = entry
            .map_err(|error| format!("{}: {error}", original.display()))?
            .path();
        let Some(file_name) = path.file_name() else {
            continue;
        };
        let target = stage.join("glyphs").join(file_name);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let unchanged = glyph_name_in(&text).is_some_and(|name| !changes.glyphs.contains(&name));
        if unchanged && target.exists() {
            std::fs::write(&target, text)
                .map_err(|error| format!("{}: {error}", target.display()))?;
        }
    }
    Ok(())
}

/// The `glyphname` of a package's `.glyph` file.
fn glyph_name_in(text: &str) -> Option<String> {
    let line = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("glyphname = "))?;
    let value = line.strip_suffix(';')?.trim();
    Some(
        value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(value)
            .replace("\\\"", "\"")
            .replace("\\\\", "\\"),
    )
}

fn merge(font: &mut Glyphs3, sources: &[WorkingSource]) -> Result<Changes, String> {
    let mut changes = Changes::default();
    let by_file: HashMap<&str, &WorkingSource> = sources
        .iter()
        .map(|source| (source.filename.as_str(), source))
        .collect();
    let masters: Vec<(String, &WorkingSource)> = font
        .masters
        .iter()
        .map(|master| {
            let filename = master_ufo_name(font, master);
            by_file
                .get(filename.as_str())
                .map(|source| (master.id.clone(), *source))
                .ok_or_else(|| format!("the working copy has no {filename}"))
        })
        .collect::<Result<_, _>>()?;
    if masters.len() != sources.len() {
        return Err(
            "the working copy has sources with no Glyphs master; new masters save only as UFO"
                .into(),
        );
    }
    let imported = sources
        .iter()
        .map(|source| {
            super::metadata::lib_keys::read_foreign_glyphs(&source.font).ok_or(
                "the working copy has no imported glyph inventory; reopen the original source",
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let original = font.clone();
    let template = original
        .glyphs
        .iter()
        .flat_map(|glyph| glyph.layers.first())
        .next()
        .cloned();

    // The glyph set, in the first master's order, with Unicode values from the first master
    // that has the glyph.
    let mut order: Vec<String> = Vec::new();
    let mut codepoints: HashMap<String, Vec<u32>> = HashMap::new();
    for (_, working) in &masters {
        let layer = working.font.default_layer();
        let listed: Vec<&str> = working
            .font
            .lib
            .get(super::metadata::lib_keys::GLYPH_ORDER_KEY)
            .and_then(plist::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(plist::Value::as_string)
            .collect();
        for name in listed
            .into_iter()
            .chain(layer.iter().map(|glyph| glyph.name().as_str()))
        {
            if let Some(glyph) = layer.get_glyph(name)
                && !codepoints.contains_key(name)
            {
                order.push(name.to_owned());
                codepoints.insert(
                    name.to_owned(),
                    glyph.codepoints.iter().map(u32::from).collect(),
                );
            }
        }
    }
    let keep: HashSet<&str> = order.iter().map(String::as_str).collect();
    let count = font.glyphs.len();
    font.glyphs.retain(|glyph| {
        keep.contains(glyph.name.as_str())
            || imported
                .iter()
                .all(|names| names.get(glyph.name.as_str()) == Some(&false))
    });
    changes.glyph_set = font.glyphs.len() != count;

    let (first_groups, second_groups) = kerning_groups(&masters[0].1.font);
    // The right-to-left glyphs of the original file, whose groups are the other way round.
    let rtl = rtl_kerning_glyphs(&original);
    for name in &order {
        let index = match font.glyphs.iter().position(|glyph| glyph.name == *name) {
            Some(index) => index,
            None => {
                font.glyphs.push(Glyph {
                    name: name.clone(),
                    export: true,
                    ..Default::default()
                });
                changes.glyph_set = true;
                font.glyphs.len() - 1
            }
        };
        let before = original.glyphs.iter().find(|glyph| glyph.name == *name);
        let glyph = &mut font.glyphs[index];
        let snapshot = glyph.clone();
        if glyph.unicode != codepoints[name] {
            glyph.unicode.clone_from(&codepoints[name]);
        }
        let (first, second) = if rtl.contains(name) {
            (&mut glyph.kern_left, &mut glyph.kern_right)
        } else {
            (&mut glyph.kern_right, &mut glyph.kern_left)
        };
        *first = first_groups.get(name).cloned();
        *second = second_groups.get(name).cloned();
        for (master, working) in &masters {
            let Some(edited) = working.font.default_layer().get_glyph(name.as_str()) else {
                continue;
            };
            let position = glyph.layers.iter().position(|layer| {
                layer.layer_id == *master && layer.associated_master_id.is_none()
            });
            match position {
                Some(at) => {
                    let unchanged = before
                        .and_then(|before| {
                            before
                                .layers
                                .iter()
                                .find(|layer| {
                                    layer.layer_id == *master
                                        && layer.associated_master_id.is_none()
                                })
                                .map(|layer| (before, layer))
                        })
                        .and_then(|(before, layer)| layer_to_norad(&original, before, layer).ok())
                        .is_some_and(|imported| same_drawing(&imported, edited));
                    if !unchanged {
                        apply(&mut glyph.layers[at], edited);
                    }
                }
                None => {
                    let mut layer = blank_layer(
                        template
                            .as_ref()
                            .ok_or("the Glyphs file has no layer to copy")?,
                        master,
                    );
                    apply(&mut layer, edited);
                    glyph.layers.push(layer);
                }
            }
            sync_backup_layers(&original, before, glyph, master, working, template.as_ref());
        }
        if *glyph != snapshot || before.is_none() {
            changes.glyphs.insert(name.clone());
        }
    }

    let groups = &masters[0].1.font.groups;
    // Whether one side of a UFO pair is right to left: a right-to-left glyph, or a group with one.
    let rtl_side = |side: &str| {
        if side.starts_with("public.kern1.") || side.starts_with("public.kern2.") {
            groups
                .get(side)
                .is_some_and(|members| members.iter().any(|member| rtl.contains(member.as_str())))
        } else {
            rtl.contains(side)
        }
    };
    for (master, working) in &masters {
        let imported: BTreeMap<String, BTreeMap<String, f64>> = ufo_kerning(&original, master)
            .unwrap_or_default()
            .into_iter()
            .map(|(left, pairs)| {
                (
                    left,
                    pairs
                        .into_iter()
                        .map(|(right, value)| (right, f64::from(value)))
                        .collect(),
                )
            })
            .collect();
        let keys = |table: Option<&BTreeMap<String, BTreeMap<String, f32>>>,
                    name: fn(&str, bool) -> String| {
            table
                .into_iter()
                .flatten()
                .flat_map(|(left, pairs)| {
                    pairs
                        .keys()
                        .map(move |right| (name(left, true), name(right, false)))
                })
                .collect::<HashSet<(String, String)>>()
        };
        let was_rtl = keys(original.kerning_rtl.get(master), rtl_kern_name);
        let was_ltr = keys(original.kerning.get(master), kern_name);
        let edited: BTreeMap<String, BTreeMap<String, f64>> = working
            .font
            .kerning
            .iter()
            .map(|(left, pairs)| {
                (
                    left.to_string(),
                    pairs
                        .iter()
                        .map(|(right, value)| (right.to_string(), *value))
                        .collect(),
                )
            })
            .collect();
        if imported == edited {
            continue;
        }
        let mut ltr: BTreeMap<String, BTreeMap<String, f32>> = BTreeMap::new();
        let mut rtl_table: BTreeMap<String, BTreeMap<String, f32>> = BTreeMap::new();
        for (left, pairs) in edited {
            for (right, value) in pairs {
                let key = (left.clone(), right.clone());
                let right_to_left = if was_rtl.contains(&key) {
                    true
                } else if was_ltr.contains(&key) {
                    false
                } else {
                    rtl_side(&left) && rtl_side(&right)
                };
                if right_to_left {
                    rtl_table
                        .entry(glyphs_rtl_side(&left))
                        .or_default()
                        .insert(glyphs_rtl_side(&right), narrow(value));
                } else {
                    ltr.entry(glyphs_side(&left))
                        .or_default()
                        .insert(glyphs_side(&right), narrow(value));
                }
            }
        }
        for (table, values) in [(&mut font.kerning, ltr), (&mut font.kerning_rtl, rtl_table)] {
            if values.is_empty() {
                table.remove(master);
            } else {
                table.insert(master.clone(), values);
            }
        }
        changes.kerning = true;
    }
    Ok(changes)
}

/// Each glyph's kerning group on each side, from the UFO's `public.kern1` and `public.kern2`.
fn kerning_groups(font: &norad::Font) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut first = HashMap::new();
    let mut second = HashMap::new();
    for (group, members) in &font.groups {
        let (side, name) = match (
            group.as_str().strip_prefix("public.kern1."),
            group.as_str().strip_prefix("public.kern2."),
        ) {
            (Some(name), _) => (&mut first, name),
            (_, Some(name)) => (&mut second, name),
            _ => continue,
        };
        for member in members {
            side.insert(member.to_string(), name.to_owned());
        }
    }
    (first, second)
}

/// One side of a right-to-left pair in Glyphs 3: `public.kern1.x` is `@MMK_R_x` and
/// `public.kern2.y` is `@MMK_L_y`.
fn glyphs_rtl_side(side: &str) -> String {
    if let Some(group) = side.strip_prefix("public.kern1.") {
        format!("@MMK_R_{group}")
    } else if let Some(group) = side.strip_prefix("public.kern2.") {
        format!("@MMK_L_{group}")
    } else {
        side.to_owned()
    }
}

fn glyphs_side(side: &str) -> String {
    if let Some(group) = side.strip_prefix("public.kern1.") {
        format!("@MMK_L_{group}")
    } else if let Some(group) = side.strip_prefix("public.kern2.") {
        format!("@MMK_R_{group}")
    } else {
        side.to_owned()
    }
}

/// Make the glyph's backup layers of `master` match the working UFO's extra layers: a layer
/// drawn differently is rebuilt, a new UFO layer is added under the master, and a backup layer
/// the UFO no longer has is removed. The background and proposal layers are not backup layers.
fn sync_backup_layers(
    original: &Glyphs3,
    before: Option<&Glyph>,
    glyph: &mut Glyph,
    master: &str,
    working: &WorkingSource,
    template: Option<&Layer>,
) {
    let name = glyph.name.clone();
    let default_name = working.font.default_layer().name().to_string();
    let mut kept: HashSet<String> = HashSet::new();
    for ufo_layer in working.font.layers.iter() {
        let layer_name = ufo_layer.name().to_string();
        if layer_name == default_name
            || layer_name == "public.background"
            || layer_name.eq_ignore_ascii_case("background")
            || layer_name.starts_with("com.runebender.proposal.")
            || layer_name.starts_with('{')
            || layer_name.starts_with('[')
        {
            continue;
        }
        let Some(edited) = ufo_layer.get_glyph(name.as_str()) else {
            continue;
        };
        kept.insert(layer_name.clone());
        let position = glyph.layers.iter().position(|layer| {
            is_backup_layer(layer, master) && layer.name.as_deref() == Some(layer_name.as_str())
        });
        match position {
            Some(at) => {
                let unchanged = before
                    .and_then(|before| layer_to_norad(original, before, &glyph.layers[at]).ok())
                    .is_some_and(|imported| same_drawing(&imported, edited));
                if !unchanged {
                    apply(&mut glyph.layers[at], edited);
                }
            }
            None => {
                let Some(template) = template else {
                    continue;
                };
                let mut layer = blank_layer(template, master);
                layer.layer_id = backup_layer_id(master, &name, &layer_name);
                layer.associated_master_id = Some(master.to_owned());
                layer.name = Some(layer_name);
                apply(&mut layer, edited);
                glyph.layers.push(layer);
            }
        }
    }
    // A backup layer that was read on open and is gone from the UFO was deleted.
    glyph.layers.retain(|layer| {
        !is_backup_layer(layer, master)
            || layer.name.as_ref().is_some_and(|name| kept.contains(name))
            || before.is_none_or(|before| layer_to_norad(original, before, layer).is_err())
    });
}

/// A stable identifier for a new backup layer, in the form Glyphs uses for layer ids.
fn backup_layer_id(master: &str, glyph: &str, layer: &str) -> String {
    use sha2::{Digest as _, Sha256};
    let digest = Sha256::digest(format!("{master}\n{glyph}\n{layer}").as_bytes());
    let hex: String = digest
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02X}"))
        .collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// A copy of `template` with nothing drawn, as the master layer for `master`.
fn blank_layer(template: &Layer, master: &str) -> Layer {
    let mut layer = template.clone();
    layer.anchors.clear();
    layer.annotations.clear();
    layer.associated_master_id = None;
    layer.attr = BTreeMap::default();
    layer.background = None;
    layer.background_image = None;
    layer.color = None;
    layer.guides.clear();
    layer.hints.clear();
    layer.layer_id = master.to_owned();
    layer.metric_bottom = None;
    layer.metric_left = None;
    layer.metric_right = None;
    layer.metric_top = None;
    layer.metric_vert_origin = None;
    layer.metric_vert_width = None;
    layer.metric_width = None;
    layer.name = None;
    layer.part_selection.clear();
    layer.shapes.clear();
    layer.user_data = BTreeMap::default();
    layer.vert_origin = None;
    layer.vert_width = None;
    layer.width = 0.0;
    layer
}

/// Replace a layer's width, anchors, paths and components with the UFO glyph's.
fn apply(layer: &mut Layer, glyph: &norad::Glyph) {
    layer.width = narrow(glyph.width);
    layer.anchors = glyph
        .anchors
        .iter()
        .map(|anchor| Anchor {
            locked: false,
            name: anchor
                .name
                .as_ref()
                .map(|name| name.as_str().to_owned())
                .unwrap_or_default(),
            orientation: glyphslib::common::Orientation::default(),
            pos: (narrow(anchor.x), narrow(anchor.y)),
            user_data: None,
        })
        .collect();
    let paths = glyph.contours.iter().map(|contour| {
        let closed = contour
            .points
            .first()
            .is_some_and(|point| point.typ != norad::PointType::Move);
        let mut nodes: Vec<Node> = contour
            .points
            .iter()
            .map(|point| Node {
                x: narrow(point.x),
                y: narrow(point.y),
                node_type: node_type(point.typ, point.smooth),
                user_data: None,
            })
            .collect();
        // Glyphs keeps a closed contour's first node last.
        if closed {
            nodes.rotate_left(1);
        }
        Shape::Path(GlyphsPath {
            attr: BTreeMap::default(),
            closed,
            nodes,
        })
    });
    let components = glyph.components.iter().map(|component| {
        let t = component.transform;
        let (angle, scale, slant) = if t.xy_scale == 0.0 && t.yx_scale == 0.0 {
            (0.0, (t.x_scale, t.y_scale), 0.0)
        } else {
            // The import composes rotate, then scale, then slant; undo it in that order.
            let angle = t.xy_scale.atan2(t.x_scale);
            let (sin, cos) = angle.sin_cos();
            let scale_x = t.x_scale.hypot(t.xy_scale);
            let scale_y = cos * t.y_scale - sin * t.yx_scale;
            let shear = (cos * t.yx_scale + sin * t.y_scale) / scale_x;
            (
                angle.to_degrees(),
                (scale_x, scale_y),
                shear.atan().to_degrees(),
            )
        };
        Shape::Component(Component {
            component_glyph: component.base.to_string(),
            position: (narrow(t.x_offset), narrow(t.y_offset)),
            angle: narrow(angle),
            scale: (narrow(scale.0), narrow(scale.1)),
            slant: (narrow(slant), 0.0),
            ..Default::default()
        })
    });
    layer.shapes = paths.chain(components).collect();
}

/// A coordinate in the single precision the Glyphs format stores.
#[expect(
    clippy::cast_possible_truncation,
    reason = "Glyphs files store coordinates and values as single-precision floats"
)]
fn narrow(value: f64) -> f32 {
    value as f32
}

fn node_type(typ: norad::PointType, smooth: bool) -> NodeType {
    match (typ, smooth) {
        (norad::PointType::OffCurve, _) => NodeType::OffCurve,
        (norad::PointType::Curve, false) => NodeType::Curve,
        (norad::PointType::Curve, true) => NodeType::CurveSmooth,
        (norad::PointType::QCurve, false) => NodeType::QCurve,
        (norad::PointType::QCurve, true) => NodeType::QCurveSmooth,
        (_, false) => NodeType::Line,
        (_, true) => NodeType::LineSmooth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"{
.appVersion = "3260";
.formatVersion = 3;
familyName = "Test Sans";
fontMaster = (
{
id = m01;
name = Regular;
}
);
glyphs = (
{
glyphname = A;
layers = (
{
layerId = m01;
shapes = (
{
closed = 1;
nodes = (
(100,0,l),
(0,0,l),
(50,100,l)
);
}
);
width = 500;
}
);
unicode = 65;
},
{
glyphname = B;
layers = (
{
layerId = m01;
hints = (
{
type = Stem;
}
);
width = 400;
}
);
unicode = 66;
}
);
unitsPerEm = 1000;
}
"#;

    fn convert(tag: &str) -> (Glyphs3, Vec<WorkingSource>) {
        convert_source(tag, SOURCE)
    }

    fn convert_source(tag: &str, text: &str) -> (Glyphs3, Vec<WorkingSource>) {
        let GlyphsFont::Glyphs3(font) = GlyphsFont::load_str(text).unwrap() else {
            panic!("a Glyphs 3 source");
        };
        let files = super::super::glyphs_import::glyphs_to_ufo_files(text).unwrap();
        let root = std::env::temp_dir().join(format!("rb-glyphs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for file in &files.files {
            let path = root.join(&file.path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, &file.text).unwrap();
        }
        let sources =
            super::super::foreign::read_working_copy(&root.join("TestSans-Regular.ufo")).unwrap();
        let _ = std::fs::remove_dir_all(&root);
        (font, sources)
    }

    #[test]
    fn an_unedited_working_copy_changes_nothing() {
        let (mut font, sources) = convert("same");
        let original = font.clone();
        let changes = merge(&mut font, &sources).unwrap();
        assert_eq!(font, original);
        assert!(changes.glyphs.is_empty() && !changes.glyph_set && !changes.kerning);
    }

    #[test]
    fn backup_layers_open_as_ufo_layers_and_save_back_edited_added_and_deleted() {
        let text = SOURCE.replace(
            "layerId = m01;\nshapes = (",
            r#"associatedMasterId = m01;
layerId = "AAAA-1";
name = "Nov 15, 26 at 16:00";
shapes = (
{
closed = 1;
nodes = (
(200,0,l),
(0,0,l),
(100,200,l)
);
}
);
width = 620;
},
{
associatedMasterId = m01;
attr = { coordinates = (500); };
layerId = "BBBB-2";
name = "{500}";
width = 510;
},
{
layerId = m01;
shapes = ("#,
        );
        let (mut font, mut sources) = convert_source("backup", &text);
        let before = font.clone();

        // The backup layer is a UFO layer of the same name; the brace layer is not read.
        let ufo = &mut sources[0].font;
        let backup = ufo
            .layers
            .get("Nov 15, 26 at 16:00")
            .expect("the backup layer opens");
        assert_eq!(backup.get_glyph("A").unwrap().width, 620.0);
        assert!(ufo.layers.get("{500}").is_none());
        assert_eq!(ufo.default_layer().get_glyph("A").unwrap().width, 500.0);
        let changes = merge(&mut font, &sources).unwrap();
        assert_eq!(font, before, "an unedited backup layer changes nothing");
        assert!(changes.glyphs.is_empty());

        // Edit the backup layer, and add a second one, as the Layers panel does.
        let ufo = &mut sources[0].font;
        let mut edited = ufo
            .layers
            .get("Nov 15, 26 at 16:00")
            .unwrap()
            .get_glyph("A")
            .unwrap()
            .clone();
        edited.width = 640.0;
        ufo.layers
            .get_mut("Nov 15, 26 at 16:00")
            .unwrap()
            .insert_glyph(edited.clone());
        edited.width = 700.0;
        ufo.layers
            .new_layer("Wide sketch")
            .unwrap()
            .insert_glyph(edited);
        merge(&mut font, &sources).unwrap();
        let glyph = font.glyphs.iter().find(|glyph| glyph.name == "A").unwrap();
        let layer = |name: &str| {
            glyph
                .layers
                .iter()
                .find(|layer| layer.name.as_deref() == Some(name))
        };
        let backup = layer("Nov 15, 26 at 16:00").unwrap();
        assert_eq!(backup.width, 640.0);
        assert_eq!(
            backup.layer_id, "AAAA-1",
            "an edited layer keeps its identity"
        );
        let added = layer("Wide sketch").expect("a new UFO layer is added");
        assert_eq!(added.width, 700.0);
        assert_eq!(added.associated_master_id.as_deref(), Some("m01"));
        assert_ne!(added.layer_id, "m01");
        assert!(layer("{500}").is_some(), "the brace layer is left alone");
        let master = glyph
            .layers
            .iter()
            .find(|layer| layer.layer_id == "m01")
            .unwrap();
        assert_eq!(master.width, 500.0);

        // Deleting the UFO layer deletes the backup layer, and only that.
        sources[0].font.layers.remove("Nov 15, 26 at 16:00");
        merge(&mut font, &sources).unwrap();
        let glyph = font.glyphs.iter().find(|glyph| glyph.name == "A").unwrap();
        let names: Vec<_> = glyph
            .layers
            .iter()
            .filter_map(|l| l.name.as_deref())
            .collect();
        assert!(!names.contains(&"Nov 15, 26 at 16:00"), "{names:?}");
        assert!(
            names.contains(&"Wide sketch") && names.contains(&"{500}"),
            "{names:?}"
        );
    }

    #[test]
    fn skipped_glyphs_survive_but_deliberate_deletions_do_not() {
        let text = SOURCE.replace(
            "glyphs = (",
            r#"glyphs = (
{
glyphname = Alternate;
layers = ({ layerId = alternate; associatedMasterId = m01; width = 321;
hints = ({ type = Stem; }); });
},
{
glyphname = Broken;
layers = ({ layerId = m01; width = 123; shapes = ({ ref = Missing; }); });
},
{
glyphname = Composite;
layers = ({ layerId = m01; width = 400; shapes = ({ ref = B; }); });
},"#,
        );
        let (mut font, mut sources) = convert_source("skipped", &text);
        assert!(sources[0].font.get_glyph("Alternate").is_none());
        assert!(sources[0].font.get_glyph("Broken").is_none());
        assert!(sources[0].font.get_glyph("Composite").is_some());
        let before = font.clone();
        let changes = merge(&mut font, &sources).unwrap();
        assert_eq!(font, before);
        assert!(!changes.glyph_set);

        let layer = sources[0].font.default_layer_mut();
        layer.remove_glyph("B");
        layer.insert_glyph(norad::Glyph::new("New"));
        merge(&mut font, &sources).unwrap();
        assert!(!font.glyphs.iter().any(|glyph| glyph.name == "B"));
        assert!(font.glyphs.iter().any(|glyph| glyph.name == "New"));

        // Deleting the base on the previous save made Composite unimportable now,
        // but the original imported inventory still identifies its deletion.
        let layer = sources[0].font.default_layer_mut();
        layer.remove_glyph("Composite");
        layer.remove_glyph("New");
        merge(&mut font, &sources).unwrap();
        let names: Vec<_> = font
            .glyphs
            .iter()
            .map(|glyph| glyph.name.as_str())
            .collect();
        assert_eq!(names, ["Alternate", "Broken", "A"]);
        for name in ["Alternate", "Broken"] {
            assert_eq!(
                font.glyphs.iter().find(|glyph| glyph.name == name),
                before.glyphs.iter().find(|glyph| glyph.name == name)
            );
        }
    }

    #[test]
    fn package_glyph_names_are_read_back() {
        assert_eq!(glyph_name_in("{\nglyphname = A;\n"), Some("A".into()));
        assert_eq!(
            glyph_name_in("{\nglyphname = \".notdef\";\n"),
            Some(".notdef".into())
        );
    }

    #[test]
    fn edits_replace_only_their_layers() {
        let (mut font, mut sources) = convert("edit");
        let layer = sources[0].font.default_layer_mut();
        layer.get_glyph_mut("A").unwrap().width = 520.0;
        let mut c = norad::Glyph::new("C");
        c.width = 300.0;
        c.codepoints = ['C'].into_iter().collect();
        c.components.push(norad::Component::new(
            norad::Name::new("A").unwrap(),
            norad::AffineTransform {
                x_scale: -1.0,
                xy_scale: 0.0,
                yx_scale: 0.0,
                y_scale: 1.0,
                x_offset: 500.0,
                y_offset: 0.0,
            },
            None,
        ));
        layer.insert_glyph(c);
        layer.remove_glyph("B");
        merge(&mut font, &sources).unwrap();

        let names: Vec<_> = font
            .glyphs
            .iter()
            .map(|glyph| glyph.name.as_str())
            .collect();
        assert_eq!(names, ["A", "C"]);
        let a = &font.glyphs[0].layers[0];
        assert_eq!(a.width, 520.0);
        let Shape::Path(path) = &a.shapes[0] else {
            panic!("a path")
        };
        assert_eq!((path.nodes[2].x, path.nodes[2].y), (50.0, 100.0));
        let c = &font.glyphs[1];
        assert_eq!(c.unicode, [67]);
        assert!(c.export);
        let Shape::Component(component) = &c.layers[0].shapes[0] else {
            panic!("a component")
        };
        assert_eq!(
            (component.scale, component.position),
            ((-1.0, 1.0), (500.0, 0.0))
        );
        assert_eq!(c.layers[0].layer_id, "m01");
    }

    /// Glyphs 3 right-to-left kerning: reh is the first glyph of the pair and kerns with its
    /// left side, so its left group is its first-side group.
    const RTL_SOURCE: &str = r#"{
.appVersion = "3260";
.formatVersion = 3;
familyName = "Test Sans";
fontMaster = (
{
id = m01;
name = Regular;
}
);
glyphs = (
{
glyphname = "reh-ar";
kernLeft = reh;
layers = (
{
layerId = m01;
width = 400;
}
);
unicode = 1585;
},
{
glyphname = "alef-ar";
kernRight = alef;
layers = (
{
layerId = m01;
width = 200;
}
);
unicode = 1575;
},
{
glyphname = A;
kernLeft = A;
kernRight = A;
layers = (
{
layerId = m01;
width = 500;
}
);
unicode = 65;
}
);
kerningLTR = {
m01 = {
"@MMK_L_A" = {
"@MMK_R_A" = -20;
};
};
};
kerningRTL = {
m01 = {
"@MMK_R_reh" = {
"@MMK_L_alef" = -60;
};
};
};
unitsPerEm = 1000;
}"#;

    /// The UFO kerning of a converted source, as (first, second, value).
    fn ufo_pairs(text: &str) -> Vec<(String, String, f64)> {
        let files = super::super::glyphs_import::glyphs_to_ufo_files(text).unwrap();
        let kerning = files
            .files
            .iter()
            .find(|file| file.path == "TestSans-Regular.ufo/kerning.plist")
            .expect("a kerning plist");
        let value: plist::Dictionary = plist::from_bytes(kerning.text.as_bytes()).unwrap();
        let mut pairs = Vec::new();
        for (first, row) in value {
            for (second, value) in row.into_dictionary().unwrap() {
                pairs.push((first.clone(), second, value.as_real().unwrap()));
            }
        }
        pairs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        pairs
    }

    #[test]
    fn right_to_left_kerning_imports_and_writes_back() {
        // The import merges both tables, with the groups of a right-to-left glyph swapped.
        assert_eq!(
            ufo_pairs(RTL_SOURCE),
            [
                ("public.kern1.A".into(), "public.kern2.A".into(), -20.0),
                ("public.kern1.reh".into(), "public.kern2.alef".into(), -60.0),
            ]
        );
        let files = super::super::glyphs_import::glyphs_to_ufo_files(RTL_SOURCE).unwrap();
        let groups = files
            .files
            .iter()
            .find(|file| file.path == "TestSans-Regular.ufo/groups.plist")
            .unwrap();
        let groups: plist::Dictionary = plist::from_bytes(groups.text.as_bytes()).unwrap();
        let members = |group: &str| -> Vec<String> {
            groups
                .get(group)
                .and_then(|value| value.as_array())
                .map(|members| {
                    members
                        .iter()
                        .filter_map(|member| member.as_string().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        assert_eq!(members("public.kern1.reh"), ["reh-ar"]);
        assert_eq!(members("public.kern2.alef"), ["alef-ar"]);
        assert_eq!(members("public.kern2.reh"), Vec::<String>::new());

        // An unedited working copy changes nothing.
        let (mut font, sources) = convert_source("rtl-same", RTL_SOURCE);
        let original = font.clone();
        let changes = merge(&mut font, &sources).unwrap();
        assert_eq!(font, original);
        assert!(!changes.kerning);

        // An edited value stays right to left, a new pair of right-to-left glyphs goes to
        // `kerningRTL`, and a new Latin pair to `kerningLTR`.
        let (mut font, mut sources) = convert_source("rtl-edit", RTL_SOURCE);
        let name = |text: &str| norad::Name::new(text).unwrap();
        let kerning = &mut sources[0].font.kerning;
        kerning
            .get_mut(&name("public.kern1.reh"))
            .unwrap()
            .insert(name("public.kern2.alef"), -80.0);
        kerning
            .entry(name("reh-ar"))
            .or_default()
            .insert(name("alef-ar"), -10.0);
        kerning
            .entry(name("A"))
            .or_default()
            .insert(name("A"), -5.0);
        let changes = merge(&mut font, &sources).unwrap();
        assert!(changes.kerning);
        let table = |kerning: &BTreeMap<String, BTreeMap<String, BTreeMap<String, f32>>>| {
            kerning
                .get("m01")
                .into_iter()
                .flatten()
                .flat_map(|(first, row)| {
                    row.iter()
                        .map(move |(second, value)| (first.clone(), second.clone(), *value))
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            table(&font.kerning_rtl),
            [
                ("@MMK_R_reh".to_owned(), "@MMK_L_alef".to_owned(), -80.0),
                ("reh-ar".to_owned(), "alef-ar".to_owned(), -10.0),
            ]
        );
        assert_eq!(
            table(&font.kerning),
            [
                ("@MMK_L_A".to_owned(), "@MMK_R_A".to_owned(), -20.0),
                ("A".to_owned(), "A".to_owned(), -5.0),
            ]
        );
        // The groups of the right-to-left glyph keep their sides.
        let reh = font
            .glyphs
            .iter()
            .find(|glyph| glyph.name == "reh-ar")
            .unwrap();
        assert_eq!(
            (reh.kern_left.as_deref(), reh.kern_right.as_deref()),
            (Some("reh"), None)
        );

        // The written file opens with the edited values.
        let path =
            std::env::temp_dir().join(format!("rb-glyphs-rtl-{}.glyphs", std::process::id()));
        GlyphsFont::Glyphs3(font).save(&path).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            ufo_pairs(&written),
            [
                ("A".into(), "A".into(), -5.0),
                ("public.kern1.A".into(), "public.kern2.A".into(), -20.0),
                ("public.kern1.reh".into(), "public.kern2.alef".into(), -80.0),
                ("reh-ar".into(), "alef-ar".into(), -10.0),
            ]
        );
    }
}
