// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lasso geometry for the label tool.
//!
//! A freehand drag that comes back near where it started is a loop, and a loop becomes a
//! polygon with few corners.

use kurbo::{Line, Point};

/// Whether a freehand drag is a loop: it is longer than three times `near` and ends within
/// `near`, or a fifth of its length, of where it started. `near` is in the units of the points.
pub fn is_loop(points: &[Point], near: f64) -> bool {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return false;
    };
    let length: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    length >= near && length > near * 3.0 && first.distance(*last) <= near.max(length * 0.2)
}

/// A freehand loop as a polygon with few corners (Ramer-Douglas-Peucker).
///
/// The tolerance grows until the polygon has at most `max_corners`, and the result keeps
/// at least three corners.
pub fn simplify_loop(points: &[Point], tolerance: f64, max_corners: usize) -> Vec<Point> {
    if points.len() <= 3 {
        return points.to_vec();
    }
    let mut tolerance = tolerance.max(1e-3);
    loop {
        let mut keep = vec![false; points.len()];
        keep[0] = true;
        keep[points.len() - 1] = true;
        douglas_peucker(points, 0, points.len() - 1, tolerance, &mut keep);
        let mut corners: Vec<Point> = points
            .iter()
            .zip(&keep)
            .filter(|(_, keep)| **keep)
            .map(|(p, _)| *p)
            .collect();
        // The loop closes itself: a last corner on top of the first is noise.
        if corners.len() > 3 && corners[0].distance(corners[corners.len() - 1]) <= tolerance {
            corners.pop();
        }
        if corners.len() <= max_corners.max(3) || tolerance > 1e6 {
            return corners;
        }
        tolerance *= 1.5;
    }
}

fn douglas_peucker(points: &[Point], first: usize, last: usize, tolerance: f64, keep: &mut [bool]) {
    if last <= first + 1 {
        return;
    }
    let line = Line::new(points[first], points[last]);
    let (mut farthest, mut distance) = (first, 0.0);
    for (index, point) in points.iter().enumerate().take(last).skip(first + 1) {
        let d = distance_to_segment(*point, line);
        if d > distance {
            distance = d;
            farthest = index;
        }
    }
    if distance > tolerance {
        keep[farthest] = true;
        douglas_peucker(points, first, farthest, tolerance, keep);
        douglas_peucker(points, farthest, last, tolerance, keep);
    }
}

fn distance_to_segment(point: Point, line: Line) -> f64 {
    let along = line.p1 - line.p0;
    let length2 = along.dot(along);
    if length2 < 1e-12 {
        return point.distance(line.p0);
    }
    let t = ((point - line.p0).dot(along) / length2).clamp(0.0, 1.0);
    point.distance(line.p0 + along * t)
}

/// The nearest point on `line` to `point`, and its distance.
pub fn nearest_on_segment(point: Point, line: Line) -> (Point, f64) {
    let along = line.p1 - line.p0;
    let length2 = along.dot(along);
    let t = if length2 < 1e-12 {
        0.0
    } else {
        ((point - line.p0).dot(along) / length2).clamp(0.0, 1.0)
    };
    let at = line.p0 + along * t;
    (at, point.distance(at))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loop_simplifies_to_few_corners_and_an_open_drag_is_no_loop() {
        let circle: Vec<Point> = (0..200)
            .map(|i| {
                let t = f64::from(i) / 200.0 * std::f64::consts::TAU;
                Point::new(100.0 * t.cos(), 100.0 * t.sin())
            })
            .collect();
        let corners = simplify_loop(&circle, 2.0, 12);
        assert!(
            corners.len() >= 3 && corners.len() <= 12,
            "{}",
            corners.len()
        );
        assert!(is_loop(&circle, 8.0));

        let across = [
            Point::new(150.0, -10.0),
            Point::new(150.0, 50.0),
            Point::new(150.0, 110.0),
        ];
        assert!(!is_loop(&across, 8.0));
        assert!(!is_loop(&[Point::new(0.0, 0.0), Point::new(4.0, 0.0)], 8.0));
    }
}
