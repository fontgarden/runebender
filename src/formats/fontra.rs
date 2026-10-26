// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Fontra `.fontra` folders.
//!
//! [`to_ufo_files`] converts a folder to UFO sources and a designspace for the editor to open.
//! [`write_back`] merges a saved working copy into a copy of the original folder: changed glyph
//! layers, the glyph set and Unicode values, kerning, feature text and line metrics.
//! A glyph file whose drawings did not change keeps its original text.
//!
//! Read: continuous axes and their mappings, full font sources, outlines, components, anchors,
//! advance widths, kerning groups and values, and feature text.
//! Not read: sparse sources, glyph layers between sources, glyph axes and component locations.
//! They stay in the folder on write-back and are reported as warnings when the folder opens.
//! The folder layout follows Fontra's own backend, `fontra/backends/fontra.py`.
//!
//! babelfont-rs has a Fontra loader, but at the pinned revision it reads no outlines and
//! reads kerning and locations from the wrong places, so this module does not use it.
//!
//! # Known gaps
//!
//! Basic alpha support; see also the shared gaps in [`super::foreign`].
//!
//! - Discrete axes open at their default value only, and sources at other values are skipped.
//!   Cross-axis mappings are ignored.
//! - Sparse sources, glyph sources between font sources, glyph axes, and the location of
//!   variable components are not read, so they cannot be edited. Write-back leaves them as
//!   they were, but a moved or rescaled component keeps its old location.
//! - Only the `kern` kerning table is read. Kerning groups come from the first source and
//!   other UFO groups are dropped. Pair rows are written in sorted order, not file order.
//! - Feature text comes from the first source only. Conditional substitutions are not read.
//! - Font info, axes and source names edited in Runebender are not written back; of the
//!   font data, only line metrics and the italic angle are.
//! - `serde_json` here sorts object keys, so an edited glyph file comes back with its keys
//!   in a different order. Unedited glyph files keep their original bytes.
//! - Background images and the glyph-info columns after the code points are kept as they
//!   were, but they cannot be edited.
//! - Written folders have not yet been opened in Fontra itself.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use kurbo::Affine;
use serde::Serialize as _;
use serde_json::{Map, Value, json};

use super::foreign::{WorkingSource, same_drawing};
use super::glyphs_import::{ConversionResult, ConvertedFile};
use crate::outline::glyph_paths::round_units;

const FONT_DATA: &str = "font-data.json";
const GLYPH_INFO: &str = "glyph-info.csv";
const KERNING: &str = "kerning.csv";
const FEATURES: &str = "features.txt";
const GLYPHS: &str = "glyphs";
const KERN: &str = "kern";

/// A value in the single precision a designspace stores.
#[expect(
    clippy::cast_possible_truncation,
    reason = "norad stores designspace locations as single-precision floats"
)]
fn narrow(value: f64) -> f32 {
    value as f32
}

/// A continuous font axis, in user coordinates with its map to design coordinates.
struct Axis {
    name: String,
    tag: String,
    minimum: f64,
    default: f64,
    maximum: f64,
    hidden: bool,
    /// `[user, design]` pairs.
    mapping: Vec<(f64, f64)>,
}

impl Axis {
    fn to_design(&self, user: f64) -> f64 {
        piecewise(&self.mapping, user)
    }
}

/// Map `value` through sorted `(input, output)` pairs, linearly between them.
fn piecewise(mapping: &[(f64, f64)], value: f64) -> f64 {
    let Some(first) = mapping.first() else {
        return value;
    };
    if value <= first.0 {
        return first.1 + (value - first.0);
    }
    for pair in mapping.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        if value <= x1 {
            let span = x1 - x0;
            return if span.abs() < f64::EPSILON {
                y0
            } else {
                y0 + (value - x0) / span * (y1 - y0)
            };
        }
    }
    let last = mapping[mapping.len() - 1];
    last.1 + (value - last.0)
}

/// A full font source: its identifier, display name and design location by axis name.
struct FontSource {
    id: String,
    name: String,
    location: BTreeMap<String, f64>,
}

/// What `font-data.json` says, read loosely so unknown keys survive a write-back.
struct FontData {
    data: Value,
    units_per_em: f64,
    axes: Vec<Axis>,
    /// Discrete axes and their default values; only sources at the default are read.
    fixed: Vec<(String, f64)>,
    sources: Vec<FontSource>,
    warnings: Vec<String>,
}

impl FontData {
    fn read(folder: &Path) -> Result<Self, String> {
        let path = folder.join(FONT_DATA);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let data: Value =
            serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        let order: SourceOrder =
            serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        let mut warnings = Vec::new();
        let units_per_em = data["unitsPerEm"].as_f64().unwrap_or(1000.0);
        let mut axes = Vec::new();
        let mut fixed = Vec::new();
        for axis in data["axes"]["axes"].as_array().into_iter().flatten() {
            let name = axis["name"]
                .as_str()
                .ok_or("axis without a name")?
                .to_owned();
            if axis.get("values").is_some() {
                let default = axis["defaultValue"].as_f64().unwrap_or_default();
                warnings.push(format!(
                    "discrete axis {name}: only sources at its default value {default} are read"
                ));
                fixed.push((name, default));
                continue;
            }
            let number = |key: &str| {
                axis[key]
                    .as_f64()
                    .ok_or_else(|| format!("axis {name} needs {key}"))
            };
            let mut mapping: Vec<(f64, f64)> = axis["mapping"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|pair| Some((pair[0].as_f64()?, pair[1].as_f64()?)))
                .collect();
            mapping.sort_by(|a, b| a.0.total_cmp(&b.0));
            axes.push(Axis {
                tag: axis["tag"].as_str().unwrap_or(&name).to_owned(),
                minimum: number("minValue")?,
                default: number("defaultValue")?,
                maximum: number("maxValue")?,
                hidden: axis["hidden"].as_bool().unwrap_or(false),
                mapping,
                name,
            });
        }
        if data["axes"]["mappings"]
            .as_array()
            .is_some_and(|mappings| !mappings.is_empty())
        {
            warnings.push("cross-axis mappings are not read".into());
        }
        let default_location: BTreeMap<String, f64> = axes
            .iter()
            .map(|axis| (axis.name.clone(), axis.to_design(axis.default)))
            .collect();
        let mut sources = Vec::new();
        for (id, source) in data["sources"].as_object().into_iter().flatten() {
            if source["isSparse"].as_bool().unwrap_or(false) {
                warnings.push(format!("sparse source {id} is not read"));
                continue;
            }
            if !at_fixed_defaults(&fixed, &source["location"]) {
                warnings.push(format!(
                    "source {id} is off a discrete axis default and is not read"
                ));
                continue;
            }
            let mut location = default_location.clone();
            for (axis, value) in source["location"].as_object().into_iter().flatten() {
                if let (Some(slot), Some(value)) = (location.get_mut(axis), value.as_f64()) {
                    *slot = value;
                }
            }
            sources.push(FontSource {
                id: id.clone(),
                name: source["name"].as_str().unwrap_or(id).to_owned(),
                location,
            });
        }
        sources.sort_by_key(|source| order.sources.iter().position(|id| *id == source.id));
        if sources.is_empty() {
            // Older folders keep sources only in glyphs; the default location stands in.
            sources.push(FontSource {
                id: String::new(),
                name: "Regular".into(),
                location: default_location,
            });
        }
        Ok(Self {
            data,
            units_per_em,
            axes,
            fixed,
            sources,
            warnings,
        })
    }

    fn full_location(&self, location: &Value) -> BTreeMap<String, f64> {
        self.axes
            .iter()
            .map(|axis| {
                let value = location[&axis.name]
                    .as_f64()
                    .unwrap_or_else(|| axis.to_design(axis.default));
                (axis.name.clone(), value)
            })
            .collect()
    }

    /// The layer name of the glyph source drawn at `source`, if the glyph has one.
    fn layer_for<'a>(&self, glyph: &'a Value, source: &FontSource) -> Option<&'a str> {
        let sources = glyph["sources"].as_array()?;
        let active = || {
            sources
                .iter()
                .filter(|s| !s["inactive"].as_bool().unwrap_or(false))
        };
        if !source.id.is_empty()
            && let Some(found) = active().find(|s| s["locationBase"].as_str() == Some(&source.id))
        {
            return found["layerName"].as_str();
        }
        active()
            .find(|s| {
                s["locationBase"].as_str().is_none()
                    && at_fixed_defaults(&self.fixed, &s["location"])
                    && same_location(&self.full_location(&s["location"]), &source.location)
            })
            .and_then(|s| s["layerName"].as_str())
    }
}

/// Whether a location sits at every discrete axis's default, a missing axis counting as there.
fn at_fixed_defaults(fixed: &[(String, f64)], location: &Value) -> bool {
    fixed.iter().all(|(axis, default)| {
        location[axis]
            .as_f64()
            .is_none_or(|value| (value - default).abs() < 1e-6)
    })
}

/// The source identifiers in file order, which `Value` does not keep.
#[derive(serde::Deserialize)]
struct SourceOrder {
    #[serde(default, deserialize_with = "keys_in_order")]
    sources: Vec<String>,
}

fn keys_in_order<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    struct Keys;
    impl<'de> serde::de::Visitor<'de> for Keys {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an object")
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut keys = Vec::new();
            while let Some((key, serde::de::IgnoredAny)) = map.next_entry::<String, _>()? {
                keys.push(key);
            }
            Ok(keys)
        }
    }
    deserializer.deserialize_map(Keys)
}

fn same_location(a: &BTreeMap<String, f64>, b: &BTreeMap<String, f64>) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|((k, x), (l, y))| k == l && (x - y).abs() < 1e-6)
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}

/// Serialize the way Fontra does: `json.dumps(indent=0, ensure_ascii=False)` and a newline.
fn fontra_json(value: &Value) -> String {
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"");
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
    value
        .serialize(&mut serializer)
        .expect("a JSON value always serializes");
    let mut text = String::from_utf8(out).expect("serde_json writes UTF-8");
    text.push('\n');
    text
}

/// A whole number as an integer, so files read `500` rather than `500.0`.
fn number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        json!(round_units(value))
    } else {
        json!(value)
    }
}

/// The glyph map: glyph names in file order with their code points, and the rest of each row.
struct GlyphInfo {
    header: Vec<String>,
    rows: Vec<(String, Vec<char>, Vec<String>)>,
}

impl GlyphInfo {
    fn read(folder: &Path) -> Result<Self, String> {
        let path = folder.join(GLYPH_INFO);
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b';')
            .has_headers(false)
            .flexible(true)
            .from_path(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let mut records = reader.records();
        let header: Vec<String> = match records.next() {
            Some(record) => record
                .map_err(|error| format!("{}: {error}", path.display()))?
                .iter()
                .map(str::to_owned)
                .collect(),
            None => vec!["glyph name".into(), "code points".into()],
        };
        let mut rows = Vec::new();
        for record in records {
            let record = record.map_err(|error| format!("{}: {error}", path.display()))?;
            let mut cells = record.iter();
            let Some(name) = cells.next().filter(|name| !name.is_empty()) else {
                continue;
            };
            let codepoints = cells
                .next()
                .unwrap_or_default()
                .split(',')
                .filter_map(|cell| {
                    let hex = cell.trim().trim_start_matches("U+");
                    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
                })
                .collect();
            rows.push((
                name.to_owned(),
                codepoints,
                cells.map(str::to_owned).collect(),
            ));
        }
        Ok(Self { header, rows })
    }
}

/// Every glyph file in the folder, by glyph name: its path and contents.
fn read_glyph_files(
    folder: &Path,
) -> Result<BTreeMap<String, (std::path::PathBuf, Value)>, String> {
    let mut glyphs = BTreeMap::new();
    let directory = folder.join(GLYPHS);
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Ok(glyphs);
    };
    for entry in entries {
        let path = entry
            .map_err(|error| format!("{}: {error}", directory.display()))?
            .path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let glyph = read_json(&path)?;
        let name = glyph["name"].as_str().map_or_else(
            || file_name_to_string(&path.file_stem().unwrap_or_default().to_string_lossy()),
            str::to_owned,
        );
        glyphs.insert(name, (path, glyph));
    }
    Ok(glyphs)
}

/// Fontra's glyph file name: the name with reserved characters escaped, then `^` and a code
/// that records which letters are capitals, so names differing only in case differ on disk.
pub fn string_to_file_name(name: &str) -> String {
    const RESERVED: &str = " \"%*+/:<>?[\\]|^";
    const RESERVED_NAMES: [&str; 12] = [
        "con", "prn", "aux", "clock$", "nul", "com1", "lpt1", "lpt2", "lpt3", "com2", "com3",
        "com4",
    ];
    const BASE32: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";
    let chars: Vec<char> = name.chars().collect();
    let mut digits: Vec<usize> = chars
        .chunks(5)
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .filter(|(_, c)| c.is_uppercase())
                .map(|(bit, _)| 1 << bit)
                .sum()
        })
        .collect();
    while digits.last() == Some(&0) {
        digits.pop();
    }
    let mut file: String = chars
        .iter()
        .map(|&c| {
            if RESERVED.contains(c) || u32::from(c) < 32 || u32::from(c) == 0x7F {
                format!("%{:02X}", u32::from(c))
            } else {
                c.to_string()
            }
        })
        .collect();
    if let Some(rest) = file.strip_prefix('.') {
        file = format!("%2E{rest}");
    } else if let Some((base, rest)) = file.split_once('.')
        && RESERVED_NAMES.contains(&base.to_lowercase().as_str())
    {
        file = format!("{base}%2E{rest}");
    }
    if digits.is_empty() && RESERVED_NAMES.contains(&file.to_lowercase().as_str()) {
        digits.push(0);
    }
    if !digits.is_empty() {
        file.push('^');
        file.extend(digits.iter().map(|&digit| char::from(BASE32[digit])));
    }
    file
}

fn file_name_to_string(file_name: &str) -> String {
    let escaped = file_name.split('^').next().unwrap_or_default();
    let mut out = String::new();
    let mut rest = escaped;
    while let Some(index) = rest.find('%') {
        out.push_str(&rest[..index]);
        let code = rest
            .get(index + 1..index + 3)
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match code {
            Some(byte) => {
                out.push(char::from(byte));
                rest = &rest[index + 3..];
            }
            None => {
                out.push('%');
                rest = &rest[index + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    On,
    Cubic,
    Quad,
}

/// A layer's contours as `(x, y, kind, smooth)` points, from either path encoding.
/// A point as `(x, y, kind, smooth)`.
type ContourPoint = (f64, f64, Kind, bool);

fn read_contours(path: &Value) -> Vec<(Vec<ContourPoint>, bool)> {
    if let Some(contours) = path["contours"].as_array() {
        return contours
            .iter()
            .map(|contour| {
                let points = contour["points"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|point| {
                        let kind = match point["type"].as_str() {
                            Some("cubic") => Kind::Cubic,
                            Some("quad") => Kind::Quad,
                            _ => Kind::On,
                        };
                        (
                            point["x"].as_f64().unwrap_or_default(),
                            point["y"].as_f64().unwrap_or_default(),
                            kind,
                            point["smooth"].as_bool().unwrap_or(false),
                        )
                    })
                    .collect();
                (points, contour["isClosed"].as_bool().unwrap_or(false))
            })
            .collect();
    }
    let coordinates: Vec<f64> = path["coordinates"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_f64)
        .collect();
    let types: Vec<i64> = path["pointTypes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .collect();
    let mut start = 0;
    let mut contours = Vec::new();
    for info in path["contourInfo"].as_array().into_iter().flatten() {
        let Some(end) = info["endPoint"]
            .as_u64()
            .and_then(|end| usize::try_from(end).ok())
        else {
            continue;
        };
        let points = (start..=end)
            .filter_map(|index| {
                let kind = match types.get(index)? & 0x07 {
                    1 => Kind::Quad,
                    2 => Kind::Cubic,
                    _ => Kind::On,
                };
                Some((
                    *coordinates.get(2 * index)?,
                    *coordinates.get(2 * index + 1)?,
                    kind,
                    types[index] & 0x08 != 0,
                ))
            })
            .collect();
        contours.push((points, info["isClosed"].as_bool().unwrap_or(false)));
        start = end + 1;
    }
    contours
}

/// Fontra's decomposed transform, composed as fontTools' `DecomposedTransform.toTransform`.
fn compose(transformation: &Value) -> Affine {
    let get = |key: &str, default: f64| transformation[key].as_f64().unwrap_or(default);
    let (cx, cy) = (get("tCenterX", 0.0), get("tCenterY", 0.0));
    Affine::translate((get("translateX", 0.0) + cx, get("translateY", 0.0) + cy))
        * Affine::rotate(get("rotation", 0.0).to_radians())
        * Affine::scale_non_uniform(get("scaleX", 1.0), get("scaleY", 1.0))
        * Affine::new([
            1.0,
            get("skewY", 0.0).to_radians().tan(),
            get("skewX", 0.0).to_radians().tan(),
            1.0,
            0.0,
            0.0,
        ])
        * Affine::translate((-cx, -cy))
}

/// The inverse of [`compose`], as fontTools' `DecomposedTransform.fromTransform`.
fn decompose(transform: norad::AffineTransform) -> Value {
    let (mut a, mut b, c, d) = (
        transform.x_scale,
        transform.xy_scale,
        transform.yx_scale,
        transform.y_scale,
    );
    let flip = if a < 0.0 { -1.0 } else { 1.0 };
    a *= flip;
    b *= flip;
    let delta = a * d - b * c;
    let (mut rotation, mut scale_x, mut scale_y, mut skew_x, mut skew_y) =
        (0.0, 0.0, 0.0, 0.0, 0.0);
    if a != 0.0 || b != 0.0 {
        let r = a.hypot(b);
        rotation = if b >= 0.0 {
            (a / r).acos()
        } else {
            -(a / r).acos()
        };
        scale_x = r;
        scale_y = delta / r;
        skew_x = ((a * c + b * d) / (r * r)).atan();
    } else if c != 0.0 || d != 0.0 {
        let s = c.hypot(d);
        rotation = std::f64::consts::FRAC_PI_2
            - if d >= 0.0 {
                (-c / s).acos()
            } else {
                -(c / s).acos()
            };
        scale_x = delta / s;
        scale_y = s;
        skew_y = ((a * c + b * d) / (s * s)).atan();
    }
    json!({
        "translateX": number(transform.x_offset),
        "translateY": number(transform.y_offset),
        "rotation": number(rotation.to_degrees()),
        "scaleX": number(scale_x * flip),
        "scaleY": number(scale_y),
        "skewX": number(skew_x.to_degrees() * flip),
        "skewY": number(skew_y.to_degrees()),
        "tCenterX": 0,
        "tCenterY": 0,
    })
}

/// A Fontra static glyph as a UFO glyph.
fn read_layer(
    name: &str,
    glyph: &Value,
    codepoints: &[char],
    warnings: &mut Vec<String>,
) -> Result<norad::Glyph, String> {
    let mut out = norad::Glyph::new(name);
    out.width = glyph["xAdvance"].as_f64().unwrap_or_default();
    out.codepoints = codepoints.iter().copied().collect();
    for (points, closed) in read_contours(&glyph["path"]) {
        let count = points.len();
        let contour: Vec<norad::ContourPoint> = points
            .iter()
            .enumerate()
            .map(|(index, &(x, y, kind, smooth))| {
                let typ = match kind {
                    Kind::Cubic | Kind::Quad => norad::PointType::OffCurve,
                    Kind::On if !closed && index == 0 => norad::PointType::Move,
                    Kind::On => match points[(index + count - 1) % count].2 {
                        Kind::Cubic => norad::PointType::Curve,
                        Kind::Quad => norad::PointType::QCurve,
                        Kind::On => norad::PointType::Line,
                    },
                };
                norad::ContourPoint::new(x, y, typ, smooth, None, None)
            })
            .collect();
        if !contour.is_empty() {
            out.contours.push(norad::Contour::new(contour, None));
        }
    }
    for component in glyph["components"].as_array().into_iter().flatten() {
        let base = component["name"].as_str().unwrap_or_default();
        if component["location"]
            .as_object()
            .is_some_and(|location| !location.is_empty())
        {
            warnings.push(format!(
                "{name}: the location of component {base} is not read"
            ));
        }
        let c = compose(&component["transformation"]).as_coeffs();
        out.components.push(norad::Component::new(
            norad::Name::new(base).map_err(|error| format!("{name}: component {base}: {error}"))?,
            norad::AffineTransform {
                x_scale: c[0],
                xy_scale: c[1],
                yx_scale: c[2],
                y_scale: c[3],
                x_offset: c[4],
                y_offset: c[5],
            },
            None,
        ));
    }
    for anchor in glyph["anchors"].as_array().into_iter().flatten() {
        let anchor_name = anchor["name"]
            .as_str()
            .map(norad::Name::new)
            .transpose()
            .map_err(|error| format!("{name}: anchor: {error}"))?;
        out.anchors.push(norad::Anchor::new(
            anchor["x"].as_f64().unwrap_or_default(),
            anchor["y"].as_f64().unwrap_or_default(),
            anchor_name,
            None,
            None,
        ));
    }
    Ok(out)
}

/// A UFO glyph's drawing as the keys of a Fontra static glyph.
fn write_layer(glyph: &norad::Glyph) -> Map<String, Value> {
    let contours: Vec<Value> = glyph
        .contours
        .iter()
        .map(|contour| {
            let points = &contour.points;
            let closed = points
                .first()
                .is_some_and(|point| point.typ != norad::PointType::Move);
            let next_on = |index: usize| {
                let count = points.len();
                (1..=count)
                    .map(|step| index + step)
                    .filter(|&at| closed || at < count)
                    .map(|at| &points[at % count])
                    .find(|point| point.typ != norad::PointType::OffCurve)
                    .map(|point| point.typ)
            };
            let points: Vec<Value> = points
                .iter()
                .enumerate()
                .map(|(index, point)| {
                    let mut out = Map::new();
                    out.insert("x".into(), number(point.x));
                    out.insert("y".into(), number(point.y));
                    if point.typ == norad::PointType::OffCurve {
                        let kind = match next_on(index) {
                            Some(norad::PointType::QCurve) | None => "quad",
                            _ => "cubic",
                        };
                        out.insert("type".into(), kind.into());
                    } else if point.smooth {
                        out.insert("smooth".into(), true.into());
                    }
                    Value::Object(out)
                })
                .collect();
            json!({"points": points, "isClosed": closed})
        })
        .collect();
    let components: Vec<Value> = glyph
        .components
        .iter()
        .map(|component| {
            json!({
                "name": component.base.as_str(),
                "transformation": decompose(component.transform),
            })
        })
        .collect();
    let anchors: Vec<Value> = glyph
        .anchors
        .iter()
        .map(|anchor| {
            let mut out = Map::new();
            if let Some(name) = &anchor.name {
                out.insert("name".into(), name.as_str().into());
            }
            out.insert("x".into(), number(anchor.x));
            out.insert("y".into(), number(anchor.y));
            Value::Object(out)
        })
        .collect();
    let mut out = Map::new();
    out.insert("path".into(), json!({ "contours": contours }));
    out.insert("components".into(), components.into());
    out.insert("xAdvance".into(), number(glyph.width));
    out.insert("anchors".into(), anchors.into());
    out
}

/// One kerning table: groups on each side, the source identifiers, and values by pair.
#[derive(Clone, Default, PartialEq)]
struct KernTable {
    groups1: BTreeMap<String, Vec<String>>,
    groups2: BTreeMap<String, Vec<String>>,
    sources: Vec<String>,
    values: BTreeMap<(String, String), Vec<Option<f64>>>,
}

/// `kerning.csv`: tables by type, in file order.
fn read_kerning(folder: &Path) -> Result<Vec<(String, KernTable)>, String> {
    let path = folder.join(KERNING);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .has_headers(false)
        .flexible(true)
        .from_path(&path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let rows: Vec<Vec<String>> = reader
        .records()
        .map(|record| {
            record
                .map(|record| record.iter().map(str::to_owned).collect())
                .map_err(|error| format!("{}: {error}", path.display()))
        })
        .collect::<Result<_, _>>()?;
    let blank = |row: &Vec<String>| row.iter().all(String::is_empty);
    let mut tables = Vec::new();
    let mut table: Option<(String, KernTable)> = None;
    let mut section = "";
    let mut index = 0;
    while index < rows.len() {
        let row = &rows[index];
        index += 1;
        if blank(row) {
            section = "";
            continue;
        }
        match row[0].as_str() {
            "TYPE" => {
                tables.extend(table.take());
                let kind = rows
                    .get(index)
                    .and_then(|row| row.first())
                    .cloned()
                    .unwrap_or_default();
                index += 1;
                table = Some((kind, KernTable::default()));
                section = "";
            }
            "GROUPS1" | "GROUPS" => section = "groups1",
            "GROUPS2" => section = "groups2",
            "VALUES" => {
                if let (Some((_, table)), Some(header)) = (table.as_mut(), rows.get(index)) {
                    table.sources = header.iter().skip(2).cloned().collect();
                }
                index += 1;
                section = "values";
            }
            _ => {
                let Some((_, table)) = table.as_mut() else {
                    return Err(format!("{}: kerning before a TYPE row", path.display()));
                };
                match section {
                    "groups1" => {
                        table.groups1.insert(row[0].clone(), row[1..].to_vec());
                    }
                    "groups2" => {
                        table.groups2.insert(row[0].clone(), row[1..].to_vec());
                    }
                    "values" if row.len() >= 2 => {
                        let values = row[2..]
                            .iter()
                            .map(|cell| cell.trim().parse::<f64>().ok())
                            .collect();
                        table
                            .values
                            .insert((row[0].clone(), row[1].clone()), values);
                    }
                    _ => {}
                }
            }
        }
    }
    tables.extend(table);
    Ok(tables)
}

/// Rows as Fontra's `csv.writer(delimiter=";")` writes them; an empty row is a blank line.
fn csv_text(rows: &[Vec<String>], crlf: bool) -> Result<String, String> {
    let terminator = if crlf { "\r\n" } else { "\n" };
    let mut text = String::new();
    for row in rows {
        if row.is_empty() {
            text.push_str(terminator);
            continue;
        }
        let mut writer = csv::WriterBuilder::new()
            .delimiter(b';')
            .flexible(true)
            .terminator(if crlf {
                csv::Terminator::CRLF
            } else {
                csv::Terminator::Any(b'\n')
            })
            .from_writer(Vec::new());
        writer
            .write_record(row)
            .map_err(|error| error.to_string())?;
        let bytes = writer.into_inner().map_err(|error| error.to_string())?;
        text.push_str(&String::from_utf8(bytes).map_err(|error| error.to_string())?);
    }
    Ok(text)
}

/// Whether an existing file ends its lines with CRLF, as Python's csv module does; a new
/// file does too.
fn uses_crlf(path: &Path) -> bool {
    std::fs::read(path).map_or(true, |bytes| bytes.windows(2).any(|pair| pair == b"\r\n"))
}

fn write_kerning(tables: &[(String, KernTable)], crlf: bool) -> Result<String, String> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (kind, table) in tables {
        if table.values.is_empty() && table.groups1.is_empty() && table.groups2.is_empty() {
            continue;
        }
        if !rows.is_empty() {
            rows.push(vec![]);
        }
        rows.push(vec!["TYPE".into()]);
        rows.push(vec![kind.clone()]);
        rows.push(vec![]);
        for (label, groups) in [("GROUPS1", &table.groups1), ("GROUPS2", &table.groups2)] {
            rows.push(vec![label.into()]);
            for (name, members) in groups {
                rows.push(
                    std::iter::once(name.clone())
                        .chain(members.iter().cloned())
                        .collect(),
                );
            }
            rows.push(vec![]);
        }
        rows.push(vec!["VALUES".into()]);
        rows.push(
            ["side1".to_owned(), "side2".to_owned()]
                .into_iter()
                .chain(table.sources.iter().cloned())
                .collect(),
        );
        for ((left, right), values) in &table.values {
            let cells = values.iter().map(|value| {
                value.map_or_else(String::new, |value| match number(value) {
                    Value::Number(number) => number.to_string(),
                    other => other.to_string(),
                })
            });
            rows.push(
                [left.clone(), right.clone()]
                    .into_iter()
                    .chain(cells)
                    .collect(),
            );
        }
    }
    csv_text(&rows, crlf)
}

/// A kerning side in UFO terms: `@name` is a group on that side.
fn ufo_side(side: &str, first: bool) -> String {
    match side.strip_prefix('@') {
        Some(group) if first => format!("public.kern1.{group}"),
        Some(group) => format!("public.kern2.{group}"),
        None => side.to_owned(),
    }
}

fn fontra_side(side: &str) -> String {
    side.strip_prefix("public.kern1.")
        .or_else(|| side.strip_prefix("public.kern2."))
        .map_or_else(|| side.to_owned(), |group| format!("@{group}"))
}

fn compact(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect()
}

/// Convert a `.fontra` folder to UFO sources and, when it has axes or several sources, a
/// designspace. Unread data is listed in the result's warnings.
pub fn to_ufo_files(folder: &Path) -> Result<ConversionResult, String> {
    let font = FontData::read(folder)?;
    let mut warnings = font.warnings.clone();
    let info = GlyphInfo::read(folder)?;
    let glyph_files = read_glyph_files(folder)?;
    let kerning = read_kerning(folder)?;
    let features = std::fs::read_to_string(folder.join(FEATURES)).unwrap_or_default();
    let font_info = &font.data["fontInfo"];
    let family = font_info["familyName"]
        .as_str()
        .filter(|name| !name.trim().is_empty())
        .map_or_else(
            || {
                folder
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            },
            str::to_owned,
        );
    let family_compact = compact(&family);
    let codepoints: HashMap<&str, &[char]> = info
        .rows
        .iter()
        .map(|(name, codepoints, _)| (name.as_str(), codepoints.as_slice()))
        .collect();
    let mut order: Vec<&str> = info.rows.iter().map(|(name, ..)| name.as_str()).collect();
    for name in glyph_files.keys() {
        if !codepoints.contains_key(name.as_str()) {
            order.push(name);
        }
    }
    let kern = kerning
        .iter()
        .find(|(kind, _)| kind == KERN)
        .map(|(_, table)| table);
    for (kind, _) in kerning.iter().filter(|(kind, _)| kind != KERN) {
        warnings.push(format!("the {kind} kerning table is not read"));
    }

    let scratch = scratch_directory()?;
    let result = (|| {
        let mut document = norad::designspace::DesignSpaceDocument {
            format: 5.0,
            ..Default::default()
        };
        for axis in &font.axes {
            document.axes.push(norad::designspace::Axis {
                name: axis.name.clone(),
                tag: axis.tag.clone(),
                minimum: Some(narrow(axis.minimum)),
                default: narrow(axis.default),
                maximum: Some(narrow(axis.maximum)),
                hidden: axis.hidden,
                map: (!axis.mapping.is_empty()).then(|| {
                    axis.mapping
                        .iter()
                        .map(|&(input, output)| norad::designspace::AxisMapping {
                            input: narrow(input),
                            output: narrow(output),
                        })
                        .collect()
                }),
                ..Default::default()
            });
        }
        let mut used = std::collections::HashSet::new();
        let mut matched_layers = std::collections::HashSet::new();
        for source in &font.sources {
            let mut filename = format!("{family_compact}-{}.ufo", compact(&source.name));
            let mut suffix = 1;
            while !used.insert(filename.clone()) {
                suffix += 1;
                filename = format!("{family_compact}-{}-{suffix}.ufo", compact(&source.name));
            }
            let mut ufo = norad::Font::new();
            fill_font_info(&mut ufo.font_info, &font, source, &family)?;
            ufo.features.clone_from(&features);
            let layer = ufo.default_layer_mut();
            for name in &order {
                let Some((_, glyph)) = glyph_files.get(*name) else {
                    continue;
                };
                let Some(layer_name) = font.layer_for(glyph, source) else {
                    continue;
                };
                matched_layers.insert((name.to_string(), layer_name.to_owned()));
                let static_glyph = &glyph["layers"][layer_name]["glyph"];
                let codepoints = codepoints.get(name).copied().unwrap_or_default();
                layer.insert_glyph(read_layer(name, static_glyph, codepoints, &mut warnings)?);
            }
            ufo.lib.insert(
                "public.glyphOrder".into(),
                plist::Value::Array(order.iter().map(|&name| name.into()).collect()),
            );
            if let Some(kern) = kern {
                for (group, members) in kern
                    .groups1
                    .iter()
                    .map(|(group, members)| (format!("public.kern1.{group}"), members))
                    .chain(
                        kern.groups2
                            .iter()
                            .map(|(group, members)| (format!("public.kern2.{group}"), members)),
                    )
                {
                    let members = members
                        .iter()
                        .filter(|member| !member.is_empty())
                        .map(|member| norad::Name::new(member))
                        .collect::<Result<_, _>>()
                        .map_err(|error| format!("kerning group {group}: {error}"))?;
                    ufo.groups.insert(
                        norad::Name::new(&group).map_err(|error| error.to_string())?,
                        members,
                    );
                }
                if let Some(column) = kern.sources.iter().position(|id| *id == source.id) {
                    for ((left, right), values) in &kern.values {
                        let Some(Some(value)) = values.get(column) else {
                            continue;
                        };
                        let (left, right) = (ufo_side(left, true), ufo_side(right, false));
                        ufo.kerning
                            .entry(norad::Name::new(&left).map_err(|error| error.to_string())?)
                            .or_default()
                            .insert(
                                norad::Name::new(&right).map_err(|error| error.to_string())?,
                                *value,
                            );
                    }
                }
            }
            ufo.save(scratch.join(&filename))
                .map_err(|error| format!("{filename}: {error}"))?;
            document.sources.push(norad::designspace::Source {
                familyname: Some(family.clone()),
                stylename: Some(source.name.clone()),
                name: Some(source.id.clone()),
                filename,
                location: font
                    .axes
                    .iter()
                    .map(|axis| norad::designspace::Dimension {
                        name: axis.name.clone(),
                        xvalue: Some(narrow(source.location[&axis.name])),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            });
        }
        let skipped = glyph_files
            .iter()
            .flat_map(|(name, (_, glyph))| {
                glyph["layers"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(move |(layer, _)| (name.clone(), layer.clone()))
            })
            .filter(|key| !matched_layers.contains(key))
            .count();
        if skipped > 0 {
            warnings.push(format!(
                "{skipped} glyph layers between sources or in the background are not read"
            ));
        }
        if !font.axes.is_empty() || document.sources.len() > 1 {
            document
                .save(scratch.join(format!("{family_compact}.designspace")))
                .map_err(|error| error.to_string())?;
        }
        Ok::<_, String>(())
    })();
    let files = result.and_then(|()| collect_text_files(&scratch));
    let _ = std::fs::remove_dir_all(&scratch);
    Ok(ConversionResult {
        family_name: family,
        files: files?,
        warnings,
    })
}

fn fill_font_info(
    info: &mut norad::FontInfo,
    font: &FontData,
    source: &FontSource,
    family: &str,
) -> Result<(), String> {
    let text = |key: &str| font.data["fontInfo"][key].as_str().map(str::to_owned);
    info.family_name = Some(family.to_owned());
    info.style_name = Some(source.name.clone());
    info.units_per_em = norad::fontinfo::NonNegativeIntegerOrFloat::new(font.units_per_em);
    info.copyright = text("copyright");
    info.trademark = text("trademark");
    info.open_type_name_designer = text("designer");
    info.open_type_name_designer_url = text("designerURL");
    info.open_type_name_manufacturer = text("manufacturer");
    info.open_type_name_manufacturer_url = text("manufacturerURL");
    info.open_type_name_license = text("licenseDescription");
    info.open_type_name_license_url = text("licenseInfoURL");
    info.open_type_name_description = text("description");
    info.open_type_os2_vendor_id = text("vendorID");
    info.version_major = font.data["fontInfo"]["versionMajor"]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok());
    info.version_minor = font.data["fontInfo"]["versionMinor"]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok());
    let source_data = &font.data["sources"][&source.id];
    let metric = |key: &str| source_data["lineMetricsHorizontalLayout"][key]["value"].as_f64();
    info.ascender = metric("ascender");
    info.descender = metric("descender");
    info.x_height = metric("xHeight");
    info.cap_height = metric("capHeight");
    info.italic_angle = source_data["italicAngle"]
        .as_f64()
        .filter(|angle| *angle != 0.0);
    Ok(())
}

fn scratch_directory() -> Result<std::path::PathBuf, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "runebender-fontra-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(path)
}

fn collect_text_files(root: &Path) -> Result<Vec<ConvertedFile>, String> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?
        {
            let path = entry
                .map_err(|error| format!("{}: {error}", directory.display()))?
                .path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            files.push(ConvertedFile {
                path: relative,
                text,
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn copy_folder(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|error| format!("{}: {error}", to.display()))?;
    for entry in std::fs::read_dir(from).map_err(|error| format!("{}: {error}", from.display()))? {
        let path = entry
            .map_err(|error| format!("{}: {error}", from.display()))?
            .path();
        let target = to.join(path.file_name().unwrap_or_default());
        if path.is_dir() {
            copy_folder(&path, &target)?;
        } else {
            std::fs::copy(&path, &target)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
    }
    Ok(())
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    std::fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))
}

/// Write `original` with the working copy's edits merged in, as a new folder at `stage`.
pub fn write_back(original: &Path, sources: &[WorkingSource], stage: &Path) -> Result<(), String> {
    copy_folder(original, stage)?;
    let mut font = FontData::read(stage)?;
    let font_sources: Vec<(&FontSource, &WorkingSource)> = sources
        .iter()
        .map(|working| {
            let found = match &working.name {
                Some(name) => font.sources.iter().find(|source| source.id == *name),
                None if font.sources.len() == 1 => font.sources.first(),
                None => None,
            };
            found.map(|source| (source, working)).ok_or_else(|| {
                format!(
                    "{} has no source in the Fontra folder; new sources save only as UFO",
                    working.filename
                )
            })
        })
        .collect::<Result<_, _>>()?;

    // Line metrics and italic angle, per source.
    let mut data = font.data.clone();
    for (source, working) in &font_sources {
        if source.id.is_empty() {
            continue;
        }
        let entry = &mut data["sources"][&source.id];
        let info = &working.font.font_info;
        for (key, value) in [
            ("ascender", info.ascender),
            ("descender", info.descender),
            ("xHeight", info.x_height),
            ("capHeight", info.cap_height),
        ] {
            if let Some(value) = value {
                let metric = &mut entry["lineMetricsHorizontalLayout"][key];
                if metric["value"].as_f64() != Some(value) {
                    metric["value"] = number(value);
                }
            }
        }
        let angle = info.italic_angle.unwrap_or_default();
        if entry["italicAngle"].as_f64().unwrap_or_default() != angle {
            entry["italicAngle"] = number(angle);
        }
    }
    if data != font.data {
        write_text(&stage.join(FONT_DATA), &fontra_json(&data))?;
        font.data = data;
    }

    // The glyph set, in the order of the first source, and Unicode values.
    let mut order: Vec<&str> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut codepoints: HashMap<&str, Vec<char>> = HashMap::new();
    for (_, working) in &font_sources {
        let listed: Vec<&str> = working
            .font
            .lib
            .get("public.glyphOrder")
            .and_then(plist::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(plist::Value::as_string)
            .collect();
        let layer = working.font.default_layer();
        for name in listed
            .into_iter()
            .chain(layer.iter().map(|glyph| glyph.name().as_str()))
        {
            if let Some(glyph) = layer.get_glyph(name)
                && seen.insert(glyph.name().as_str())
            {
                order.push(glyph.name().as_str());
                codepoints.insert(glyph.name().as_str(), glyph.codepoints.iter().collect());
            }
        }
    }
    let info = GlyphInfo::read(stage)?;
    let extra: HashMap<&str, &Vec<String>> = info
        .rows
        .iter()
        .map(|(name, _, rest)| (name.as_str(), rest))
        .collect();
    let mut names: Vec<&str> = order.clone();
    names.sort_unstable();
    let mut rows = vec![info.header.clone()];
    for name in names {
        let mut row = vec![
            name.to_owned(),
            codepoints[name]
                .iter()
                .map(|c| format!("U+{:04X}", u32::from(*c)))
                .collect::<Vec<_>>()
                .join(","),
        ];
        row.extend(
            extra
                .get(name)
                .into_iter()
                .flat_map(|rest| rest.iter().cloned()),
        );
        while row.len() > 2 && row.last().is_some_and(String::is_empty) {
            row.pop();
        }
        rows.push(row);
    }
    let glyph_info = csv_text(&rows, uses_crlf(&stage.join(GLYPH_INFO)))?;
    let glyph_info_path = stage.join(GLYPH_INFO);
    if std::fs::read_to_string(&glyph_info_path).ok().as_deref() != Some(glyph_info.as_str()) {
        write_text(&glyph_info_path, &glyph_info)?;
    }

    // Glyph files: replace changed layers, add new glyphs and sources, drop removed glyphs.
    let mut files = read_glyph_files(stage)?;
    let mut ignored = Vec::new();
    for name in &order {
        let (path, mut glyph) = files.remove(*name).unwrap_or_else(|| {
            (
                stage
                    .join(GLYPHS)
                    .join(format!("{}.json", string_to_file_name(name))),
                json!({"name": name, "sources": [], "layers": {}}),
            )
        });
        let before = glyph.clone();
        for (source, working) in &font_sources {
            let Some(edited) = working.font.default_layer().get_glyph(name) else {
                continue;
            };
            let layer_name = match font.layer_for(&glyph, source) {
                Some(layer) => layer.to_owned(),
                None => {
                    let mut layer = if source.id.is_empty() {
                        source.name.clone()
                    } else {
                        source.id.clone()
                    };
                    while glyph["layers"].get(&layer).is_some() {
                        layer.push('*');
                    }
                    let mut entry = json!({"name": source.name, "layerName": layer});
                    if source.id.is_empty() {
                        entry["location"] = json!({});
                    } else {
                        entry["location"] = json!({});
                        entry["locationBase"] = source.id.clone().into();
                    }
                    if !glyph["sources"].is_array() {
                        glyph["sources"] = json!([]);
                    }
                    glyph["sources"]
                        .as_array_mut()
                        .expect("sources is an array")
                        .push(entry);
                    layer
                }
            };
            let current = &glyph["layers"][&layer_name]["glyph"];
            let unchanged = !current.is_null()
                && read_layer(name, current, &[], &mut ignored)
                    .is_ok_and(|current| same_drawing(&current, edited));
            if unchanged {
                continue;
            }
            if !glyph["layers"].is_object() {
                glyph["layers"] = json!({});
            }
            let layer = glyph["layers"]
                .as_object_mut()
                .expect("layers is an object")
                .entry(layer_name)
                .or_insert_with(|| json!({"glyph": {}}));
            if !layer["glyph"].is_object() {
                layer["glyph"] = json!({});
            }
            let target = layer["glyph"].as_object_mut().expect("glyph is an object");
            for (key, value) in write_layer(edited) {
                target.insert(key, value);
            }
        }
        if glyph != before {
            write_text(&path, &fontra_json(&glyph))?;
        }
    }
    for (path, _) in files.into_values() {
        std::fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    }

    // Kerning: the "kern" table takes the UFO groups, and the UFO values in the columns of the
    // sources Runebender read; columns of other sources stay as they were.
    let mut tables = read_kerning(stage)?;
    let existing = tables.iter().position(|(kind, _)| kind == KERN);
    let original_kern = existing.map(|index| tables[index].1.clone());
    let mut kern = original_kern.clone().unwrap_or_default();
    for (source, _) in &font_sources {
        if !kern.sources.contains(&source.id) {
            kern.sources.push(source.id.clone());
        }
    }
    let width = kern.sources.len();
    for values in kern.values.values_mut() {
        values.resize(width, None);
    }
    kern.groups1.clear();
    kern.groups2.clear();
    if let Some((_, first)) = font_sources.first() {
        for (group, members) in &first.font.groups {
            let members: Vec<String> = members.iter().map(|m| m.as_str().to_owned()).collect();
            if let Some(name) = group.as_str().strip_prefix("public.kern1.") {
                kern.groups1.insert(name.to_owned(), members);
            } else if let Some(name) = group.as_str().strip_prefix("public.kern2.") {
                kern.groups2.insert(name.to_owned(), members);
            }
        }
    }
    for (source, working) in &font_sources {
        let column = kern
            .sources
            .iter()
            .position(|id| *id == source.id)
            .expect("every read source has a column");
        for values in kern.values.values_mut() {
            values[column] = None;
        }
        for (left, pairs) in &working.font.kerning {
            for (right, value) in pairs {
                kern.values
                    .entry((fontra_side(left.as_str()), fontra_side(right.as_str())))
                    .or_insert_with(|| vec![None; width])[column] = Some(*value);
            }
        }
    }
    kern.values
        .retain(|_, values| values.iter().any(Option::is_some));
    let empty = kern.values.is_empty() && kern.groups1.is_empty() && kern.groups2.is_empty();
    let changed = match &original_kern {
        Some(original) => *original != kern,
        None => !empty,
    };
    if changed {
        match existing {
            Some(index) => tables[index].1 = kern,
            None => tables.insert(0, (KERN.into(), kern)),
        }
        let path = stage.join(KERNING);
        write_text(&path, &write_kerning(&tables, uses_crlf(&path))?)?;
    }

    // Feature text from the first source.
    if let Some((_, first)) = font_sources.first() {
        let path = stage.join(FEATURES);
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current != first.font.features {
            if first.font.features.is_empty() {
                let _ = std::fs::remove_file(&path);
            } else {
                write_text(&path, &first.font.features)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("rb-fontra-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let folder = root.join("Test.fontra");
        write_text(
            &folder.join(FONT_DATA),
            r#"{
"unitsPerEm": 1000,
"fontInfo": {"familyName": "Test Sans"},
"axes": {"axes": [{"name": "Weight", "label": "Weight", "tag": "wght",
"minValue": 100, "defaultValue": 400, "maxValue": 900,
"mapping": [[100, 0], [400, 50], [900, 100]]}]},
"sources": {
"light": {"name": "Light", "location": {"Weight": 0},
"lineMetricsHorizontalLayout": {"ascender": {"value": 750}}},
"regular": {"name": "Regular", "location": {"Weight": 50}},
"bold": {"name": "Bold", "location": {"Weight": 100}}
}
}
"#,
        )
        .unwrap();
        write_text(
            &folder.join(GLYPH_INFO),
            "glyph name;code points\r\nA;U+0041\r\nAacute;U+00C1\r\n",
        )
        .unwrap();
        let layer = |width: i64| {
            json!({"glyph": {
                "path": {"contours": [{"points": [
                    {"x": 0, "y": 0}, {"x": 100, "y": 700},
                    {"x": 150, "y": 720, "type": "cubic"}, {"x": 250, "y": 720, "type": "cubic"},
                    {"x": 300, "y": 700, "smooth": true}
                ], "isClosed": true}]},
                "xAdvance": width,
                "anchors": [{"name": "top", "x": 200, "y": 720}]
            }})
        };
        let mut glyph = json!({"name": "A", "sources": [], "layers": {}});
        for (id, width) in [("light", 500), ("regular", 550), ("bold", 600)] {
            glyph["sources"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name": id, "layerName": id, "location": {}, "locationBase": id}));
            glyph["layers"][id] = layer(width);
        }
        write_text(&folder.join("glyphs/A^1.json"), &fontra_json(&glyph)).unwrap();
        let accented = json!({"name": "Aacute",
        "sources": [{"name": "Regular", "layerName": "default", "location": {"Weight": 50}}],
        "layers": {"default": {"glyph": {"xAdvance": 550, "components": [
            {"name": "A", "transformation": {"translateX": 10, "scaleX": -1}}
        ]}}}});
        write_text(
            &folder.join("glyphs/Aacute^1.json"),
            &fontra_json(&accented),
        )
        .unwrap();
        write_text(
            &folder.join(KERNING),
            "TYPE\r\nkern\r\n\r\nGROUPS1\r\nA;A;Aacute\r\n\r\nGROUPS2\r\n\r\nVALUES\r\nside1;side2;light;regular;bold\r\n@A;A;-10;;-30\r\n",
        )
        .unwrap();
        folder
    }

    fn load(files: &ConversionResult, root: &Path) -> Vec<WorkingSource> {
        for file in &files.files {
            write_text(&root.join(&file.path), &file.text).unwrap();
        }
        super::super::foreign::read_working_copy(&root.join("TestSans.designspace")).unwrap()
    }

    #[test]
    fn file_names_match_fontra() {
        assert_eq!(string_to_file_name("A"), "A^1");
        assert_eq!(string_to_file_name("a"), "a");
        assert_eq!(string_to_file_name("Aacute"), "Aacute^1");
        assert_eq!(string_to_file_name("aaaaaB"), "aaaaaB^01");
        assert_eq!(string_to_file_name(".notdef"), "%2Enotdef");
        assert_eq!(string_to_file_name("con"), "con^0");
        assert_eq!(file_name_to_string("%2Enotdef"), ".notdef");
    }

    #[test]
    fn a_folder_opens_as_ufo_sources_on_a_designspace() {
        let folder = fixture("read");
        let result = to_ufo_files(&folder).unwrap();
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let sources = load(&result, &folder.with_file_name("out"));
        assert_eq!(sources.len(), 3);
        let light = &sources[0].font;
        assert_eq!(light.font_info.ascender, Some(750.0));
        let a = light.get_glyph("A").unwrap();
        assert_eq!(a.width, 500.0);
        let types: Vec<_> = a.contours[0].points.iter().map(|p| p.typ).collect();
        assert_eq!(
            types,
            [
                norad::PointType::Line,
                norad::PointType::Line,
                norad::PointType::OffCurve,
                norad::PointType::OffCurve,
                norad::PointType::Curve
            ]
        );
        assert!(a.contours[0].points[4].smooth);
        assert_eq!(a.codepoints.iter().collect::<Vec<_>>(), ['A']);
        // The component glyph has only a Regular source, found by its location.
        assert!(light.get_glyph("Aacute").is_none());
        let regular = &sources[1].font;
        let component = &regular.get_glyph("Aacute").unwrap().components[0];
        assert_eq!(component.transform.x_scale, -1.0);
        assert_eq!(component.transform.x_offset, 10.0);
        assert_eq!(light.kerning["public.kern1.A"]["A"], -10.0);
        assert!(!regular.kerning.contains_key("public.kern1.A"));
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }

    #[test]
    fn write_back_changes_only_edited_glyphs() {
        let folder = fixture("write");
        let result = to_ufo_files(&folder).unwrap();
        let mut sources = load(&result, &folder.with_file_name("out"));
        let untouched = std::fs::read_to_string(folder.join("glyphs/Aacute^1.json")).unwrap();
        // Widen the bold A, add a glyph, and change a kerning value.
        let bold = &mut sources[2].font;
        bold.default_layer_mut().get_glyph_mut("A").unwrap().width = 640.0;
        let mut b = norad::Glyph::new("B");
        b.width = 300.0;
        b.codepoints = ['B'].into_iter().collect();
        bold.default_layer_mut().insert_glyph(b);
        bold.kerning
            .get_mut("public.kern1.A")
            .unwrap()
            .insert(norad::Name::new("A").unwrap(), -40.0);

        let stage = folder.with_file_name("Saved.fontra");
        write_back(&folder, &sources, &stage).unwrap();

        assert_eq!(
            std::fs::read_to_string(stage.join("glyphs/Aacute^1.json")).unwrap(),
            untouched
        );
        let a = read_json(&stage.join("glyphs/A^1.json")).unwrap();
        assert_eq!(a["layers"]["bold"]["glyph"]["xAdvance"], 640);
        assert_eq!(a["layers"]["light"]["glyph"]["xAdvance"], 500);
        let b = read_json(&stage.join("glyphs/B^1.json")).unwrap();
        assert_eq!(b["sources"][0]["locationBase"], "bold");
        let info = std::fs::read_to_string(stage.join(GLYPH_INFO)).unwrap();
        assert!(info.contains("B;U+0042"), "{info}");
        let kerning = std::fs::read_to_string(stage.join(KERNING)).unwrap();
        assert!(kerning.contains("@A;A;-10;;-40"), "{kerning}");

        // The saved folder reads back with the edits.
        let reread = load(
            &to_ufo_files(&stage).unwrap(),
            &folder.with_file_name("again"),
        );
        assert_eq!(reread[2].font.get_glyph("A").unwrap().width, 640.0);
        assert!(same_drawing(
            reread[0].font.get_glyph("A").unwrap(),
            sources[0].font.get_glyph("A").unwrap()
        ));
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }
}
