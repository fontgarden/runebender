// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Guarded correspondence and document operations for scoped HOI authoring.
//!
//! Preview and export share exact cubic trajectories with local easing and connected handles.
//! Compilation samples these trajectories into disposable glyph-local sources.
//! Legacy glyph-level midpoint metadata keeps its separate existing behavior.

mod authoring;
mod evaluate;
pub use authoring::HoiInterval;
pub(super) use evaluate::curvature_bound;

use super::history::{HistoryDirection, HistoryReplayError, HistoryReplayOutcome};
use sha2::{Digest, Sha256};

use super::model::designspace::{AxisId, CanonicalAxis};
use super::model::hoi_trajectories::{HoiAxis, HoiEasing, HoiEndpoint, HoiTrajectory};
use super::project::Project;
use super::variable::LayerId;
use super::{LayerPointType, LayerView, PointId};

/// Session-local addresses recovered from a validated persistent trajectory record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundHoiTrajectory {
    /// Current canonical axis identity.
    pub axis: AxisId,
    /// Current lower endpoint layer.
    pub lower_layer: LayerId,
    /// Current upper endpoint layer.
    pub upper_layer: LayerId,
    /// Current lower endpoint point identity.
    pub lower_point: PointId,
    /// Current upper endpoint point identity.
    pub upper_point: PointId,
}

fn axis_guard(axis: &CanonicalAxis) -> HoiAxis {
    HoiAxis {
        tag: axis.coordinates.tag.clone(),
        name: axis.coordinates.name.clone(),
        minimum: axis.coordinates.min,
        default: axis.coordinates.default,
        maximum: axis.coordinates.max,
        mapping: axis.coordinates.map.clone(),
    }
}

fn role(point_type: LayerPointType) -> char {
    match point_type {
        LayerPointType::Move => 'M',
        LayerPointType::Line => 'L',
        LayerPointType::OffCurve => 'O',
        LayerPointType::Curve => 'C',
        LayerPointType::QCurve => 'Q',
    }
}

fn topology(layer: LayerView<'_>) -> Result<Vec<Vec<char>>, String> {
    if layer.hoi_intermediates().is_some_and(|old| !old.is_empty()) {
        return Err(
            "legacy HOI needs an explicit conversion before new trajectory authoring".into(),
        );
    }
    let mut contours = Vec::new();
    for contour in layer.contours() {
        if !contour.is_closed() || contour.is_hyper() {
            return Err("HOI v1 authoring supports closed cubic/line contours".into());
        }
        let points: Vec<_> = contour.points().collect();
        let roles: Vec<_> = points
            .iter()
            .map(|point| role(point.point_type()))
            .collect();
        if roles.is_empty()
            || roles.iter().all(|r| *r == 'O')
            || roles.iter().any(|r| matches!(r, 'Q' | 'M'))
            || points.iter().any(|p| !p.position().is_finite())
        {
            return Err(
                "HOI v1 needs finite explicit cubic/line nodes; quadratic controls are unsupported"
                    .into(),
            );
        }
        for (index, r) in roles.iter().enumerate().filter(|(_, r)| **r != 'O') {
            let controls = (1..roles.len())
                .take_while(|back| roles[(index + roles.len() - back) % roles.len()] == 'O')
                .count();
            if controls != if *r == 'C' { 2 } else { 0 } {
                return Err("HOI endpoint has invalid cubic control connectivity".into());
            }
        }
        contours.push(roles);
    }
    Ok(contours)
}

fn outline_fingerprint(layer: LayerView<'_>) -> Result<String, String> {
    let outline: Vec<_> = layer
        .contours()
        .map(|contour| {
            (
                contour.source_identifier(),
                contour.is_closed(),
                contour.is_hyper(),
                contour
                    .points()
                    .map(|point| {
                        (
                            point.source_identifier(),
                            role(point.point_type()),
                            point.is_smooth(),
                            point.position().x,
                            point.position().y,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let bytes = serde_json::to_vec(&outline).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

impl Project {
    fn hoi_axis(&self, id: AxisId) -> Result<&CanonicalAxis, String> {
        let ds = self
            .document_designspace()
            .ok_or("HOI needs a Designspace")?;
        if ds.axes().len() != 1 {
            return Err("HOI v1 authoring requires a single continuous axis".into());
        }
        ds.axes()
            .iter()
            .find(|axis| axis.id() == id)
            .ok_or_else(|| "HOI axis no longer exists".into())
    }

    fn hoi_sources(
        &self,
        glyph: &str,
        axis: &CanonicalAxis,
    ) -> Result<Vec<(LayerId, f64)>, String> {
        let ds = self
            .document_designspace()
            .ok_or("HOI needs a Designspace")?;
        let mut sources = Vec::new();
        for source in self.glyph_sources(glyph)? {
            let location = ds
                .sparse_sources()
                .iter()
                .find(|candidate| candidate.layer == source.layer)
                .map(|candidate| &candidate.location)
                .or_else(|| {
                    ds.sources()
                        .iter()
                        .find(|candidate| candidate.default_layer == source.layer)
                        .map(|candidate| &candidate.location)
                })
                .ok_or("HOI source descriptor is missing")?;
            let value = location.design(axis);
            if !value.is_finite() || value < axis.design_minimum() || value > axis.design_maximum()
            {
                return Err("HOI source lies outside the supported axis range".into());
            }
            sources.push((source.layer, value));
        }
        sources.sort_by(|a, b| a.1.total_cmp(&b.1));
        if sources.windows(2).any(|pair| pair[0].1 == pair[1].1) {
            return Err("HOI has ambiguous glyph sources at the same axis location".into());
        }
        Ok(sources)
    }

    fn hoi_interval(
        &self,
        glyph: &str,
        axis: &CanonicalAxis,
        lower: &LayerId,
        upper: &LayerId,
    ) -> Result<(f64, f64), String> {
        let sources = self.hoi_sources(glyph, axis)?;
        let pair = sources
            .windows(2)
            .find(|pair| &pair[0].0 == lower && &pair[1].0 == upper)
            .ok_or(
                "HOI endpoints must be neighboring contributing glyph sources in increasing order",
            )?;
        let a = self
            .document_layer(glyph, lower)
            .ok_or("missing lower HOI glyph")?;
        let b = self
            .document_layer(glyph, upper)
            .ok_or("missing upper HOI glyph")?;
        if topology(a)? != topology(b)? {
            return Err("HOI endpoint contours have incompatible point order or roles".into());
        }
        Ok((pair[0].1, pair[1].1))
    }

    fn capture_hoi_endpoint(
        &self,
        glyph: &str,
        layer_id: &LayerId,
        location: f64,
        contour_index: usize,
        point_index: usize,
    ) -> Result<HoiEndpoint, String> {
        let ds = self
            .document_designspace()
            .ok_or("HOI needs a Designspace")?;
        let source = ds
            .sources()
            .iter()
            .find(|source| source.id() == layer_id.source)
            .ok_or("HOI source is missing")?;
        let layer = self
            .document_layer(glyph, layer_id)
            .ok_or("HOI glyph layer is missing")?;
        let contour = layer
            .contours()
            .nth(contour_index)
            .ok_or("HOI contour is missing")?;
        let point = contour
            .points()
            .nth(point_index)
            .ok_or("HOI point is missing")?;
        if point.point_type() == LayerPointType::OffCurve {
            return Err(
                "HOI authoring selects on-curve nodes; their outline handles follow".into(),
            );
        }
        Ok(HoiEndpoint {
            source: source.filename.clone(),
            layer: layer_id.name.clone(),
            location,
            contour: contour_index,
            point: point_index,
            contour_identifier: contour.source_identifier().map(str::to_owned),
            point_identifier: point.source_identifier().map(str::to_owned),
            outline_sha256: outline_fingerprint(layer)?,
        })
    }

    /// Capture a straight cubic authoring record without changing either endpoint drawing.
    ///
    /// The selected point is in `lower`; the upper correspondence uses validated outline order.
    /// Controls and easing may be adjusted before installing the record with `set_hoi_trajectories`.
    pub fn prepare_hoi_trajectory(
        &self,
        glyph: &str,
        axis: AxisId,
        lower: &LayerId,
        upper: &LayerId,
        point: PointId,
    ) -> Result<HoiTrajectory, String> {
        let axis = self.hoi_axis(axis)?;
        let (lo, hi) = self.hoi_interval(glyph, axis, lower, upper)?;
        let layer = self
            .document_layer(glyph, lower)
            .ok_or("HOI lower glyph is missing")?;
        let (ci, pi, a) = layer
            .contours()
            .enumerate()
            .find_map(|(ci, contour)| {
                contour
                    .points()
                    .enumerate()
                    .find(|(_, candidate)| candidate.id() == point)
                    .map(|(pi, point)| (ci, pi, point.position()))
            })
            .ok_or("selected HOI point is not in the lower source")?;
        let b = self
            .document_layer(glyph, upper)
            .and_then(|layer| layer.contours().nth(ci))
            .and_then(|contour| contour.points().nth(pi))
            .ok_or("upper HOI point is missing")?
            .position();
        let c1 = a.lerp(b, 1.0 / 3.0);
        let c2 = a.lerp(b, 2.0 / 3.0);
        let record = HoiTrajectory {
            glyph: glyph.to_owned(),
            axis: axis_guard(axis),
            lower: self.capture_hoi_endpoint(glyph, lower, lo, ci, pi)?,
            upper: self.capture_hoi_endpoint(glyph, upper, hi, ci, pi)?,
            controls: [[c1.x, c1.y], [c2.x, c2.y]],
            easing: HoiEasing::Linear,
        };
        record.validate()?;
        Ok(record)
    }

    /// Resolve a record to this session's identities, refusing stale or ambiguous correspondence.
    ///
    /// Version 1 conservatively invalidates a record after any endpoint outline or axis change.
    /// Reordering never retargets a stored point index to a different node.
    pub fn bind_hoi_trajectory(
        &self,
        record: &HoiTrajectory,
    ) -> Result<BoundHoiTrajectory, String> {
        record.validate()?;
        let ds = self
            .document_designspace()
            .ok_or("HOI needs a Designspace")?;
        let axis = ds
            .axes()
            .iter()
            .find(|axis| axis.coordinates.tag == record.axis.tag)
            .ok_or("HOI axis was removed or renamed")?;
        self.hoi_axis(axis.id())?;
        if axis_guard(axis) != record.axis {
            return Err(
                "HOI axis bounds or mapping changed; reauthor the affected trajectory".into(),
            );
        }
        let layer_id = |endpoint: &HoiEndpoint| -> Result<LayerId, String> {
            let source = ds
                .sources()
                .iter()
                .find(|source| source.filename == endpoint.source)
                .ok_or("HOI source was removed or renamed")?;
            Ok(LayerId {
                source: source.id(),
                name: endpoint.layer.clone(),
            })
        };
        let lower = layer_id(&record.lower)?;
        let upper = layer_id(&record.upper)?;
        let (lo, hi) = self.hoi_interval(&record.glyph, axis, &lower, &upper)?;
        let resolve = |endpoint: &HoiEndpoint,
                       layer: &LayerId,
                       location|
         -> Result<PointId, String> {
            let current = self.capture_hoi_endpoint(
                &record.glyph,
                layer,
                location,
                endpoint.contour,
                endpoint.point,
            )?;
            if current != *endpoint {
                return Err("HOI endpoint drawing, topology, identity, or location changed; record suspended".into());
            }
            self.document_layer(&record.glyph, layer)
                .and_then(|layer| layer.contours().nth(endpoint.contour))
                .and_then(|contour| contour.points().nth(endpoint.point))
                .map(|point| point.id())
                .ok_or_else(|| "HOI point is missing".into())
        };
        Ok(BoundHoiTrajectory {
            axis: axis.id(),
            lower_point: resolve(&record.lower, &lower, lo)?,
            upper_point: resolve(&record.upper, &upper, hi)?,
            lower_layer: lower,
            upper_layer: upper,
        })
    }

    /// Read authoring records, preserving malformed or unknown source payloads in place.
    pub fn hoi_trajectories(&self) -> Result<Vec<HoiTrajectory>, String> {
        self.document_designspace()
            .map_or_else(|| Ok(Vec::new()), |ds| ds.hoi_trajectories())
    }

    /// Install validated records and dirty only Designspace metadata, returning whether it changed.
    ///
    /// Unchanged suspended records may be retained while another record is removed.
    /// Newly added or edited records must rebind successfully. Each change records one undo step.
    pub fn set_hoi_trajectories(&mut self, records: &[HoiTrajectory]) -> Result<bool, String> {
        let before = self
            .document_designspace()
            .ok_or("HOI needs a Designspace")?
            .clone();
        let previous = before.hoi_trajectories()?;
        for record in records {
            if !previous.contains(record) {
                self.bind_hoi_trajectory(record)?;
            }
        }
        let mut after = before.clone();
        if !after.set_hoi_trajectories(records)? {
            return Ok(false);
        }
        self.variable
            .replace_designspace_if_current(&before, after.clone())
            .map_err(|_| "Designspace changed during HOI authoring")?;
        self.variable.revision = self.variable.revision.wrapping_add(1);
        self.ds_dirty = true;
        self.hoi_history.record(before, after);
        Ok(true)
    }

    /// Replay one guarded authoring transaction without changing any source drawing.
    ///
    /// A Designspace edit makes an older step stale. Restored records must still bind to
    /// their endpoint drawings; a failed replay leaves both the document and history intact.
    pub fn replay_hoi_history(
        &mut self,
        direction: HistoryDirection,
    ) -> Result<HistoryReplayOutcome, HistoryReplayError<String>> {
        let current = self
            .document_designspace()
            .cloned()
            .ok_or_else(|| HistoryReplayError::Apply("HOI needs a Designspace".to_owned()))?;
        let mut history = std::mem::take(&mut self.hoi_history);
        let result = history.replay(&current, direction, |expected, replacement| {
            for record in replacement.hoi_trajectories()? {
                self.bind_hoi_trajectory(&record)?;
            }
            self.variable
                .replace_designspace_if_current(expected, replacement.clone())
                .map_err(|_| "Designspace changed during HOI history replay".to_owned())?;
            self.variable.revision = self.variable.revision.wrapping_add(1);
            self.ds_dirty = true;
            Ok(())
        });
        self.hoi_history = history;
        result
    }

    /// Validate authoring metadata before rendering or exporting, never silently linearizing it.
    pub(crate) fn require_evaluable_hoi(&self, glyph: Option<&str>) -> Result<(), String> {
        self.require_evaluable_hoi_records(glyph, &self.hoi_trajectories()?)
    }

    pub(crate) fn require_evaluable_hoi_records(
        &self,
        glyph: Option<&str>,
        records: &[HoiTrajectory],
    ) -> Result<(), String> {
        for record in records
            .iter()
            .filter(|record| glyph.is_none_or(|glyph| glyph == record.glyph))
        {
            self.bind_hoi_trajectory(record)?;
            for source in self.glyph_sources(&record.glyph)? {
                if self
                    .document_layer(&record.glyph, &source.layer)
                    .and_then(LayerView::hoi_intermediates)
                    .is_some_and(|old| !old.is_empty())
                {
                    return Err(
                        "legacy and scoped HOI cannot be evaluated in the same glyph".into(),
                    );
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "hoi_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "hoi/evaluation_tests.rs"]
mod evaluation_tests;
