// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Path representation & geometry. Ported from
// runebender-xilem/src/path/.

//! Path abstraction for glyph outlines: the editable representation.
//!
//! The `Path` enum wraps two curve types: `Bezier` (UFO contours whose
//! segments are lines, quadratic curves and cubic curves, in any mix)
//! and `Hyper` (hyperbezier splines with only on-curve points). Both
//! convert to `kurbo::BezPath` for rendering. Paths are created from
//! `workspace::Contour` data when a glyph is opened for editing, and
//! converted back when the session is saved.

pub mod bezier;
pub mod hyper;
pub mod hyper_model;
pub mod point;
pub mod point_list;
pub mod quadrant;
pub mod segment;

pub use bezier::BezierPath;
pub use hyper::HyperPath;
pub use point::{PathPoint, PointType};
pub use point_list::PathPoints;
pub use quadrant::Quadrant;
pub use segment::{Segment, SegmentInfo};

use self::hyper_model as workspace;
use crate::font::model::entity_id::EntityId;
use crate::font::{ContourView, LayerPointType};
use kurbo::BezPath;

#[derive(Clone, Copy)]
struct SourcePoint {
    position: kurbo::Point,
    kind: LayerPointType,
    smooth: bool,
}

fn path_point(source: SourcePoint) -> PathPoint {
    PathPoint {
        id: EntityId::next(),
        point: source.position,
        typ: if source.kind == LayerPointType::OffCurve {
            PointType::OffCurve { auto: false }
        } else {
            PointType::OnCurve {
                smooth: source.smooth,
            }
        },
    }
}

fn append_normalized_segment(
    output: &mut Vec<PathPoint>,
    controls: &[SourcePoint],
    endpoint: SourcePoint,
    append_endpoint: bool,
) {
    match endpoint.kind {
        LayerPointType::Line | LayerPointType::Move => {}
        LayerPointType::Curve if controls.len() <= 2 => {
            output.extend(controls.iter().copied().map(path_point));
        }
        LayerPointType::QCurve => {
            for pair in controls.windows(2) {
                output.push(path_point(pair[0]));
                output.push(PathPoint {
                    id: EntityId::next(),
                    point: pair[0].position.midpoint(pair[1].position),
                    typ: PointType::OnCurve { smooth: false },
                });
            }
            if let Some(last) = controls.last().copied() {
                output.push(path_point(last));
            }
        }
        LayerPointType::Curve | LayerPointType::OffCurve => {}
    }
    if append_endpoint {
        output.push(path_point(endpoint));
    }
}

fn normalized_path_points(source: &[SourcePoint], closed: bool) -> PathPoints {
    let Some(start_index) = source
        .iter()
        .position(|point| point.kind != LayerPointType::OffCurve)
    else {
        if !closed || source.is_empty() {
            return PathPoints::new();
        }
        let mut output = Vec::with_capacity(source.len() * 2);
        for (index, control) in source.iter().copied().enumerate() {
            let previous = source[(index + source.len() - 1) % source.len()];
            output.push(PathPoint {
                id: EntityId::next(),
                point: previous.position.midpoint(control.position),
                typ: PointType::OnCurve { smooth: false },
            });
            output.push(path_point(control));
        }
        return PathPoints::from_vec(output);
    };
    let rotated: Vec<_> = source[start_index..]
        .iter()
        .chain(&source[..start_index])
        .copied()
        .collect();
    let mut output = vec![path_point(rotated[0])];
    let mut controls = Vec::new();
    for point in &rotated[1..] {
        if point.kind == LayerPointType::OffCurve {
            controls.push(*point);
        } else {
            append_normalized_segment(&mut output, &controls, *point, true);
            controls.clear();
        }
    }
    if closed {
        append_normalized_segment(&mut output, &controls, rotated[0], false);
    }
    PathPoints::from_vec(output)
}

/// A path in a glyph outline: a bezier contour or a hyperbezier contour.
#[derive(Debug, Clone)]
pub enum Path {
    /// A contour of line, quadratic and cubic segments in any mix.
    Bezier(BezierPath),
    /// A hyperbezier contour; only on-curve points are stored and handles are solved.
    Hyper(HyperPath),
}

impl Path {
    /// Build an editable outline path directly from a canonical document contour.
    pub fn from_document_contour(contour: ContourView<'_>) -> Self {
        let closed = contour.is_closed();
        let hyper = contour.is_hyper();
        let source: Vec<_> = contour
            .points()
            .map(|point| SourcePoint {
                position: point.position(),
                kind: point.point_type(),
                smooth: point.is_smooth(),
            })
            .collect();
        if hyper {
            let mut points: Vec<_> = source
                .iter()
                .filter(|point| point.kind != LayerPointType::OffCurve)
                .map(|point| PathPoint {
                    id: EntityId::next(),
                    point: point.position,
                    typ: PointType::OnCurve {
                        smooth: point.kind != LayerPointType::Line,
                    },
                })
                .collect();
            if closed && !points.is_empty() {
                points.rotate_left(1);
            }
            let points = PathPoints::from_vec(points);
            Self::Hyper(HyperPath::from_points(points, closed))
        } else {
            Self::Bezier(BezierPath::new(
                normalized_path_points(&source, closed),
                closed,
            ))
        }
    }

    pub(crate) fn entity_id(&self) -> EntityId {
        match self {
            Self::Bezier(path) => path.id,
            Self::Hyper(path) => path.id,
        }
    }

    /// Renders this path as a new `kurbo::BezPath`.
    pub fn to_bezpath(&self) -> BezPath {
        match self {
            Self::Bezier(bezier) => bezier.to_bezpath(),
            Self::Hyper(hyper) => hyper.to_bezpath(),
        }
    }

    /// Appends this path's elements to an existing `BezPath` without clearing it.
    pub fn append_to_bezpath(&self, path: &mut BezPath) {
        match self {
            Self::Bezier(bezier) => bezier.append_to_bezpath(path),
            Self::Hyper(hyper) => hyper.append_to_bezpath(path),
        }
    }

    /// Detect the curve type from the contour and dispatch.
    pub fn from_contour(contour: &workspace::Contour) -> Self {
        let has_hyper = contour.points.iter().any(|pt| {
            matches!(
                pt.point_type,
                workspace::PointType::Hyper | workspace::PointType::HyperCorner
            )
        });

        if has_hyper {
            return Self::Hyper(HyperPath::from_contour(contour));
        }

        let source: Vec<_> = contour
            .points
            .iter()
            .map(|point| SourcePoint {
                position: kurbo::Point::new(point.x, point.y),
                kind: match point.point_type {
                    workspace::PointType::Move => LayerPointType::Move,
                    workspace::PointType::Line => LayerPointType::Line,
                    workspace::PointType::OffCurve => LayerPointType::OffCurve,
                    workspace::PointType::Curve => LayerPointType::Curve,
                    workspace::PointType::QCurve => LayerPointType::QCurve,
                    workspace::PointType::Hyper | workspace::PointType::HyperCorner => {
                        unreachable!("hyper contours returned above")
                    }
                },
                smooth: point.smooth,
            })
            .collect();
        let closed = contour
            .points
            .first()
            .is_none_or(|point| !matches!(point.point_type, workspace::PointType::Move));
        let mut points = normalized_path_points(&source, closed).to_vec();
        if closed && !points.is_empty() {
            points.rotate_left(1);
        }
        Self::Bezier(BezierPath::new(PathPoints::from_vec(points), closed))
    }

    /// Returns the path's editable points, whatever the curve type.
    pub fn points(&self) -> &PathPoints {
        match self {
            Self::Bezier(bezier) => bezier.points(),
            Self::Hyper(hyper) => hyper.points(),
        }
    }

    /// Whether the last segment connects back to the first point.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Bezier(bezier) => bezier.is_closed(),
            Self::Hyper(hyper) => hyper.is_closed(),
        }
    }

    /// Converts back to the `workspace::Contour` form used for saving and conversion.
    pub fn to_contour(&self) -> workspace::Contour {
        match self {
            Self::Bezier(bezier) => bezier.to_contour(),
            Self::Hyper(hyper) => hyper.to_contour(),
        }
    }
}
