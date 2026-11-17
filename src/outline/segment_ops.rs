// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Ported from runebender-web/core/src/editor.rs segment operations (Apache-2.0).

//! Segment hit testing on canonical layers: enumerate a layer's segments with their stable point
//! identities, and find the segment nearest to a point.
//! The select and pen tools use it; the edits themselves are on `LayerEditDraft`.

use kurbo::{CubicBez, Line, ParamCurveNearest, PathSeg, Point, QuadBez};

pub use crate::font::DocumentSegmentEndpoint;
use crate::font::{LayerPointType, LayerView, PointId as DocumentPointId};

/// One ordinary canonical segment addressed by stable source identities.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentSegmentHit {
    /// Stored or implied identity of the segment start.
    pub start: DocumentSegmentEndpoint,
    /// Stored or implied identity of the segment end.
    pub end: DocumentSegmentEndpoint,
    /// Stable identities of the off-curve controls in contour order.
    pub controls: Vec<DocumentPointId>,
    /// Segment geometry used for hit testing and subdivision.
    pub seg: PathSeg,
}

impl DocumentSegmentHit {
    /// Every stable point identity belonging to this segment.
    pub fn point_ids(&self) -> Vec<DocumentPointId> {
        let mut ids = Vec::new();
        self.start.append_source_ids(&mut ids);
        for id in &self.controls {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        self.end.append_source_ids(&mut ids);
        ids
    }
}

#[derive(Clone, Copy)]
struct DocumentSegmentPoint {
    id: DocumentPointId,
    position: Point,
    kind: LayerPointType,
}

fn explicit_endpoint(point: DocumentSegmentPoint) -> DocumentSegmentEndpoint {
    DocumentSegmentEndpoint::Point(point.id)
}

fn implied_endpoint(
    first: DocumentSegmentPoint,
    second: DocumentSegmentPoint,
) -> DocumentSegmentEndpoint {
    DocumentSegmentEndpoint::Implied {
        first_control: first.id,
        second_control: second.id,
    }
}

fn push_document_segment(
    output: &mut Vec<DocumentSegmentHit>,
    start: DocumentSegmentEndpoint,
    end: DocumentSegmentEndpoint,
    controls: &[DocumentSegmentPoint],
    seg: PathSeg,
) {
    output.push(DocumentSegmentHit {
        start,
        end,
        controls: controls.iter().map(|point| point.id).collect(),
        seg,
    });
}

/// Enumerate ordinary segments directly from a canonical document layer.
pub fn ordinary_layer_segments(layer: LayerView<'_>) -> Vec<DocumentSegmentHit> {
    let mut output = Vec::new();
    for contour in layer.contours() {
        if contour.is_hyper() {
            continue;
        }
        let points: Vec<_> = contour
            .points()
            .map(|point| DocumentSegmentPoint {
                id: point.id(),
                position: point.position(),
                kind: point.point_type(),
            })
            .collect();
        if points.is_empty() {
            continue;
        }
        let Some(start_index) = points
            .iter()
            .position(|point| point.kind != LayerPointType::OffCurve)
        else {
            if contour.is_closed() {
                let mut start_point = points[points.len() - 1]
                    .position
                    .midpoint(points[0].position);
                let mut start = implied_endpoint(points[points.len() - 1], points[0]);
                for (index, control) in points.iter().copied().enumerate() {
                    let next = points[(index + 1) % points.len()];
                    let end_point = control.position.midpoint(next.position);
                    let end = implied_endpoint(control, next);
                    push_document_segment(
                        &mut output,
                        start,
                        end,
                        &[control],
                        PathSeg::Quad(QuadBez::new(start_point, control.position, end_point)),
                    );
                    start = end;
                    start_point = end_point;
                }
            }
            continue;
        };
        let rotated: Vec<_> = points[start_index..]
            .iter()
            .chain(points[..start_index].iter())
            .copied()
            .collect();
        let mut start = explicit_endpoint(rotated[0]);
        let mut start_point = rotated[0].position;
        let mut controls = Vec::new();
        let length = rotated.len();
        let indices: Vec<_> = if contour.is_closed() {
            (1..=length).map(|index| index % length).collect()
        } else {
            (1..length).collect()
        };
        for index in indices {
            let point = rotated[index];
            if point.kind == LayerPointType::OffCurve {
                controls.push(point);
                continue;
            }
            let end = explicit_endpoint(point);
            match point.kind {
                LayerPointType::Line | LayerPointType::Move => push_document_segment(
                    &mut output,
                    start,
                    end,
                    &[],
                    PathSeg::Line(Line::new(start_point, point.position)),
                ),
                LayerPointType::Curve => {
                    let seg = match controls.as_slice() {
                        [first, second] => PathSeg::Cubic(CubicBez::new(
                            start_point,
                            first.position,
                            second.position,
                            point.position,
                        )),
                        [control] => PathSeg::Quad(QuadBez::new(
                            start_point,
                            control.position,
                            point.position,
                        )),
                        _ => PathSeg::Line(Line::new(start_point, point.position)),
                    };
                    let used_controls = if controls.len() <= 2 {
                        controls.as_slice()
                    } else {
                        &[]
                    };
                    push_document_segment(&mut output, start, end, used_controls, seg);
                }
                LayerPointType::QCurve if controls.len() > 1 => {
                    for pair in controls.windows(2) {
                        let implied_point = pair[0].position.midpoint(pair[1].position);
                        let implied = implied_endpoint(pair[0], pair[1]);
                        push_document_segment(
                            &mut output,
                            start,
                            implied,
                            &pair[..1],
                            PathSeg::Quad(QuadBez::new(
                                start_point,
                                pair[0].position,
                                implied_point,
                            )),
                        );
                        start = implied;
                        start_point = implied_point;
                    }
                    let control = *controls.last().expect("quadratic controls are nonempty");
                    push_document_segment(
                        &mut output,
                        start,
                        end,
                        &[control],
                        PathSeg::Quad(QuadBez::new(start_point, control.position, point.position)),
                    );
                }
                LayerPointType::QCurve => {
                    let seg = controls.first().map_or_else(
                        || PathSeg::Line(Line::new(start_point, point.position)),
                        |control| {
                            PathSeg::Quad(QuadBez::new(
                                start_point,
                                control.position,
                                point.position,
                            ))
                        },
                    );
                    push_document_segment(&mut output, start, end, &controls, seg);
                }
                LayerPointType::OffCurve => unreachable!("controls were handled above"),
            }
            controls.clear();
            start = end;
            start_point = point.position;
        }
    }
    output
}

/// Find the nearest ordinary canonical segment within `radius`.
pub fn nearest_ordinary_layer_segment_with_t(
    layer: LayerView<'_>,
    design_point: Point,
    radius: f64,
) -> Option<(DocumentSegmentHit, f64)> {
    let max_distance_squared = radius * radius;
    let mut best: Option<(DocumentSegmentHit, f64, f64)> = None;
    for hit in ordinary_layer_segments(layer) {
        let nearest = hit.seg.nearest(design_point, 1e-6);
        if nearest.distance_sq > max_distance_squared {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(_, _, distance)| nearest.distance_sq < *distance)
        {
            best = Some((hit, nearest.t, nearest.distance_sq));
        }
    }
    best.map(|(hit, parameter, _)| (hit, parameter))
}
