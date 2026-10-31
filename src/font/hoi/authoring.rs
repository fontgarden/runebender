// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Read-only authoring geometry and transient previews for interactive HOI tools.

use std::collections::HashSet;

use kurbo::{BezPath, Point};

use super::{AxisId, HoiTrajectory, LayerId, LayerPointType, Project};
use crate::font::var_model::{Location, normalize_value};

/// One supported neighboring interval for a glyph on its single continuous axis.
#[derive(Clone, Debug, PartialEq)]
pub struct HoiInterval {
    /// Stable axis identity.
    pub axis: AxisId,
    /// Axis name used by normalized locations.
    pub axis_name: String,
    /// Lower contributing glyph layer, including authored sparse layers.
    pub lower: LayerId,
    /// Upper contributing glyph layer.
    pub upper: LayerId,
    /// Lower mapped design coordinate.
    pub lower_location: f64,
    /// Upper mapped design coordinate.
    pub upper_location: f64,
}

impl Project {
    /// List neighboring intervals without modifying the source drawings or authoring data.
    pub fn hoi_intervals(&self, glyph: &str) -> Result<Vec<HoiInterval>, String> {
        let ds = self
            .document_designspace()
            .ok_or("Interpolation editing needs a Designspace")?;
        let axis = ds
            .axes()
            .first()
            .ok_or("Interpolation editing needs a continuous axis")?;
        self.hoi_axis(axis.id())?;
        let sources = self.hoi_sources(glyph, axis)?;
        Ok(sources
            .windows(2)
            .map(|pair| HoiInterval {
                axis: axis.id(),
                axis_name: axis.coordinates.name.clone(),
                lower: pair[0].0.clone(),
                upper: pair[1].0.clone(),
                lower_location: pair[0].1,
                upper_location: pair[1].1,
            })
            .collect())
    }

    /// All on-curve connections in an interval, using stored curves where they still bind.
    /// Straight connections returned here are not added to the document until edited.
    pub fn hoi_connections(
        &self,
        glyph: &str,
        interval: &HoiInterval,
    ) -> Result<Vec<HoiTrajectory>, String> {
        let axis = self.hoi_axis(interval.axis)?;
        self.hoi_interval(glyph, axis, &interval.lower, &interval.upper)?;
        let records = self.hoi_trajectories()?;
        self.require_evaluable_hoi(Some(glyph))?;
        let layer = self
            .document_layer(glyph, &interval.lower)
            .ok_or("Missing lower glyph")?;
        let mut result = Vec::new();
        for contour in layer.contours() {
            for point in contour
                .points()
                .filter(|point| point.point_type() != LayerPointType::OffCurve)
            {
                let straight = self.prepare_hoi_trajectory(
                    glyph,
                    interval.axis,
                    &interval.lower,
                    &interval.upper,
                    point.id(),
                )?;
                result.push(
                    records
                        .iter()
                        .find(|record| record.same_scope(&straight))
                        .cloned()
                        .unwrap_or(straight),
                );
            }
        }
        Ok(result)
    }

    /// Resolve the four geometric cubic points of a validated trajectory.
    pub fn hoi_trajectory_geometry(&self, record: &HoiTrajectory) -> Result<[Point; 4], String> {
        self.hoi_curve_points(record)
    }

    /// Convert interval progress to a normalized design location, without applying a user map twice.
    pub fn hoi_location(&self, interval: &HoiInterval, progress: f64) -> Result<Location, String> {
        if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
            return Err("Interpolation progress must be between zero and one".into());
        }
        let axis = self.hoi_axis(interval.axis)?;
        let design = interval.lower_location
            + progress * (interval.upper_location - interval.lower_location);
        Ok(Location::from([(
            axis.coordinates.name.clone(),
            normalize_value(
                design,
                axis.design_minimum(),
                axis.design_default(),
                axis.design_maximum(),
            ),
        )]))
    }

    /// Evaluate a transient authoring draft, including components, without history, revision, or writes.
    pub fn preview_hoi_trajectories(
        &self,
        glyph: &str,
        location: &Location,
        records: &[HoiTrajectory],
    ) -> Result<BezPath, String> {
        for (index, record) in records.iter().enumerate() {
            if records[..index]
                .iter()
                .any(|other| record.same_scope(other))
            {
                return Err("Duplicate HOI trajectory in preview".into());
            }
        }
        fn resolve(
            project: &Project,
            glyph: &str,
            location: &Location,
            records: &[HoiTrajectory],
            seen: &mut HashSet<String>,
        ) -> Result<BezPath, String> {
            if seen.len() >= 64 || !seen.insert(glyph.to_owned()) {
                return Err("Cyclic or excessively deep component graph".into());
            }
            let layer = project.try_interpolated_layer_with_hoi(glyph, location, records)?;
            let mut path = layer.contours_to_bezpath();
            for component in layer.components() {
                let child = resolve(project, &component.reference, location, records, seen)?;
                path.extend((component.transform * child).elements().iter().copied());
            }
            seen.remove(glyph);
            Ok(path)
        }
        resolve(self, glyph, location, records, &mut HashSet::new())
    }
}

#[cfg(test)]
mod tests {
    use crate::font::hoi::tests::{files, fixture, root, saved_files};

    #[test]
    fn authoring_preview_has_no_revision_history_or_source_side_effects() {
        let mut memory = fixture();
        let project = &mut memory.project;
        let interval = &project.hoi_intervals("dot").unwrap()[0];
        let connections = project.hoi_connections("dot", interval).unwrap();
        assert_eq!(connections.len(), 4, "only on-curve points are connections");
        let mut record = connections[0].clone();
        record.controls[0][1] += 80.0;
        record.controls[1][1] += 120.0;
        let location = project.hoi_location(interval, 0.37).unwrap();
        let revision = project.document_revision();
        let preview = project
            .preview_hoi_trajectories("dot", &location, &[record.clone()])
            .unwrap();
        assert_ne!(
            preview,
            project
                .preview_hoi_trajectories("dot", &location, &[])
                .unwrap()
        );
        assert_eq!(project.document_revision(), revision);
        assert!(!project.is_modified());
        assert!(project.hoi_trajectories().unwrap().is_empty());
        assert!(
            project
                .preview_hoi_trajectories("dot", &location, &[record.clone(), record.clone()])
                .is_err()
        );
        project.set_hoi_trajectories(&[record.clone()]).unwrap();
        assert_eq!(
            project
                .preview_hoi_trajectories("dot", &location, &[record])
                .unwrap(),
            preview
        );
        let original = files(&root());
        let saved = saved_files(&memory, original.clone());
        for (name, bytes) in original.iter().filter(|(name, _)| name.ends_with(".glif")) {
            assert_eq!(&saved[name], bytes, "{name}");
        }
    }
}
