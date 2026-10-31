// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Exact scoped trajectories and the curvature bound used by the export sampler.

use kurbo::Point;

use super::{HoiEasing, HoiTrajectory, Project};
use crate::font::LayerPointType;
use crate::font::interpolation::InterpolatedLayer;
use crate::font::var_model::{Location, denormalize_value};

pub(super) fn progress(easing: HoiEasing, t: f64) -> f64 {
    match easing {
        HoiEasing::Linear => t,
        HoiEasing::EaseIn => t * t,
        HoiEasing::EaseOut => t * (2.0 - t),
        HoiEasing::EaseInOut => t * t * (3.0 - 2.0 * t),
    }
}

pub(super) fn evaluate(points: [Point; 4], easing: HoiEasing, t: f64) -> Point {
    let s = progress(easing, t);
    let a = points[0].lerp(points[1], s);
    let b = points[1].lerp(points[2], s);
    let c = points[2].lerp(points[3], s);
    a.lerp(b, s).lerp(b.lerp(c, s), s)
}

/// Bound the norm of the second derivative with respect to interval progress.
pub(in crate::font) fn curvature_bound(points: [Point; 4], easing: HoiEasing) -> f64 {
    let edges = [
        points[1] - points[0],
        points[2] - points[1],
        points[3] - points[2],
    ];
    let first = 3.0 * edges.iter().map(|v| v.hypot()).fold(0.0, f64::max);
    let second = 6.0
        * (edges[1] - edges[0])
            .hypot()
            .max((edges[2] - edges[1]).hypot());
    let (speed, acceleration) = match easing {
        HoiEasing::Linear => (1.0, 0.0),
        HoiEasing::EaseIn | HoiEasing::EaseOut => (2.0, 2.0),
        HoiEasing::EaseInOut => (1.5, 6.0),
    };
    second * speed * speed + first * acceleration
}

impl Project {
    pub(in crate::font) fn hoi_curve_points(
        &self,
        record: &HoiTrajectory,
    ) -> Result<[Point; 4], String> {
        let bound = self.bind_hoi_trajectory(record)?;
        let point = |layer| {
            self.document_layer(&record.glyph, layer)
                .and_then(|layer| layer.contours().nth(record.lower.contour))
                .and_then(|contour| contour.points().nth(record.lower.point))
                .map(|point| point.position())
                .ok_or_else(|| "HOI endpoint point is missing".to_owned())
        };
        Ok([
            point(&bound.lower_layer)?,
            (record.controls[0][0], record.controls[0][1]).into(),
            (record.controls[1][0], record.controls[1][1]).into(),
            point(&bound.upper_layer)?,
        ])
    }

    pub(crate) fn apply_hoi_trajectories(
        &self,
        glyph: &str,
        location: &Location,
        output: &mut InterpolatedLayer,
        records: &[HoiTrajectory],
    ) -> Result<(), String> {
        for record in records.iter().filter(|record| record.glyph == glyph) {
            let points = self.hoi_curve_points(record)?;
            let axis = self
                .document_designspace()
                .expect("bound HOI has a Designspace")
                .axes()
                .iter()
                .find(|axis| axis.coordinates.tag == record.axis.tag)
                .expect("bound HOI has its axis");
            let normalized = location.get(&axis.coordinates.name).copied().unwrap_or(0.0);
            if !normalized.is_finite() {
                return Err("HOI location must be finite".into());
            }
            let design = denormalize_value(
                normalized,
                axis.design_minimum(),
                axis.design_default(),
                axis.design_maximum(),
            );
            let t =
                (design - record.lower.location) / (record.upper.location - record.lower.location);
            // Keep both source drawings exact, including shared interval boundaries.
            if t <= 0.0 || t >= 1.0 {
                continue;
            }
            let position = evaluate(points, record.easing, t);
            let ci = record.lower.contour;
            let pi = record.lower.point;
            let contour = output
                .contours()
                .nth(ci)
                .ok_or("HOI output contour is missing")?;
            let point = contour
                .points
                .get(pi)
                .ok_or("HOI output point is missing")?;
            let displacement = position - point.position;
            let count = contour.points.len();
            let attached: Vec<_> = [(pi + count - 1) % count, (pi + 1) % count]
                .into_iter()
                .filter(|&index| contour.points[index].point_type == LayerPointType::OffCurve)
                .collect();
            if !position.is_finite() || !displacement.is_finite() {
                return Err("HOI trajectory exceeded finite coordinates".into());
            }
            output.point_at_mut(ci, pi).expect("checked point").position = position;
            // In an explicit cubic, each control belongs to exactly one endpoint.
            for index in attached {
                let handle = output.point_at_mut(ci, index).expect("checked handle");
                handle.position += displacement;
                if !handle.position.is_finite() {
                    return Err("HOI handle exceeded finite coordinates".into());
                }
            }
        }
        Ok(())
    }
}
