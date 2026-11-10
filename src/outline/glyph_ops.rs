// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Historical UFO reversal oracle for canonical geometry parity tests.
//!
//! Standalone editing regressions exercise Project transactions in `glyph_ops_tests.rs`.
//! Only the reversal adapter and its conversion helper remain for parity comparisons.

use std::collections::{HashMap, HashSet};

use kurbo::BezPath;
use norad::{Contour, ContourPoint, Glyph, PointType};

use crate::outline::glyph_paths;
use crate::outline::glyph_paths::point_key;

/// A legacy point address: (contour index, point index).
pub type PointId = (usize, usize);

/// Reverse selected contours through the historical integer-rounded UFO conversion.
///
/// An empty selection reverses every contour.
pub fn reverse_contours(glyph: &mut Glyph, selected: &HashSet<PointId>) -> bool {
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
        let path = glyph_paths::contour_to_bezpath(&glyph.contours[c]).reverse_subpaths();
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
    let mut start: Option<kurbo::Point> = None;
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
                    .map(|q| kurbo::Point::new(q.x, q.y))
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

#[path = "glyph_ops_tests.rs"]
mod tests;
