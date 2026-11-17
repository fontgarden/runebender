// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Point markers: the colors and the painting order that every point, handle and anchor shares.
//!
//! A marker is a halo, when the theme asks for one, then an interior, then a ring.
//! The halo keeps the marker's edge visible over the outline under it.
//! The interior masks what runs under the marker, and the ring has a constant width.

use masonry::imaging::{PaintShape, Painter};
use masonry::kurbo::Stroke;
use xilem::Color;

use super::theme::Palette;
use super::{
    POINT_CORNER_RADIUS, POINT_CURVE_RADIUS, POINT_HALO_EXTRA, POINT_RING_WIDTH, point_marker_scale,
};

/// What a point is, which sets its hue, its shape and its radius.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PointKind {
    /// An off-curve control point: a circle.
    OffCurve,
    /// A smooth on-curve point: a circle.
    Smooth,
    /// A corner on-curve point: a square.
    Corner,
}

impl PointKind {
    /// The kind of a point from its on-curve and smooth flags.
    pub(crate) fn of(on_curve: bool, smooth: bool) -> Self {
        match (on_curve, smooth) {
            (false, _) => Self::OffCurve,
            (true, true) => Self::Smooth,
            (true, false) => Self::Corner,
        }
    }

    /// Whether the marker is a square.
    pub(crate) fn is_square(self) -> bool {
        self == Self::Corner
    }

    /// The marker radius at unit scale.
    pub(crate) fn radius(self) -> f64 {
        if self.is_square() {
            POINT_CORNER_RADIUS
        } else {
            POINT_CURVE_RADIUS
        }
    }

    /// The theme hue for this kind.
    pub(crate) fn hue(self, pal: &Palette) -> Color {
        pal.role(match self {
            Self::OffCurve => "pointOffcurve",
            Self::Smooth => "pointSmooth",
            Self::Corner => "pointCorner",
        })
    }
}

/// The two colors of one marker.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ink {
    /// The constant-width ring.
    pub(crate) ring: Color,
    /// The fill inside the ring.
    pub(crate) interior: Color,
}

/// The keyline color that a filled marker and a selected marker use for the ring.
pub(crate) fn outline(pal: &Palette) -> Color {
    pal.point_outline.unwrap_or(pal.text)
}

/// The ink of a marker whose hue is `hue`, on `ground`.
///
/// A selected marker has the selection interior in the keyline ring.
/// A theme with filled points fills the hue inside the keyline; any other theme draws a hue ring
/// around the ground.
pub(crate) fn ink(pal: &Palette, hue: Color, selected: bool, ground: Color) -> Ink {
    if selected {
        Ink {
            ring: outline(pal),
            interior: pal.role("pointSelected"),
        }
    } else if pal.points_filled {
        Ink {
            ring: outline(pal),
            interior: hue,
        }
    } else {
        Ink {
            ring: hue,
            interior: ground,
        }
    }
}

/// The stroke widths of the markers at one zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Widths {
    /// The factor that marker radii are multiplied by.
    pub(crate) scale: f64,
    /// The ring width.
    pub(crate) ring: f64,
    /// The halo width.
    pub(crate) halo: f64,
}

/// The marker widths at `zoom`.
pub(crate) fn widths(zoom: f64) -> Widths {
    let scale = point_marker_scale(zoom);
    let ring = (POINT_RING_WIDTH * scale).max(super::Stroke::Hairline.px());
    Widths {
        scale,
        ring,
        halo: ring + POINT_HALO_EXTRA,
    }
}

/// The halo color, when the theme draws a halo.
pub(crate) fn halo(pal: &Palette) -> Option<Color> {
    pal.point_halo.then(|| pal.point_halo_fill())
}

/// Paint the halo, when there is one, and the interior.
pub(crate) fn paint_ground<S: ?Sized>(
    painter: &mut Painter<'_>,
    shape: &S,
    interior: Color,
    halo: Option<Color>,
    widths: Widths,
) where
    for<'b> &'b S: PaintShape<'b>,
{
    if let Some(halo) = halo {
        painter
            .stroke(shape, &Stroke::new(widths.halo), halo)
            .draw();
    }
    painter.fill(shape, interior).draw();
}

/// Paint the ring.
pub(crate) fn paint_ring<S: ?Sized>(
    painter: &mut Painter<'_>,
    shape: &S,
    ring: Color,
    widths: Widths,
) where
    for<'b> &'b S: PaintShape<'b>,
{
    painter
        .stroke(shape, &Stroke::new(widths.ring), ring)
        .draw();
}

/// Paint a whole marker: the halo, when there is one, the interior and the ring.
pub(crate) fn paint<S: ?Sized>(
    painter: &mut Painter<'_>,
    shape: &S,
    ink: Ink,
    halo: Option<Color>,
    widths: Widths,
) where
    for<'b> &'b S: PaintShape<'b>,
{
    paint_ground(painter, shape, ink.interior, halo, widths);
    paint_ring(painter, shape, ink.ring, widths);
}
