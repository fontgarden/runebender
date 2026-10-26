// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Glyphs write-back: merge a saved UFO working copy into the original `.glyphs` file or
//! `.glyphspackage` folder.
//!
//! Each master maps to the working-copy UFO that [`super::glyphs_import`] named for it.
//! A master layer is replaced only when its drawing differs from what the import made of it,
//! so unedited layers keep Glyphs data the import does not read, such as hints and node user
//! data. Glyphs new to the working copy are added and glyphs gone from every master removed.
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
//! - Kerning groups come from the first master. Right-to-left and vertical kerning are not
//!   read or written.
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
use super::glyphs_import::{kern_name, layer_to_norad, master_ufo_name};

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
            .get("public.glyphOrder")
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
    font.glyphs
        .retain(|glyph| keep.contains(glyph.name.as_str()));
    changes.glyph_set = font.glyphs.len() != count;

    let (first_groups, second_groups) = kerning_groups(&masters[0].1.font);
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
        glyph.kern_right = first_groups.get(name).cloned();
        glyph.kern_left = second_groups.get(name).cloned();
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
        }
        if *glyph != snapshot || before.is_none() {
            changes.glyphs.insert(name.clone());
        }
    }

    for (master, working) in &masters {
        let imported: BTreeMap<String, BTreeMap<String, f64>> = original
            .kerning
            .get(master)
            .into_iter()
            .flatten()
            .map(|(left, pairs)| {
                (
                    kern_name(left, true),
                    pairs
                        .iter()
                        .map(|(right, value)| (kern_name(right, false), f64::from(*value)))
                        .collect(),
                )
            })
            .collect();
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
        let kerning = edited
            .into_iter()
            .map(|(left, pairs)| {
                (
                    glyphs_side(&left),
                    pairs
                        .into_iter()
                        .map(|(right, value)| (glyphs_side(&right), narrow(value)))
                        .collect(),
                )
            })
            .collect();
        font.kerning.insert(master.clone(), kerning);
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

fn glyphs_side(side: &str) -> String {
    if let Some(group) = side.strip_prefix("public.kern1.") {
        format!("@MMK_L_{group}")
    } else if let Some(group) = side.strip_prefix("public.kern2.") {
        format!("@MMK_R_{group}")
    } else {
        side.to_owned()
    }
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
        let GlyphsFont::Glyphs3(font) = GlyphsFont::load_str(SOURCE).unwrap() else {
            panic!("a Glyphs 3 source");
        };
        let files = super::super::glyphs_import::glyphs_to_ufo_files(SOURCE).unwrap();
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
}
