// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Ported from runebender-xilem/src/model/workspace.rs (Apache-2.0).
//
// Strips: anyhow/norad/file I/O, Arc<RwLock> wrappers, the
// `read_workspace` / `write_workspace` lock helpers, and Workspace::path.
// UFO load/save will be provided by the JS host (via fetch / File API)
// and round-tripped as serialized JSON across the wasm-bindgen boundary.

//! Font data model: owned, single-threaded data structures.
//!
//! These are the Xilem-era glyph types. They no longer hold the live
//! font model, which is now `norad` based. Today they survive as the
//! intermediate form that `crate::outline::path` uses to convert contours to and
//! from hyperbezier paths. See `Contour::from_norad` and
//! `norad_contour_is_hyper`.

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// A contour is a closed path.
#[derive(Debug, Clone)]
pub struct Contour {
    /// The points in contour order.
    pub points: Vec<ContourPoint>,
}

/// A point in a contour.
#[derive(Debug, Clone)]
pub struct ContourPoint {
    /// X coordinate in font units.
    pub x: f64,
    /// Y coordinate in font units.
    pub y: f64,
    /// How the point takes part in the path; see `PointType`.
    pub point_type: PointType,
    /// UFO smooth attribute: tangent continuity.
    pub smooth: bool,
}

/// Point type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointType {
    /// The first point of an open contour.
    Move,
    /// An on-curve point reached by a straight line.
    Line,
    /// An off-curve control handle.
    OffCurve,
    /// An on-curve point ending a cubic segment.
    Curve,
    /// An on-curve point ending a quadratic segment.
    QCurve,
    /// Hyperbezier smooth point (on-curve, auto control points).
    Hyper,
    /// Hyperbezier corner point (on-curve, independent segments).
    HyperCorner,
}

// ============================================================================
// NORAD CONVERSION
// ============================================================================

/// Whether a norad contour is a hyperbezier.
///
/// The identifier convention is shared by all Runebender editors.
#[cfg(test)]
pub fn norad_contour_is_hyper(contour: &norad::Contour) -> bool {
    contour
        .identifier()
        .map(|id| id.as_ref().contains("hyper"))
        .unwrap_or(false)
}

impl Contour {
    /// Convert from a norad contour.
    ///
    /// A hyperbezier contour, marked by an identifier containing
    /// "hyper", maps curve and move points to smooth hyper points
    /// and line points to hyper corners.
    #[cfg(test)]
    pub fn from_norad(contour: &norad::Contour) -> Self {
        let hyper = norad_contour_is_hyper(contour);
        Self {
            points: contour
                .points
                .iter()
                .map(|p| ContourPoint {
                    x: p.x,
                    y: p.y,
                    smooth: p.smooth,
                    point_type: match (hyper, p.typ) {
                        (true, norad::PointType::Curve | norad::PointType::Move) => {
                            PointType::Hyper
                        }
                        (true, norad::PointType::Line) => PointType::HyperCorner,
                        (_, norad::PointType::Move) => PointType::Move,
                        (_, norad::PointType::Line) => PointType::Line,
                        (_, norad::PointType::OffCurve) => PointType::OffCurve,
                        (_, norad::PointType::Curve) => PointType::Curve,
                        (_, norad::PointType::QCurve) => PointType::QCurve,
                    },
                })
                .collect(),
        }
    }

    /// Convert back to a norad contour, setting the hyperbezier
    /// identifier when the contour carries hyper points.
    #[cfg(test)]
    pub fn to_norad(&self) -> norad::Contour {
        let hyper = self
            .points
            .iter()
            .any(|p| matches!(p.point_type, PointType::Hyper | PointType::HyperCorner));
        let points = self
            .points
            .iter()
            .map(|p| {
                norad::ContourPoint::new(
                    p.x,
                    p.y,
                    match p.point_type {
                        PointType::Move => norad::PointType::Move,
                        PointType::Line | PointType::HyperCorner => norad::PointType::Line,
                        PointType::OffCurve => norad::PointType::OffCurve,
                        PointType::Curve | PointType::Hyper => norad::PointType::Curve,
                        PointType::QCurve => norad::PointType::QCurve,
                    },
                    p.smooth,
                    None,
                    None,
                )
            })
            .collect();
        let identifier = hyper.then(|| norad::Identifier::new("hyperbezier").expect("static id"));
        norad::Contour::new(points, identifier)
    }
}
