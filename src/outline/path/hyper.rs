// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Ported from runebender-xilem/src/path/hyper.rs (Apache-2.0).

//! Hyperbezier path representation.
//!
//! A hyperbezier path stores only on-curve points. Off-curve control
//! points are automatically computed by the `spline` crate's solver to
//! create smooth G2-continuous curves.

use super::point::{PathPoint, PointType};
use super::point_list::PathPoints;
use crate::font::model::entity_id::EntityId;
use crate::outline::path::hyper_model as workspace;
use kurbo::{BezPath, Point};
use spline::SplineSpec;
use std::sync::Arc;

/// A single contour represented as a hyperbezier path.
///
/// Unlike cubic paths, hyperbezier paths only store on-curve points.
/// Control points are automatically computed by the spline solver.
#[derive(Debug, Clone)]
pub struct HyperPath {
    /// The on-curve points only; the solver derives the handles.
    pub points: PathPoints,
    /// Whether the contour is closed.
    pub closed: bool,
    /// Unique identity for selection and hit testing.
    pub id: EntityId,
    /// Cached bezier path for rendering.
    bezier: Arc<BezPath>,
}

impl HyperPath {
    /// Create a new hyper path from existing points.
    pub fn from_points(points: PathPoints, closed: bool) -> Self {
        let mut path = Self {
            points,
            closed,
            id: EntityId::next(),
            bezier: Arc::new(BezPath::new()),
        };

        path.rebuild_bezier();
        path
    }

    /// Creates an open path with no points and an empty bezier cache.
    pub fn empty() -> Self {
        Self {
            points: PathPoints::new(),
            closed: false,
            id: EntityId::next(),
            bezier: Arc::new(BezPath::new()),
        }
    }

    /// Returns the number of on-curve points.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Returns `true` when the path has no points.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Returns the path's on-curve points.
    pub fn points(&self) -> &PathPoints {
        &self.points
    }

    /// Whether the last segment connects back to the first point.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Returns a copy of the cached solved bezier.
    pub fn to_bezpath(&self) -> BezPath {
        (*self.bezier).clone()
    }

    /// Appends the cached solved bezier's elements to `path`.
    pub fn append_to_bezpath(&self, path: &mut BezPath) {
        for el in self.bezier.elements() {
            path.push(*el);
        }
    }

    /// Expand the solved spline into a `CubicPath` with explicit
    /// on-curve and off-curve control points.
    pub fn to_cubic(&self) -> super::cubic::CubicPath {
        use super::cubic::CubicPath;

        let mut points = Vec::new();

        for el in self.bezier.elements() {
            match el {
                kurbo::PathEl::MoveTo(p) => {
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p,
                        typ: PointType::OnCurve { smooth: true },
                    });
                }
                kurbo::PathEl::LineTo(p) => {
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p,
                        typ: PointType::OnCurve { smooth: false },
                    });
                }
                kurbo::PathEl::CurveTo(p1, p2, p3) => {
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p1,
                        typ: PointType::OffCurve { auto: false },
                    });
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p2,
                        typ: PointType::OffCurve { auto: false },
                    });
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p3,
                        typ: PointType::OnCurve { smooth: true },
                    });
                }
                kurbo::PathEl::QuadTo(p1, p2) => {
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p1,
                        typ: PointType::OffCurve { auto: false },
                    });
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p2,
                        typ: PointType::OnCurve { smooth: true },
                    });
                }
                kurbo::PathEl::ClosePath => {
                    // Handled by closed flag.
                }
            }
        }

        CubicPath::new(PathPoints::from_vec(points), self.closed)
    }

    /// Rebuild the bezier cache from points using the spline solver.
    fn rebuild_bezier(&mut self) {
        let num_points = self.points.len();

        if num_points == 0 || num_points < 2 {
            self.bezier = Arc::new(BezPath::new());
            return;
        }

        // spline solves on kurbo 0.9 — convert at the boundary.
        let mut spec = SplineSpec::new();

        #[inline(always)]
        fn to_spline_point(p: Point) -> kurbo_09::Point {
            kurbo_09::Point::new(p.x, p.y)
        }

        let mut points_iter = self.points.iter();

        let Some(first) = points_iter.next() else {
            return;
        };
        let first_point = first.point;
        spec.move_to(to_spline_point(first_point));

        for pt in points_iter {
            spec.spline_to(None, None, to_spline_point(pt.point), true);
        }

        if self.closed && num_points >= 3 {
            spec.spline_to(None, None, to_spline_point(first_point), true);
            spec.close();
        }

        let spline = spec.solve();
        let spline_bezpath = spline.render();

        // Convert spline's kurbo 0.9 BezPath back to kurbo 0.12.
        let mut result = BezPath::new();
        for el in spline_bezpath.elements() {
            match el {
                kurbo_09::PathEl::MoveTo(p) => {
                    result.move_to(Point::new(p.x, p.y));
                }
                kurbo_09::PathEl::LineTo(p) => {
                    result.line_to(Point::new(p.x, p.y));
                }
                kurbo_09::PathEl::QuadTo(p1, p2) => {
                    result.quad_to(Point::new(p1.x, p1.y), Point::new(p2.x, p2.y));
                }
                kurbo_09::PathEl::CurveTo(p1, p2, p3) => {
                    result.curve_to(
                        Point::new(p1.x, p1.y),
                        Point::new(p2.x, p2.y),
                        Point::new(p3.x, p3.y),
                    );
                }
                kurbo_09::PathEl::ClosePath => {
                    result.close_path();
                }
            }
        }

        self.bezier = Arc::new(result);
    }

    /// Convert from a workspace contour. Only on-curve points are kept;
    /// off-curve handles will be recomputed by the spline solver.
    pub fn from_contour(contour: &workspace::Contour) -> Self {
        if contour.points.is_empty() {
            return Self::empty();
        }

        let closed = !matches!(contour.points[0].point_type, workspace::PointType::Move);

        let mut path_points: Vec<PathPoint> = contour
            .points
            .iter()
            .filter(|pt| !matches!(pt.point_type, workspace::PointType::OffCurve))
            .map(|pt| {
                let smooth = !matches!(pt.point_type, workspace::PointType::HyperCorner);
                PathPoint {
                    id: EntityId::next(),
                    point: Point::new(pt.x, pt.y),
                    typ: PointType::OnCurve { smooth },
                }
            })
            .collect();

        if closed && !path_points.is_empty() {
            path_points.rotate_left(1);
        }

        Self::from_points(PathPoints::from_vec(path_points), closed)
    }

    /// Save only the on-curve hyperbezier points with smooth/corner
    /// flags. Off-curve handles are recomputed by the solver on load.
    pub fn to_contour(&self) -> workspace::Contour {
        use crate::outline::path::hyper_model::{Contour, ContourPoint, PointType as WsPointType};

        let mut points = Vec::new();

        for (i, path_point) in self.points.iter().enumerate() {
            let point_type = if i == 0 && !self.closed {
                WsPointType::Move
            } else {
                match path_point.typ {
                    PointType::OnCurve { smooth: true } => WsPointType::Hyper,
                    PointType::OnCurve { smooth: false } => WsPointType::HyperCorner,
                    PointType::OffCurve { .. } => {
                        // HyperPath should only contain on-curve points;
                        // skip any stray off-curve as a data-integrity
                        // safeguard.
                        continue;
                    }
                }
            };

            points.push(ContourPoint {
                x: path_point.point.x,
                y: path_point.point.y,
                point_type,
                smooth: false,
            });
        }

        if self.closed && !points.is_empty() {
            points.rotate_right(1);
        }

        Contour { points }
    }
}
