// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Ported from runebender-web/core/src/tool.rs (Apache-2.0), which
// ported the approach from runebender-xilem's knife tool.

//! The knife tool's slicing: cut closed contours along a line.
//!
//! Bezier paths slice in their own segment types: a cut cubic piece
//! stays cubic and a cut quadratic piece stays quadratic. Hyperbezier
//! paths convert to explicit cubics when actually cut. A single hit on
//! two nested contours joins them (cutting an O yields two Cs), unless a
//! contour is made of quadratic curves and lines only. Everything here
//! is UI-free: paths in, paths out.

use kurbo::{CubicBez, Line, ParamCurve, ParamCurveArclen, Point, Shape as _};

use crate::font::model::EntityId;
use crate::outline::path::{
    BezierPath, Path, PathPoint, PathPoints, PointType, Segment, SegmentInfo,
};

const MAX_KNIFE_RECURSE: usize = 16;
const KNIFE_HIT_CLUSTER_TOLERANCE: f64 = 1e-4;

#[derive(Clone, Copy)]
struct Hit {
    line_t: f64,
    segment_t: f64,
    point: Point,
    segment_info: SegmentInfo,
}

#[derive(Clone)]
struct SingleHitPath {
    path: BezierPath,
    hit: Hit,
}

#[derive(Clone)]
enum SliceItem {
    Paths(Vec<Path>),
    SingleHit(SingleHitPath),
}

/// Cut every closed path in `paths` along `line` and return the
/// resulting paths.
///
/// Paths the line misses come back unchanged. A single hit on two
/// nested contours joins them.
pub fn slice_paths(paths: &[Path], line: Line) -> Vec<Path> {
    let mut items = Vec::new();
    for path in paths {
        match path {
            Path::Bezier(bezier_path) => {
                // A contour of quadratic curves and lines is never joined to a nested contour.
                let single = if is_quadratic(bezier_path) {
                    None
                } else {
                    single_closed_hit(bezier_path, line)
                };
                if let Some(hit) = single {
                    items.push(SliceItem::SingleHit(SingleHitPath {
                        path: bezier_path.clone(),
                        hit,
                    }));
                } else {
                    let mut sliced = Vec::new();
                    slice_path(bezier_path, line, &mut sliced);
                    items.push(SliceItem::Paths(sliced));
                }
            }
            Path::Hyper(hyper_path) => {
                let cubic_path = hyper_path.to_cubic();
                if let Some(hit) = single_closed_hit(&cubic_path, line) {
                    items.push(SliceItem::SingleHit(SingleHitPath {
                        path: cubic_path,
                        hit,
                    }));
                } else if hit_count(&cubic_path, line) <= 1 {
                    items.push(SliceItem::Paths(vec![Path::Hyper(hyper_path.clone())]));
                } else {
                    let mut sliced = Vec::new();
                    slice_path(&cubic_path, line, &mut sliced);
                    items.push(SliceItem::Paths(sliced));
                }
            }
        }
    }
    coalesce_single_hit_compound_cuts(items)
}

/// Whether a path has a quadratic segment and no cubic segment, as a UFO contour with `qcurve`
/// points and no `curve` point has.
fn is_quadratic(path: &BezierPath) -> bool {
    let mut quadratic = false;
    for segment in path.iter_segments() {
        match segment.segment {
            Segment::Cubic(_) => return false,
            Segment::Quadratic(_) => quadratic = true,
            Segment::Line(_) => {}
        }
    }
    quadratic
}

fn collect_hits(path: &BezierPath, line: Line, hits: &mut Vec<Hit>) {
    hits.clear();
    for segment in path.iter_segments() {
        for (segment_t, line_t) in intersect_line_segment(line, &segment.segment) {
            hits.push(Hit {
                line_t,
                segment_t,
                point: line.eval(line_t),
                segment_info: segment,
            });
        }
    }
    sort_and_dedup_knife_hits(hits, line);
}

fn single_closed_hit(path: &BezierPath, line: Line) -> Option<Hit> {
    if !path.closed {
        return None;
    }
    let mut hits = Vec::new();
    collect_hits(path, line, &mut hits);
    if hits.len() == 1 { Some(hits[0]) } else { None }
}

fn hit_count(path: &BezierPath, line: Line) -> usize {
    let mut hits = Vec::new();
    collect_hits(path, line, &mut hits);
    hits.len()
}

fn coalesce_single_hit_compound_cuts(items: Vec<SliceItem>) -> Vec<Path> {
    let mut out = Vec::new();
    let mut consumed = vec![false; items.len()];

    for i in 0..items.len() {
        if consumed[i] {
            continue;
        }

        match &items[i] {
            SliceItem::Paths(paths) => {
                out.extend(paths.clone());
            }
            SliceItem::SingleHit(first) => {
                let mut paired = None;
                for j in (i + 1)..items.len() {
                    if consumed[j] {
                        continue;
                    }
                    let SliceItem::SingleHit(second) = &items[j] else {
                        continue;
                    };
                    if paths_are_nested(&first.path, &second.path) {
                        paired = Some(j);
                        break;
                    }
                }

                if let Some(j) = paired {
                    let SliceItem::SingleHit(second) = &items[j] else {
                        unreachable!("paired item must be a single-hit path");
                    };
                    out.push(Path::Bezier(join_single_hit_paths(first, second)));
                    consumed[j] = true;
                } else {
                    out.push(Path::Bezier(first.path.clone()));
                }
            }
        }
    }

    out
}

fn slice_path(path: &BezierPath, line: Line, acc: &mut Vec<Path>) {
    let mut hits = Vec::new();
    slice_path_impl(path.clone(), line, acc, &mut hits, 0);
}

fn slice_path_impl(
    path: BezierPath,
    line: Line,
    acc: &mut Vec<Path>,
    hit_buf: &mut Vec<Hit>,
    recurse: usize,
) {
    hit_buf.clear();
    for segment in path.iter_segments() {
        for (segment_t, line_t) in intersect_line_segment(line, &segment.segment) {
            hit_buf.push(Hit {
                line_t,
                segment_t,
                point: line.eval(line_t),
                segment_info: segment,
            });
        }
    }

    if hit_buf.len() <= 1 || recurse == MAX_KNIFE_RECURSE {
        acc.push(Path::Bezier(path));
        return;
    }

    sort_and_dedup_knife_hits(hit_buf, line);

    if hit_buf.len() <= 1 {
        acc.push(Path::Bezier(path));
        return;
    }

    let start = hit_buf[0];
    let end = hit_buf[1];
    let slice_ep = 1.0 / line.arclen(1e-6).max(1.0);
    let next_line_start_t = (end.line_t + slice_ep).min(1.0);
    let (start, end) = order_points(&path, start, end);
    let (path_one, path_two) = split_path_at_intersections(&path, start, end);

    if next_line_start_t >= 1.0 {
        acc.push(Path::Bezier(path_one));
        acc.push(Path::Bezier(path_two));
        return;
    }

    let remaining_line = line_subsegment(line, next_line_start_t, 1.0);
    slice_path_impl(path_one, remaining_line, acc, hit_buf, recurse + 1);
    slice_path_impl(path_two, remaining_line, acc, hit_buf, recurse + 1);
}

fn order_points(path: &BezierPath, start: Hit, end: Hit) -> (Hit, Hit) {
    for segment in path.iter_segments() {
        if segment.start_index == start.segment_info.start_index {
            if segment.start_index == end.segment_info.start_index
                && end.segment_t < start.segment_t
            {
                return (end, start);
            }
            return (start, end);
        } else if segment.start_index == end.segment_info.start_index {
            return (end, start);
        }
    }
    (start, end)
}

fn sort_and_dedup_knife_hits(hits: &mut Vec<Hit>, line: Line) {
    hits.sort_by(|a, b| a.line_t.total_cmp(&b.line_t));

    if hits.len() <= 1 {
        return;
    }

    let line_len = (line.p1 - line.p0).hypot();
    let line_t_tolerance = if line_len > 1e-6 {
        KNIFE_HIT_CLUSTER_TOLERANCE / line_len
    } else {
        f64::INFINITY
    };

    let mut deduped = Vec::with_capacity(hits.len());
    let mut cluster = Vec::new();
    for hit in hits.drain(..) {
        if cluster
            .last()
            .map(|previous: &Hit| {
                (hit.line_t - previous.line_t).abs() <= line_t_tolerance
                    || hit.point.distance(previous.point) <= KNIFE_HIT_CLUSTER_TOLERANCE
            })
            .unwrap_or(false)
        {
            cluster.push(hit);
        } else {
            push_preferred_knife_hit(&mut deduped, &cluster);
            cluster.clear();
            cluster.push(hit);
        }
    }
    push_preferred_knife_hit(&mut deduped, &cluster);
    *hits = deduped;
}

fn push_preferred_knife_hit(dest: &mut Vec<Hit>, cluster: &[Hit]) {
    const ENDPOINT_TOLERANCE: f64 = 1e-6;
    if cluster
        .iter()
        .any(|hit| hit.segment_t > ENDPOINT_TOLERANCE && hit.segment_t < 1.0 - ENDPOINT_TOLERANCE)
        || cluster.len() != 2
        || !are_adjacent_endpoint_hits(cluster[0], cluster[1])
    {
        dest.extend_from_slice(cluster);
        return;
    }

    let Some(best) = cluster.iter().min_by(|a, b| {
        let a_endpoint = a.segment_t.min(1.0 - a.segment_t);
        let b_endpoint = b.segment_t.min(1.0 - b.segment_t);
        a_endpoint
            .total_cmp(&b_endpoint)
            .then_with(|| a.segment_t.total_cmp(&b.segment_t))
            .then_with(|| a.segment_info.start_index.cmp(&b.segment_info.start_index))
    }) else {
        return;
    };
    dest.push(*best);
}

fn are_adjacent_endpoint_hits(a: Hit, b: Hit) -> bool {
    let a_endpoint_index = if a.segment_t <= 1e-6 {
        a.segment_info.start_index
    } else {
        a.segment_info.end_index
    };
    let b_endpoint_index = if b.segment_t <= 1e-6 {
        b.segment_info.start_index
    } else {
        b.segment_info.end_index
    };
    a_endpoint_index == b_endpoint_index
}

fn split_path_at_intersections(
    path: &BezierPath,
    start: Hit,
    end: Hit,
) -> (BezierPath, BezierPath) {
    let mut one_points = Vec::new();
    let mut two_points = Vec::new();
    let mut two_is_done = false;

    let points = path.points.to_vec();
    let segments = path.iter_segments().collect::<Vec<_>>();

    for segment in &segments {
        if segment.start_index != start.segment_info.start_index {
            append_segment_points(&mut one_points, &points, segment);
        } else {
            append_subsegment_points(&mut one_points, &points, segment, 0.0, start.segment_t);

            if segment.start_index == end.segment_info.start_index {
                append_subsegment_points(&mut one_points, &points, segment, end.segment_t, 1.0);
                append_subsegment_points(
                    &mut two_points,
                    &points,
                    segment,
                    start.segment_t,
                    end.segment_t,
                );
                two_is_done = true;
            } else {
                append_subsegment_points(&mut two_points, &points, segment, start.segment_t, 1.0);
            }

            if !path.closed {
                two_points.push(PathPoint {
                    id: EntityId::next(),
                    point: start.point,
                    typ: PointType::OnCurve { smooth: false },
                });
            }
            break;
        }
    }

    let mut found_start = false;
    for segment in &segments {
        if segment.start_index == start.segment_info.start_index {
            found_start = true;
            continue;
        }
        if !found_start {
            continue;
        }

        if segment.start_index == end.segment_info.start_index {
            append_subsegment_points(&mut one_points, &points, segment, end.segment_t, 1.0);
            if !two_is_done {
                append_subsegment_points(&mut two_points, &points, segment, 0.0, end.segment_t);
            }
            break;
        } else if !two_is_done {
            append_segment_points(&mut two_points, &points, segment);
        }
    }

    let mut found_end = false;
    for segment in &segments {
        if segment.start_index == end.segment_info.start_index {
            found_end = true;
            continue;
        }
        if found_end {
            append_segment_points(&mut one_points, &points, segment);
        }
    }

    if one_points.first().map(|p| p.point) == one_points.last().map(|p| p.point)
        && one_points.len() > 1
    {
        one_points.pop();
    }

    (
        BezierPath::new(PathPoints::from_vec(one_points), path.closed),
        BezierPath::new(PathPoints::from_vec(two_points), true),
    )
}

fn join_single_hit_paths(first: &SingleHitPath, second: &SingleHitPath) -> BezierPath {
    let mut points = open_path_at_hit(&first.path, first.hit);
    push_path_point(
        &mut points,
        second.hit.point,
        PointType::OnCurve { smooth: false },
    );

    for point in open_path_at_hit(&second.path, second.hit) {
        push_path_point(&mut points, point.point, point.typ);
    }

    if points.first().map(|point| point.point) == points.last().map(|point| point.point)
        && points.len() > 1
    {
        points.pop();
    }

    BezierPath::new(PathPoints::from_vec(points), true)
}

fn open_path_at_hit(path: &BezierPath, hit: Hit) -> Vec<PathPoint> {
    let points = path.points.to_vec();
    let segments = path.iter_segments().collect::<Vec<_>>();
    let Some(hit_segment_index) = segments.iter().position(|segment| {
        segment.start_index == hit.segment_info.start_index
            && segment.end_index == hit.segment_info.end_index
    }) else {
        return points;
    };

    let mut out = Vec::new();
    let hit_segment = &segments[hit_segment_index];
    push_path_point(&mut out, hit.point, PointType::OnCurve { smooth: false });
    append_subsegment_points(&mut out, &points, hit_segment, hit.segment_t, 1.0);

    for offset in 1..segments.len() {
        let segment = &segments[(hit_segment_index + offset) % segments.len()];
        append_segment_points(&mut out, &points, segment);
    }

    append_subsegment_points(&mut out, &points, hit_segment, 0.0, hit.segment_t);
    push_path_point(&mut out, hit.point, PointType::OnCurve { smooth: false });
    out
}

fn paths_are_nested(a: &BezierPath, b: &BezierPath) -> bool {
    let Some(a_sample) = representative_oncurve(a) else {
        return false;
    };
    let Some(b_sample) = representative_oncurve(b) else {
        return false;
    };

    a.to_bezpath().contains(b_sample) || b.to_bezpath().contains(a_sample)
}

fn representative_oncurve(path: &BezierPath) -> Option<Point> {
    path.points
        .iter()
        .find(|point| point.is_on_curve())
        .map(|point| point.point)
}

fn append_segment_points(dest: &mut Vec<PathPoint>, points: &[PathPoint], segment: &SegmentInfo) {
    let start = segment.start_index;
    let end = segment.end_index;

    if end <= start {
        let start_typ = points[start].typ;
        match segment.segment {
            Segment::Cubic(cubic) => {
                push_path_point(dest, cubic.p0, start_typ);
                push_path_point(dest, cubic.p1, PointType::OffCurve { auto: false });
                push_path_point(dest, cubic.p2, PointType::OffCurve { auto: false });
                push_path_point(dest, cubic.p3, points[end].typ);
                return;
            }
            Segment::Line(line) => {
                push_path_point(dest, line.p0, start_typ);
                push_path_point(dest, line.p1, points[end].typ);
                return;
            }
            Segment::Quadratic(quad) => {
                push_path_point(dest, quad.p0, start_typ);
                push_path_point(dest, quad.p1, PointType::OffCurve { auto: false });
                push_path_point(dest, quad.p2, points[end].typ);
                return;
            }
        }
    }

    push_path_point(dest, points[start].point, points[start].typ);
    for point in points.iter().take(end).skip(start + 1) {
        push_path_point(dest, point.point, point.typ);
    }
    if end < points.len() && end != start {
        push_path_point(dest, points[end].point, points[end].typ);
    }
}

fn append_subsegment_points(
    dest: &mut Vec<PathPoint>,
    points: &[PathPoint],
    segment: &SegmentInfo,
    t_start: f64,
    t_end: f64,
) {
    if t_start >= t_end {
        return;
    }

    const T_EPS: f64 = 1e-9;
    let start_typ = if t_start < T_EPS {
        points[segment.start_index].typ
    } else {
        PointType::OnCurve { smooth: false }
    };
    let end_typ = if t_end > 1.0 - T_EPS {
        points[segment.end_index].typ
    } else {
        PointType::OnCurve { smooth: false }
    };

    match segment.segment {
        Segment::Line(line) => {
            push_path_point(dest, line.eval(t_start), start_typ);
            push_path_point(dest, line.eval(t_end), end_typ);
        }
        Segment::Cubic(cubic) => {
            let sub = cubic_subsegment(cubic, t_start, t_end);
            push_path_point(dest, sub.p0, start_typ);
            push_path_point(dest, sub.p1, PointType::OffCurve { auto: false });
            push_path_point(dest, sub.p2, PointType::OffCurve { auto: false });
            push_path_point(dest, sub.p3, end_typ);
        }
        Segment::Quadratic(quad) => {
            let sub = quadratic_subsegment(quad, t_start, t_end);
            push_path_point(dest, sub.p0, start_typ);
            push_path_point(dest, sub.p1, PointType::OffCurve { auto: false });
            push_path_point(dest, sub.p2, end_typ);
        }
    }
}

fn push_path_point(dest: &mut Vec<PathPoint>, point: Point, typ: PointType) {
    if dest
        .last()
        .is_some_and(|pt| pt.point == point && pt.typ.is_on_curve() && typ.is_on_curve())
    {
        return;
    }
    dest.push(PathPoint {
        id: EntityId::next(),
        point,
        typ,
    });
}
fn line_subsegment(line: Line, t_start: f64, t_end: f64) -> Line {
    Line::new(line.eval(t_start), line.eval(t_end))
}

fn cubic_subsegment(cubic: CubicBez, t_start: f64, t_end: f64) -> CubicBez {
    let (_, right) = Segment::subdivide_cubic(cubic, t_start);
    let adjusted_t = if t_start < 1.0 {
        (t_end - t_start) / (1.0 - t_start)
    } else {
        1.0
    };
    let (left, _) = Segment::subdivide_cubic(right, adjusted_t.min(1.0));
    left
}

fn quadratic_subsegment(quad: kurbo::QuadBez, t_start: f64, t_end: f64) -> kurbo::QuadBez {
    let (_, right) = Segment::subdivide_quadratic(quad, t_start);
    let adjusted_t = if t_start < 1.0 {
        (t_end - t_start) / (1.0 - t_start)
    } else {
        1.0
    };
    let (left, _) = Segment::subdivide_quadratic(right, adjusted_t.min(1.0));
    left
}

fn intersect_line_segment(line: Line, segment: &Segment) -> Vec<(f64, f64)> {
    match segment {
        Segment::Line(seg_line) => intersect_line_line(line, *seg_line),
        Segment::Cubic(cubic) => intersect_line_cubic(line, *cubic),
        Segment::Quadratic(quad) => intersect_line_cubic(line, quad.raise()),
    }
}

fn intersect_line_line(measure: Line, segment: Line) -> Vec<(f64, f64)> {
    let d1 = measure.p1 - measure.p0;
    let d2 = segment.p1 - segment.p0;
    let cross = d1.x * d2.y - d1.y * d2.x;
    const EPSILON: f64 = 1e-9;
    if cross.abs() < EPSILON {
        return Vec::new();
    }
    let d = segment.p0 - measure.p0;
    let line_t = (d.x * d2.y - d.y * d2.x) / cross;
    let segment_t = (d.x * d1.y - d.y * d1.x) / cross;
    if (0.0..=1.0).contains(&line_t) && (0.0..=1.0).contains(&segment_t) {
        vec![(segment_t, line_t)]
    } else {
        Vec::new()
    }
}

fn intersect_line_cubic(line: Line, cubic: CubicBez) -> Vec<(f64, f64)> {
    let d = line.p1 - line.p0;
    let a = -d.y;
    let b = d.x;
    let c = -(a * line.p0.x + b * line.p0.y);
    let d0 = a * cubic.p0.x + b * cubic.p0.y + c;
    let d1 = a * cubic.p1.x + b * cubic.p1.y + c;
    let d2 = a * cubic.p2.x + b * cubic.p2.y + c;
    let d3 = a * cubic.p3.x + b * cubic.p3.y + c;
    let roots = solve_cubic(
        -d0 + 3.0 * d1 - 3.0 * d2 + d3,
        3.0 * d0 - 6.0 * d1 + 3.0 * d2,
        -3.0 * d0 + 3.0 * d1,
        d0,
    );
    let mut results = Vec::new();
    let line_len_sq = d.hypot2();
    const EPSILON: f64 = 1e-9;
    for t in roots {
        let pt = cubic.eval(t);
        let line_t = if line_len_sq > EPSILON {
            let v = pt - line.p0;
            (v.x * d.x + v.y * d.y) / line_len_sq
        } else {
            0.0
        };
        if (0.0..=1.0).contains(&line_t) {
            results.push((t, line_t));
        }
    }
    results
}

fn solve_cubic(a: f64, b: f64, c: f64, d: f64) -> Vec<f64> {
    let mut roots = Vec::new();
    const EPSILON: f64 = 1e-9;
    if a.abs() < EPSILON {
        if b.abs() < EPSILON {
            if c.abs() > EPSILON {
                let t = -d / c;
                if (0.0..=1.0).contains(&t) {
                    roots.push(t);
                }
            }
        } else {
            let disc = c * c - 4.0 * b * d;
            if disc >= 0.0 {
                let sqrt_disc = disc.sqrt();
                for t in [(-c + sqrt_disc) / (2.0 * b), (-c - sqrt_disc) / (2.0 * b)] {
                    if (0.0..=1.0).contains(&t)
                        && !roots.iter().any(|&r: &f64| (r - t).abs() < EPSILON)
                    {
                        roots.push(t);
                    }
                }
            }
        }
        return roots;
    }

    let p = b / a;
    let q = c / a;
    let r = d / a;
    let p1 = q - p * p / 3.0;
    let q1 = r - p * q / 3.0 + 2.0 * p * p * p / 27.0;
    let disc = q1 * q1 / 4.0 + p1 * p1 * p1 / 27.0;

    if disc > EPSILON {
        let sqrt_disc = disc.sqrt();
        let u = (-q1 / 2.0 + sqrt_disc).cbrt();
        let v = (-q1 / 2.0 - sqrt_disc).cbrt();
        let t = u + v - p / 3.0;
        if (0.0..=1.0).contains(&t) {
            roots.push(t);
        }
    } else if disc.abs() <= EPSILON {
        if q1.abs() < EPSILON {
            let t = -p / 3.0;
            if (0.0..=1.0).contains(&t) {
                roots.push(t);
            }
        } else {
            let u = (q1 / 2.0).cbrt();
            for t in [2.0 * u - p / 3.0, -u - p / 3.0] {
                if (0.0..=1.0).contains(&t) && !roots.iter().any(|&r: &f64| (r - t).abs() < EPSILON)
                {
                    roots.push(t);
                }
            }
        }
    } else {
        let m = 2.0 * (-p1 / 3.0).sqrt();
        let theta = (3.0 * q1 / (p1 * m)).acos() / 3.0;
        for k in 0..3 {
            let t = m * (theta - 2.0 * std::f64::consts::PI * k as f64 / 3.0).cos() - p / 3.0;
            if (0.0..=1.0).contains(&t) && !roots.iter().any(|&r: &f64| (r - t).abs() < EPSILON) {
                roots.push(t);
            }
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{PathEl, Rect};

    fn path_point(point: Point, smooth: bool) -> PathPoint {
        PathPoint {
            id: EntityId::next(),
            point,
            typ: PointType::OnCurve { smooth },
        }
    }

    fn off_curve(point: Point) -> PathPoint {
        PathPoint {
            id: EntityId::next(),
            point,
            typ: PointType::OffCurve { auto: false },
        }
    }

    #[cfg(test)]
    fn rect_path(rect: Rect) -> Path {
        let points = vec![
            path_point(rect.origin(), false),
            path_point(Point::new(rect.max_x(), rect.min_y()), false),
            path_point(Point::new(rect.max_x(), rect.max_y()), false),
            path_point(Point::new(rect.min_x(), rect.max_y()), false),
        ];
        Path::Bezier(BezierPath::new(PathPoints::from_vec(points), true))
    }

    #[cfg(test)]
    fn quadratic_curve_path() -> Path {
        let points = vec![
            path_point(Point::new(0.0, 0.0), false),
            off_curve(Point::new(50.0, 100.0)),
            path_point(Point::new(100.0, 0.0), true),
            path_point(Point::new(100.0, 100.0), false),
            path_point(Point::new(0.0, 100.0), false),
        ];
        Path::Bezier(BezierPath::new(PathPoints::from_vec(points), true))
    }

    #[cfg(test)]
    fn hyper_curve_path() -> Path {
        let points = vec![
            path_point(Point::new(0.0, 0.0), true),
            path_point(Point::new(100.0, 100.0), true),
            path_point(Point::new(0.0, 100.0), true),
            path_point(Point::new(100.0, 0.0), true),
        ];
        Path::Hyper(crate::outline::path::HyperPath::from_points(
            PathPoints::from_vec(points),
            true,
        ))
    }

    #[cfg(test)]
    fn rounded_icon_counter_path() -> Path {
        let points = vec![
            path_point(Point::new(96.0, 128.0), true),
            path_point(Point::new(96.0, 416.0), true),
            off_curve(Point::new(96.0, 440.0)),
            off_curve(Point::new(104.0, 448.0)),
            path_point(Point::new(128.0, 448.0), true),
            path_point(Point::new(640.0, 448.0), true),
            off_curve(Point::new(663.5, 447.5)),
            off_curve(Point::new(671.5, 439.5)),
            path_point(Point::new(672.0, 416.0), true),
            path_point(Point::new(672.0, 128.0), true),
            off_curve(Point::new(671.5, 103.5)),
            off_curve(Point::new(663.5, 95.5)),
            path_point(Point::new(640.0, 96.0), true),
            path_point(Point::new(128.0, 96.0), true),
            off_curve(Point::new(103.5, 95.5)),
            off_curve(Point::new(95.5, 103.5)),
        ];
        Path::Bezier(BezierPath::new(PathPoints::from_vec(points), true))
    }
    #[test]
    fn knife_splits_closed_rectangle_into_two_paths() {
        let paths = vec![rect_path(Rect::new(0.0, 0.0, 100.0, 100.0))];
        let sliced = slice_paths(
            &paths,
            Line::new(Point::new(50.0, -10.0), Point::new(50.0, 110.0)),
        );

        assert_eq!(sliced.len(), 2);
        for path in sliced {
            let Path::Bezier(path) = path else {
                panic!("knife should keep a bezier path");
            };
            assert!(path.closed);
            assert!(path.points.len() >= 4);
        }
    }

    #[test]
    fn knife_splits_closed_rectangle_through_on_curve_vertices() {
        let paths = vec![rect_path(Rect::new(0.0, 0.0, 100.0, 100.0))];
        let sliced = slice_paths(
            &paths,
            Line::new(Point::new(-10.0, -10.0), Point::new(110.0, 110.0)),
        );

        assert_eq!(sliced.len(), 2);
        for path in sliced {
            let Path::Bezier(path) = path else {
                panic!("knife should keep a bezier path");
            };
            assert!(path.closed);
            assert!(path.points.len() >= 3);
        }
    }

    #[test]
    fn knife_splits_each_closed_contour_crossed_by_line() {
        let paths = vec![
            rect_path(Rect::new(0.0, 0.0, 100.0, 100.0)),
            rect_path(Rect::new(25.0, 25.0, 75.0, 75.0)),
        ];
        let sliced = slice_paths(
            &paths,
            Line::new(Point::new(50.0, -10.0), Point::new(50.0, 110.0)),
        );

        assert_eq!(sliced.len(), 4);
        assert!(sliced.iter().all(|path| match path {
            Path::Bezier(path) => path.closed,
            _ => false,
        }));
    }

    #[test]
    fn knife_connects_nested_contours_with_one_hit_each() {
        let paths = vec![
            rect_path(Rect::new(0.0, 0.0, 100.0, 100.0)),
            rect_path(Rect::new(25.0, 25.0, 75.0, 75.0)),
        ];
        let sliced = slice_paths(
            &paths,
            Line::new(Point::new(-10.0, 50.0), Point::new(50.0, 50.0)),
        );

        assert_eq!(sliced.len(), 1);
        let Path::Bezier(path) = &sliced[0] else {
            panic!("nested bezier contours should stay bezier");
        };
        assert!(path.closed);
        assert!(
            path.points.len() > 8,
            "joined contour should keep both contour outlines plus bridge points"
        );
    }

    #[test]
    fn knife_preserves_path_without_two_intersections() {
        let paths = vec![rect_path(Rect::new(0.0, 0.0, 100.0, 100.0))];
        let sliced = slice_paths(
            &paths,
            Line::new(Point::new(150.0, -10.0), Point::new(150.0, 110.0)),
        );

        assert_eq!(sliced.len(), 1);
    }

    #[test]
    fn knife_splits_quadratic_curve_segments_without_raising_to_cubic() {
        let paths = vec![quadratic_curve_path()];
        let sliced = slice_paths(
            &paths,
            Line::new(Point::new(50.0, -10.0), Point::new(50.0, 110.0)),
        );

        assert_eq!(sliced.len(), 2);
        for path in sliced {
            let Path::Bezier(path) = path else {
                panic!("knife should keep a bezier path");
            };
            assert!(
                path.points.iter().any(PathPoint::is_off_curve),
                "sliced quadratic curve should retain quadratic control points"
            );
            assert!(is_quadratic(&path), "no quadratic piece may become cubic");
        }
    }

    #[test]
    fn knife_splits_hyperbezier_as_explicit_cubic_paths() {
        let paths = vec![hyper_curve_path()];
        let sliced = slice_paths(
            &paths,
            Line::new(Point::new(50.0, -10.0), Point::new(50.0, 110.0)),
        );

        assert_eq!(sliced.len(), 2);
        for path in sliced {
            let Path::Bezier(path) = path else {
                panic!("knife should convert sliced hyperbeziers to cubic paths");
            };
            assert!(path.closed);
            assert!(
                path.points.iter().any(PathPoint::is_off_curve),
                "sliced hyperbezier should retain explicit cubic controls"
            );
        }
    }

    #[test]
    fn knife_subsegments_keep_endpoint_overlapping_cubic_handles() {
        let points = vec![
            path_point(Point::new(0.0, 0.0), true),
            off_curve(Point::new(0.0, 0.0)),
            off_curve(Point::new(80.0, 100.0)),
            path_point(Point::new(100.0, 0.0), true),
        ];
        let path = BezierPath::new(PathPoints::from_vec(points.clone()), false);
        let segment = path
            .iter_segments()
            .next()
            .expect("test path should have a cubic segment");
        let mut split_points = vec![path_point(Point::new(0.0, 0.0), false)];

        append_subsegment_points(&mut split_points, &points, &segment, 0.0, 1.0);

        assert_eq!(
            split_points
                .iter()
                .filter(|point| point.is_off_curve())
                .count(),
            2,
            "endpoint-overlapping handles must not be deduped away"
        );
        let rebuilt = BezierPath::new(PathPoints::from_vec(split_points), false);
        assert!(
            rebuilt
                .to_bezpath()
                .elements()
                .iter()
                .any(|element| matches!(element, PathEl::CurveTo(_, _, _))),
            "the rebuilt segment should remain cubic, not collapse to a line"
        );
    }

    /// A box from `min` to `max` whose bottom edge is one quadratic curve.
    fn curved_quadratic_box(min: f64, max: f64) -> Path {
        let points = vec![
            path_point(Point::new(min, min), false),
            off_curve(Point::new((min + max) / 2.0, min - 10.0)),
            path_point(Point::new(max, min), false),
            path_point(Point::new(max, max), false),
            path_point(Point::new(min, max), false),
        ];
        Path::Bezier(BezierPath::new(PathPoints::from_vec(points), true))
    }

    /// Every sliced contour as saved: point types and coordinates, one contour per line.
    fn saved_contours(paths: &[Path]) -> String {
        paths
            .iter()
            .map(|path| {
                path.to_contour()
                    .points
                    .iter()
                    .map(|point| format!("{:?} {:.3} {:.3}", point.point_type, point.x, point.y))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Exact output of the knife for each contour kind. The expected values were recorded before
    /// the quadratic path type was merged into the bezier path type, so they show that cutting a
    /// cubic, a quadratic and a hyperbezier contour gives the same contours as before.
    #[test]
    fn knife_output_is_exact_for_cubic_quadratic_and_hyperbezier_contours() {
        let vertical = Line::new(Point::new(50.0, -10.0), Point::new(50.0, 110.0));
        let cubic = slice_paths(
            &[rounded_icon_counter_path()],
            Line::new(Point::new(256.0, 80.0), Point::new(256.0, 480.0)),
        );
        assert_eq!(
            saved_contours(&cubic),
            [
                "OffCurve 95.500 103.500, Curve 96.000 128.000, Line 96.000 416.000, OffCurve 96.000 440.000, OffCurve 104.000 448.000, Curve 128.000 448.000, Line 256.000 448.000, Line 256.000 96.000, Line 128.000 96.000, OffCurve 103.500 95.500",
                "Line 256.000 96.000, Line 256.000 448.000, Line 640.000 448.000, OffCurve 663.500 447.500, OffCurve 671.500 439.500, Curve 672.000 416.000, Line 672.000 128.000, OffCurve 671.500 103.500, OffCurve 663.500 95.500, Curve 640.000 96.000",
            ]
            .join("\n")
        );
        let quadratic = slice_paths(&[quadratic_curve_path()], vertical);
        assert_eq!(
            saved_contours(&quadratic),
            [
                "Line 0.000 100.000, Line 0.000 0.000, OffCurve 25.000 50.000, QCurve 50.000 50.000, Line 50.000 100.000",
                "Line 50.000 100.000, Line 50.000 50.000, OffCurve 75.000 50.000, QCurve 100.000 0.000, Line 100.000 100.000",
            ]
            .join("\n")
        );
        // A hyperbezier cut: point types, and the sums of the coordinates of each contour.
        let hyper = slice_paths(&[hyper_curve_path()], vertical);
        assert_eq!(hyper.len(), 2);
        for (path, (types, sum_x, sum_y)) in hyper.iter().zip([
            (
                "o c o o c o o c o o c o o c l o o c o o c o o c o o c o o c o o c o",
                1_700.0,
                535.839_278,
            ),
            (
                "c l o o c o o c o o c o o c o o c o o c o o c o o c o o c o o c o o",
                1_700.0,
                2_864.160_722,
            ),
        ]) {
            let contour = path.to_contour();
            let kinds: Vec<_> = contour
                .points
                .iter()
                .map(|point| match point.point_type {
                    crate::outline::path::hyper_model::PointType::OffCurve => "o",
                    crate::outline::path::hyper_model::PointType::Curve => "c",
                    crate::outline::path::hyper_model::PointType::Line => "l",
                    _ => "?",
                })
                .collect();
            assert_eq!(kinds.join(" "), types);
            let x: f64 = contour.points.iter().map(|point| point.x).sum();
            let y: f64 = contour.points.iter().map(|point| point.y).sum();
            assert!(
                (x - sum_x).abs() < 1e-5 && (y - sum_y).abs() < 1e-5,
                "{x} {y}"
            );
        }

        // One hit on each of two nested contours joins contours that have a cubic segment or
        // only lines, but not contours of quadratic curves and lines.
        let nested = |outer: Path, inner: Path| {
            saved_contours(&slice_paths(
                &[outer, inner],
                Line::new(Point::new(-10.0, 50.0), Point::new(50.0, 50.0)),
            ))
        };
        assert_eq!(
            nested(
                curved_quadratic_box(0.0, 100.0),
                curved_quadratic_box(25.0, 75.0)
            ),
            [
                "Line 0.000 100.000, Line 0.000 0.000, OffCurve 50.000 -10.000, QCurve 100.000 0.000, Line 100.000 100.000",
                "Line 25.000 75.000, Line 25.000 25.000, OffCurve 50.000 15.000, QCurve 75.000 25.000, Line 75.000 75.000",
            ]
            .join("\n")
        );
        assert_eq!(
            nested(
                rect_path(Rect::new(0.0, 0.0, 100.0, 100.0)),
                rect_path(Rect::new(25.0, 25.0, 75.0, 75.0))
            ),
            [
                "Line 25.000 50.000, Line 0.000 50.000, Line 0.000 0.000, Line 100.000 0.000, Line 100.000 100.000, Line 0.000 100.000, Line 0.000 50.000, Line 25.000 50.000, Line 25.000 25.000, Line 75.000 25.000, Line 75.000 75.000, Line 25.000 75.000",
            ]
            .join("\n")
        );

        let hits = knife_hit_points_for_paths(
            &[quadratic_curve_path(), hyper_curve_path()],
            Point::new(50.0, -10.0),
            Point::new(50.0, 110.0),
        );
        assert_eq!(hits.len(), 2);
        assert!((hits[0] - Point::new(50.0, 50.0)).hypot() < 1e-9);
        assert!((hits[1] - Point::new(50.0, 100.0)).hypot() < 1e-9);
    }

    #[test]
    fn knife_keeps_a_cut_quadratic_piece_of_a_mixed_contour_quadratic() {
        let points = vec![
            path_point(Point::new(0.0, 0.0), false),
            off_curve(Point::new(50.0, -40.0)),
            path_point(Point::new(100.0, 0.0), false),
            off_curve(Point::new(110.0, 30.0)),
            off_curve(Point::new(110.0, 70.0)),
            path_point(Point::new(100.0, 100.0), false),
            path_point(Point::new(0.0, 100.0), false),
        ];
        let mixed = Path::Bezier(BezierPath::new(PathPoints::from_vec(points), true));
        let sliced = slice_paths(
            &[mixed],
            Line::new(Point::new(50.0, -50.0), Point::new(50.0, 150.0)),
        );
        assert_eq!(sliced.len(), 2);
        let types = |path: &Path| -> Vec<_> {
            path.to_contour()
                .points
                .iter()
                .map(|point| point.point_type)
                .collect()
        };
        use crate::outline::path::hyper_model::PointType as Saved;
        for path in &sliced {
            assert_eq!(
                types(path)
                    .iter()
                    .filter(|kind| **kind == Saved::QCurve)
                    .count(),
                1,
                "each half of the cut quadratic stays one quadratic segment"
            );
        }
        assert!(
            sliced
                .iter()
                .any(|path| types(path).contains(&Saved::Curve)),
            "the uncut cubic segment stays cubic"
        );
    }

    #[test]
    fn knife_splits_rounded_icon_counter() {
        let paths = vec![rounded_icon_counter_path()];
        let sliced = slice_paths(
            &paths,
            Line::new(Point::new(256.0, 80.0), Point::new(256.0, 480.0)),
        );

        assert_eq!(sliced.len(), 2);
        assert!(sliced.iter().all(|path| match path {
            Path::Bezier(path) => path.closed,
            _ => false,
        }));
    }
}

/// Find where a knife line crosses canonical document contours.
pub fn knife_hit_points_in_layer(
    layer: crate::font::LayerView<'_>,
    p0: Point,
    p1: Point,
) -> Vec<Point> {
    let paths: Vec<_> = layer.contours().map(Path::from_document_contour).collect();
    knife_hit_points_for_paths(&paths, p0, p1)
}

fn knife_hit_points_for_paths(paths: &[Path], p0: Point, p1: Point) -> Vec<Point> {
    let line = Line::new(p0, p1);
    let mut ts: Vec<f64> = Vec::new();
    for path in paths {
        let bezier = match path {
            Path::Bezier(bezier) => bezier.clone(),
            Path::Hyper(hyper) => hyper.to_cubic(),
        };
        for segment in bezier.iter_segments() {
            for (_, line_t) in intersect_line_segment(line, &segment.segment) {
                ts.push(line_t);
            }
        }
    }
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    ts.into_iter().map(|t| line.eval(t)).collect()
}

#[cfg(test)]
mod layer_tests {
    use super::*;
    use crate::font::project::{Project, SourceInput};
    use crate::font::variable::{GlyphLayerAddress, SourceId};

    /// Cut one glyph of a project along a line and report whether its topology changed.
    fn knife_cut(project: &mut Project, glyph: &str, p0: Point, p1: Point) -> bool {
        let layer = project
            .document_source(SourceId(0))
            .expect("default source")
            .default_layer();
        let mut transaction = project
            .begin_document_layer_transaction(&GlyphLayerAddress {
                glyph: glyph.into(),
                layer,
            })
            .expect("fixture transaction");
        let changed = transaction
            .draft_mut()
            .knife_cut(p0, p1)
            .expect("finite knife line");
        project
            .commit_document_layer_transaction(transaction)
            .expect("knife commit");
        changed
    }

    fn contour_count(project: &Project, glyph: &str) -> usize {
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        project
            .document_layer(glyph, &layer)
            .unwrap()
            .contours()
            .count()
    }

    #[test]
    fn knife_cuts_fixture_zero_into_four_contours() {
        let mut project = Project::load(&crate::testing::fonts::regular_ufo()).expect("fixture");
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        assert_eq!(contour_count(&project, "zero"), 2);
        let (p0, p1) = (Point::new(340.0, -200.0), Point::new(340.0, 900.0));
        let hits =
            knife_hit_points_in_layer(project.document_layer("zero", &layer).unwrap(), p0, p1);
        assert_eq!(hits.len(), 4, "vertical line should cross zero 4 times");
        assert!(knife_cut(&mut project, "zero", p0, p1));
        assert_eq!(contour_count(&project, "zero"), 4);
    }

    #[test]
    fn knife_cuts_a_rectangle_in_two() {
        let mut glyph = norad::Glyph::new("test");
        let points = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)].map(|(x, y)| {
            norad::ContourPoint::new(x, y, norad::PointType::Line, false, None, None)
        });
        glyph
            .contours
            .push(norad::Contour::new(points.to_vec(), None));
        let mut font = norad::Font::new();
        font.default_layer_mut().insert_glyph(glyph);
        let mut project = Project::from_source(SourceInput::from_font(font, "Knife.ufo".into()))
            .expect("valid source fixture");

        // A miss changes nothing.
        assert!(!knife_cut(
            &mut project,
            "test",
            Point::new(200.0, -50.0),
            Point::new(300.0, 150.0)
        ));
        assert_eq!(contour_count(&project, "test"), 1);

        // A horizontal cut yields two closed contours.
        assert!(knife_cut(
            &mut project,
            "test",
            Point::new(-10.0, 50.0),
            Point::new(110.0, 50.0)
        ));
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let view = project.document_layer("test", &layer).unwrap();
        assert_eq!(view.contours().count(), 2);
        for contour in view.contours() {
            assert!(contour.is_closed());
            assert!(contour.points().count() >= 4);
        }
    }
}
