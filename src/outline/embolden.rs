// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Add weight by pushing the outline outward, learning how far from
//! glyphs already drawn in both masters.
//!
//! This is the workflow of drawing the key glyphs and letting them
//! carry the rest: draw `n`, `o`, `H`, `O` in the heavier master, and
//! every other glyph moves by what those moved.
//!
//! There is no model here, and on some scripts that is the point. A
//! model trained on Latin measured worse than a uniform shift on
//! Arabic, while this needs no training data at all, only a few
//! reference glyphs somebody drew.
//!
//! Points are moved, never added or removed, so the result stays
//! interpolation-compatible with what it came from. That is a
//! guarantee of the method rather than something to check afterwards.
//!
//! The offset is anisotropic because type is: a vertical stem and a
//! horizontal bar do not gain the same weight. Virtua Grotesk grows
//! its verticals by 96 units and its bars by 72.

use crate::font::LayerView;

/// How far to push, per axis, in font units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Offset {
    /// Applied to the horizontal part of a point's outward normal, so
    /// it sets how much vertical stems gain.
    pub x: f64,
    /// Applied to the vertical part, setting how much bars gain.
    pub y: f64,
}

/// Unit normal at each point, pointing the way that adds weight.
///
/// Always the right of travel, with no winding test. Under the usual
/// convention, where an outer contour runs counter-clockwise and a
/// counter runs the other way, the right of travel is outward on the
/// outer contour and into the hole on a counter. Both thicken the ink,
/// which is what is wanted.
///
/// Flipping this by winding, as an earlier version did, pushes
/// counters open and thins every bowl in the font.
///
/// The tangent comes from the neighbouring points rather than the
/// curve, so an off-curve control moves with the shape around it
/// instead of being treated as if it sat on the outline.
pub fn outward_normals_for_points(points: &[kurbo::Point]) -> Vec<(f64, f64)> {
    let n = points.len();
    if n < 3 {
        return vec![(0.0, 0.0); n];
    }
    (0..n)
        .map(|i| {
            let previous = points[(i + n - 1) % n];
            let next = points[(i + 1) % n];
            let (tx, ty) = (next.x - previous.x, next.y - previous.y);
            let len = (tx * tx + ty * ty).sqrt();
            if len < 1e-9 {
                return (0.0, 0.0);
            }
            (ty / len, -tx / len)
        })
        .collect()
}

/// Learn an anisotropic embolden offset from canonical document-layer pairs.
///
/// Incompatible contour or point structures are skipped.
pub fn learn_layer_offset(pairs: &[(LayerView<'_>, LayerView<'_>)]) -> Option<Offset> {
    let (mut sx, mut nx2, mut sy, mut ny2) = (0.0, 0.0, 0.0, 0.0);
    for (light, heavy) in pairs {
        let light_contours: Vec<_> = light.contours().collect();
        let heavy_contours: Vec<_> = heavy.contours().collect();
        if light_contours.len() != heavy_contours.len() {
            continue;
        }
        for (light, heavy) in light_contours.into_iter().zip(heavy_contours) {
            let light_points: Vec<_> = light.points().map(|point| point.position()).collect();
            let heavy_points: Vec<_> = heavy.points().map(|point| point.position()).collect();
            if light_points.len() != heavy_points.len() {
                continue;
            }
            for ((light, heavy), (nx, ny)) in light_points
                .iter()
                .zip(&heavy_points)
                .zip(outward_normals_for_points(&light_points))
            {
                sx += nx * (heavy.x - light.x);
                nx2 += nx * nx;
                sy += ny * (heavy.y - light.y);
                ny2 += ny * ny;
            }
        }
    }
    if nx2 < 1e-9 || ny2 < 1e-9 {
        return None;
    }
    let (x, y) = (sx / nx2, sy / ny2);
    if x.abs() < 1e-6 && y.abs() < 1e-6 {
        return None;
    }
    Some(Offset { x, y })
}

/// How much the advance grows from the lighter to the heavier master,
/// as the mean over pairs whose structures match.
///
/// `None` when no pair is compatible.
pub fn learn_layer_advance_delta(pairs: &[(LayerView<'_>, LayerView<'_>)]) -> Option<f64> {
    let deltas: Vec<f64> = pairs
        .iter()
        .filter(|(light, heavy)| light.contours().count() == heavy.contours().count())
        .map(|(light, heavy)| heavy.width() - light.width())
        .collect();
    if deltas.is_empty() {
        return None;
    }
    Some(deltas.iter().sum::<f64>() / deltas.len() as f64)
}

/// Snap a value to the nearest multiple of `grid`; a grid of zero or
/// less leaves it alone.
pub fn snap_to_grid(value: f64, grid: f64) -> f64 {
    if grid > 0.0 {
        (value / grid).round() * grid
    } else {
        value
    }
}

/// Where every point of a layer lands after pushing it outward, in
/// contour order, one entry per point.
///
/// Each point's move, not its position, is snapped to `grid`, so a
/// draft stays on the machine's lattice while an optical correction
/// already in the lighter master survives in the heavier one. A grid
/// of zero or less does not snap.
pub fn layer_point_moves(layer: LayerView<'_>, offset: Offset, grid: f64) -> Vec<kurbo::Point> {
    layer
        .contours()
        .flat_map(|contour| {
            let points: Vec<_> = contour.points().map(|point| point.position()).collect();
            let normals = outward_normals_for_points(&points);
            points
                .into_iter()
                .zip(normals)
                .map(move |(point, (nx, ny))| {
                    kurbo::Point::new(
                        point.x + snap_to_grid(nx * offset.x, grid),
                        point.y + snap_to_grid(ny * offset.y, grid),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::project::{Project, SourceInput};
    use crate::font::variable::{LayerId, SourceId};
    use norad::{Contour, ContourPoint, Glyph, PointType};

    fn pt(x: f64, y: f64) -> ContourPoint {
        ContourPoint::new(x, y, PointType::Line, false, None, None)
    }

    /// Counter-clockwise, the way an outer contour is drawn.
    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Contour {
        Contour::new(vec![pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)], None)
    }

    fn glyph_of(name: &str, contours: Vec<Contour>) -> Glyph {
        let mut glyph = Glyph::new(name);
        glyph.width = 500.0;
        glyph.contours = contours;
        glyph
    }

    fn project_of(glyphs: Vec<Glyph>) -> (Project, LayerId) {
        let mut font = norad::Font::new();
        for glyph in glyphs {
            font.default_layer_mut().insert_glyph(glyph);
        }
        let project = Project::from_source(SourceInput::from_font(font, "Embolden.ufo".into()))
            .expect("valid source fixture");
        let layer = project
            .document_source(SourceId(0))
            .expect("default source")
            .default_layer();
        (project, layer)
    }

    /// The glyph with every point moved where `layer_point_moves` puts it.
    fn embolden(glyph: &Glyph, offset: Offset) -> Glyph {
        let (project, layer) = project_of(vec![glyph.clone()]);
        let view = project
            .document_layer(glyph.name().as_str(), &layer)
            .expect("fixture layer");
        let mut moves = layer_point_moves(view, offset, 0.0).into_iter();
        // The canonical layer fixes the point order that the moves follow.
        let mut out = crate::formats::ufo::glyph_from_layer(view);
        for point in out
            .contours
            .iter_mut()
            .flat_map(|contour| &mut contour.points)
        {
            let moved = moves.next().expect("one move per point");
            point.x = moved.x;
            point.y = moved.y;
        }
        assert!(moves.next().is_none());
        out
    }

    /// Learn an offset from one light and heavy pair of glyphs.
    fn learn_offset(light: &Glyph, heavy: &Glyph) -> Option<Offset> {
        let heavy = glyph_of("heavy", heavy.contours.clone());
        let (project, layer) = project_of(vec![light.clone(), heavy]);
        let light = project
            .document_layer(light.name().as_str(), &layer)
            .unwrap();
        let heavy = project.document_layer("heavy", &layer).unwrap();
        learn_layer_offset(&[(light, heavy)])
    }

    #[test]
    fn an_outer_contour_grows() {
        let g = glyph_of("test", vec![square(0.0, 0.0, 100.0, 100.0)]);
        let out = embolden(&g, Offset { x: 10.0, y: 10.0 });
        let xs: Vec<f64> = out.contours[0].points.iter().map(|p| p.x).collect();
        assert!(xs.iter().cloned().fold(f64::MAX, f64::min) < 0.0, "{xs:?}");
        assert!(
            xs.iter().cloned().fold(f64::MIN, f64::max) > 100.0,
            "{xs:?}"
        );
    }

    /// A counter is wound the other way, so pushing outward has to
    /// shrink it. Getting this backwards fills in every bowl.
    #[test]
    fn a_counter_shrinks() {
        let mut inner = square(20.0, 20.0, 80.0, 80.0);
        inner.points.reverse();
        let g = glyph_of("test", vec![square(0.0, 0.0, 100.0, 100.0), inner]);
        let out = embolden(&g, Offset { x: 10.0, y: 10.0 });
        let xs: Vec<f64> = out.contours[1].points.iter().map(|p| p.x).collect();
        let (lo, hi) = (
            xs.iter().cloned().fold(f64::MAX, f64::min),
            xs.iter().cloned().fold(f64::MIN, f64::max),
        );
        assert!(
            lo > 20.0 && hi < 80.0,
            "the counter should close up: {xs:?}"
        );
    }

    /// The whole reason to move points rather than re-draw: the result
    /// still interpolates with what it came from.
    #[test]
    fn the_structure_is_untouched() {
        let g = glyph_of("test", vec![square(0.0, 0.0, 100.0, 100.0)]);
        let (project, layer) = project_of(vec![g.clone()]);
        let view = project.document_layer("test", &layer).unwrap();
        let moves = layer_point_moves(view, Offset { x: 10.0, y: 4.0 }, 0.0);
        assert_eq!(moves.len(), g.contours[0].points.len());
    }

    /// The point of the module: a reference pair says how far to push,
    /// and applying that to the reference reproduces it.
    #[test]
    fn a_reference_pair_teaches_its_own_offset() {
        let light = glyph_of("light", vec![square(0.0, 0.0, 100.0, 100.0)]);
        let heavy = embolden(&light, Offset { x: 12.0, y: 6.0 });
        let learned = learn_offset(&light, &heavy).expect("learnable");
        assert!((learned.x - 12.0).abs() < 1e-6, "{learned:?}");
        assert!((learned.y - 6.0).abs() < 1e-6, "{learned:?}");
    }

    /// Type gains weight differently on each axis, so a single number
    /// would be wrong for one of them.
    #[test]
    fn the_two_axes_are_learned_apart() {
        let light = glyph_of("light", vec![square(0.0, 0.0, 200.0, 100.0)]);
        let heavy = embolden(&light, Offset { x: 48.0, y: 12.0 });
        let learned = learn_offset(&light, &heavy).expect("learnable");
        assert!((learned.x - 48.0).abs() < 1e-6);
        assert!((learned.y - 12.0).abs() < 1e-6);
        assert_ne!(learned.x, learned.y);
    }

    #[test]
    fn incompatible_pairs_are_skipped() {
        let light = glyph_of("light", vec![square(0.0, 0.0, 100.0, 100.0)]);
        let mut odd = light.clone();
        odd.contours[0].points.push(pt(50.0, 50.0));
        assert!(learn_offset(&light, &odd).is_none());
    }

    #[test]
    fn a_pair_that_did_not_move_teaches_nothing() {
        let light = glyph_of("light", vec![square(0.0, 0.0, 100.0, 100.0)]);
        assert!(learn_offset(&light, &light).is_none());
    }
}
