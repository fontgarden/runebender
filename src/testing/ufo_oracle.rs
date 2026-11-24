// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Historical UFO outline conversions kept as oracles for canonical parity tests.
//!
//! The shipped editor renders and edits canonical layers.
//! These functions read `norad` glyphs the way the earlier editor did, so tests can check that
//! the canonical code still draws the same outline.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use kurbo::{Affine, BezPath, Point};
use norad::{Contour, ContourPoint, Font, Glyph, PointType};

use crate::font::LayerPointType;
use crate::font::model::smart_components::{
    SMART_COMPONENT_AXES_KEY, SMART_COMPONENT_POLE_KEY, SMART_COMPONENT_VALUES_KEY,
    SmartComponentAxes, SmartComponentPole, SmartComponentValues,
};
use crate::formats::metadata::metaballs::read_metaballs;
use crate::outline::glyph_paths::{self, point_key};
use crate::outline::metaballs;
use crate::outline::path::hyper_model;

/// A legacy point address: (contour index, point index).
pub(crate) type PointId = (usize, usize);

/// Reverse selected contours through the historical integer-rounded UFO conversion.
///
/// An empty selection reverses every contour.
pub(crate) fn reverse_contours(glyph: &mut Glyph, selected: &HashSet<PointId>) -> bool {
    let mut changed = false;
    for c in 0..glyph.contours.len() {
        let targeted = selected.is_empty()
            || (0..glyph.contours[c].points.len()).any(|p| selected.contains(&(c, p)));
        if !targeted || glyph.contours[c].points.is_empty() {
            continue;
        }
        let smooth_at: HashMap<(i64, i64), bool> = glyph.contours[c]
            .points
            .iter()
            .filter(|p| p.typ != PointType::OffCurve)
            .map(|p| (point_key(p.x, p.y), p.smooth))
            .collect();
        let path = contour_to_bezpath(&glyph.contours[c]).reverse_subpaths();
        if let Some(reversed) = bezpath_to_contour(&path, &smooth_at) {
            glyph.contours[c] = reversed;
            changed = true;
        }
    }
    changed
}

fn off_point((x, y): (f64, f64)) -> ContourPoint {
    ContourPoint::new(x, y, PointType::OffCurve, false, None, None)
}

/// Convert one closed historical path to a UFO contour, restoring surviving smooth flags.
fn bezpath_to_contour(path: &BezPath, smooth_at: &HashMap<(i64, i64), bool>) -> Option<Contour> {
    use kurbo::PathEl;
    let mut points: Vec<ContourPoint> = Vec::new();
    let mut start: Option<Point> = None;
    let smooth = |x: f64, y: f64| smooth_at.get(&point_key(x, y)).copied().unwrap_or(false);
    let on = |x: f64, y: f64, curve: bool, smooth: bool| {
        ContourPoint::new(
            x.round(),
            y.round(),
            if curve {
                PointType::Curve
            } else {
                PointType::Line
            },
            smooth,
            None,
            None,
        )
    };
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) => start = Some(*p),
            PathEl::LineTo(p) => points.push(on(p.x, p.y, false, smooth(p.x, p.y))),
            PathEl::CurveTo(c1, c2, p) => {
                points.push(off_point((c1.x.round(), c1.y.round())));
                points.push(off_point((c2.x.round(), c2.y.round())));
                points.push(on(p.x, p.y, true, smooth(p.x, p.y)));
            }
            PathEl::QuadTo(c, p) => {
                let s = points
                    .iter()
                    .rev()
                    .find(|q| q.typ != PointType::OffCurve)
                    .map(|q| Point::new(q.x, q.y))
                    .or(start)?;
                let c1 = s + (c.to_vec2() - s.to_vec2()) * (2.0 / 3.0);
                let c2 = *p + (c.to_vec2() - p.to_vec2()) * (2.0 / 3.0);
                points.push(off_point((c1.x.round(), c1.y.round())));
                points.push(off_point((c2.x.round(), c2.y.round())));
                points.push(on(p.x, p.y, true, smooth(p.x, p.y)));
            }
            PathEl::ClosePath => {}
        }
    }
    let start = start?;
    // A repeated start is the closing segment's endpoint in UFO storage order.
    let last_on = points.iter().rposition(|p| p.typ != PointType::OffCurve)?;
    let lp = &points[last_on];
    if (lp.x - start.x.round()).abs() < 0.51 && (lp.y - start.y.round()).abs() < 0.51 {
        let tail: Vec<ContourPoint> = points.drain(last_on..).collect();
        let (controls, on_pt) = tail.split_at(tail.len() - 1);
        let mut rotated = vec![on_pt[0].clone()];
        rotated.extend(points);
        rotated.extend(controls.iter().cloned());
        points = rotated;
    } else {
        let first = on(start.x, start.y, false, smooth(start.x, start.y));
        points.insert(0, first);
    }
    if points
        .iter()
        .filter(|p| p.typ != PointType::OffCurve)
        .count()
        < 2
    {
        return None;
    }
    Some(Contour::new(points, None))
}

/// Build a `BezPath` for a glyph, with components resolved recursively through `font`.
///
/// Component transforms apply in full.
/// A missing base glyph contributes nothing.
pub(crate) fn glyph_to_bezpath(glyph: &Glyph, font: &Font) -> BezPath {
    let mut path = BezPath::new();
    for contour in &glyph.contours {
        append_contour(&mut path, contour);
    }
    if let Ok(preview) = metaball_preview(glyph) {
        path.extend(preview);
    }
    append_components(&mut path, glyph, font, Affine::IDENTITY, 0);
    path
}

/// Only the glyph's own contours, without components.
pub(crate) fn contours_to_bezpath(glyph: &Glyph) -> BezPath {
    let mut path = BezPath::new();
    for contour in &glyph.contours {
        append_contour(&mut path, contour);
    }
    path
}

/// One contour as a `BezPath`.
pub(crate) fn contour_to_bezpath(contour: &Contour) -> BezPath {
    let mut path = BezPath::new();
    append_contour(&mut path, contour);
    path
}

/// The live metaball preview of a UFO glyph, without changing it.
fn metaball_preview(glyph: &Glyph) -> Result<BezPath, String> {
    let source = read_metaballs(glyph)?;
    let mut path = BezPath::new();
    for group in &source.groups {
        for contour in metaballs::preview(group, metaballs::OutlineOptions::default())? {
            path.extend(contour);
        }
    }
    Ok(path)
}

/// A sparse-layer pole: the axis tags it sits at the top of, and its point coordinates.
type Pole = (BTreeSet<String>, Vec<(f64, f64)>);

/// Interpolated contours for a smart part at the given axis values.
///
/// Only point-compatible pole layers take part.
/// Anything else falls back to the default outline.
fn smart_contours(
    base: &Glyph,
    font: &Font,
    axes: &SmartComponentAxes,
    values: &BTreeMap<String, f64>,
) -> Option<Vec<Contour>> {
    let flat = |glyph: &Glyph| -> Option<Vec<(f64, f64)>> {
        if glyph.contours.len() != base.contours.len() {
            return None;
        }
        let mut coords = Vec::new();
        for (a, b) in base.contours.iter().zip(glyph.contours.iter()) {
            if a.points.len() != b.points.len() {
                return None;
            }
            for p in &b.points {
                coords.push((p.x, p.y));
            }
        }
        Some(coords)
    };
    let default_coords = flat(base)?;
    let mut poles: Vec<Pole> = Vec::new();
    for layer in font.layers.iter() {
        let Some(candidate) = layer.get_glyph(base.name()) else {
            continue;
        };
        let Some(pole) = candidate
            .lib
            .get(SMART_COMPONENT_POLE_KEY)
            .and_then(|value| SmartComponentPole::from_plist(value).ok())
        else {
            continue;
        };
        let tops: BTreeSet<String> = axes
            .axes()
            .iter()
            .filter(|axis| {
                (axis.top_value() - axis.bottom_value()).abs() >= 1e-9 && pole.is_top(axis.name())
            })
            .map(|axis| axis.name().to_owned())
            .collect();
        if tops.is_empty() {
            continue;
        }
        let coords = flat(candidate)?;
        poles.push((tops, coords));
    }
    let coords = glyph_paths::interpolate_smart_coordinates(axes, values, default_coords, poles)?;
    let mut out = Vec::with_capacity(base.contours.len());
    let mut cursor = 0_usize;
    for contour in &base.contours {
        let points = contour
            .points
            .iter()
            .map(|p| {
                let (x, y) = coords[cursor];
                cursor += 1;
                ContourPoint::new(x, y, p.typ, p.smooth, None, None)
            })
            .collect();
        out.push(Contour::new(points, None));
    }
    Some(out)
}

fn append_components(
    path: &mut BezPath,
    glyph: &Glyph,
    font: &Font,
    parent_transform: Affine,
    depth: u8,
) {
    // Guard against reference cycles in malformed UFOs.
    if depth > 8 {
        return;
    }
    let component_order: Vec<_> = (0..glyph.components.len()).collect();
    let smart_values = glyph
        .lib
        .get(SMART_COMPONENT_VALUES_KEY)
        .and_then(|value| SmartComponentValues::from_plist(value, &component_order).ok());
    for (index, component) in glyph.components.iter().enumerate() {
        let Some(base) = font.get_glyph(&component.base) else {
            continue;
        };
        let t = component.transform;
        let combined = parent_transform
            * Affine::new([
                t.x_scale, t.xy_scale, t.yx_scale, t.y_scale, t.x_offset, t.y_offset,
            ]);
        let axes = base
            .lib
            .get(SMART_COMPONENT_AXES_KEY)
            .and_then(|value| SmartComponentAxes::from_plist(value).ok());
        let values = axes.as_ref().map(|axes| {
            axes.axes()
                .iter()
                .filter_map(|axis| {
                    smart_values
                        .as_ref()?
                        .value(index, axis.name())
                        .map(|value| (axis.name().to_owned(), value))
                })
                .collect::<BTreeMap<_, _>>()
        });
        let smart = axes
            .as_ref()
            .zip(values)
            .filter(|(_, values)| !values.is_empty())
            .and_then(|(axes, values)| smart_contours(base, font, axes, &values));
        let contours = smart.unwrap_or_else(|| base.contours.clone());
        for contour in &contours {
            let mut contour_path = BezPath::new();
            append_contour(&mut contour_path, contour);
            path.extend((combined * &contour_path).elements().iter().cloned());
        }
        if let Ok(preview) = metaball_preview(base) {
            path.extend(combined * preview);
        }
        append_components(path, base, font, combined, depth + 1);
    }
}

fn append_contour(path: &mut BezPath, contour: &Contour) {
    let points = &contour.points;
    if points.is_empty() {
        return;
    }
    // Hyperbezier contours carry only on-curve points; their curves come from the spline solver.
    if hyper_model::norad_contour_is_hyper(contour) {
        let model = hyper_model::Contour::from_norad(contour);
        crate::outline::path::Path::from_contour(&model).append_to_bezpath(path);
        return;
    }
    let closed = points[0].typ != PointType::Move;
    glyph_paths::append_canonical_points(
        path,
        points.iter().map(|point| {
            (
                Point::new(point.x, point.y),
                match point.typ {
                    PointType::Move => LayerPointType::Move,
                    PointType::Line => LayerPointType::Line,
                    PointType::OffCurve => LayerPointType::OffCurve,
                    PointType::Curve => LayerPointType::Curve,
                    PointType::QCurve => LayerPointType::QCurve,
                },
            )
        }),
        closed,
    );
}
