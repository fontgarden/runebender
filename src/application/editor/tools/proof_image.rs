// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Proof images for the chat model: a glyph and its proposal as pictures a
//! vision model can answer closed questions about.
//!
//! A vision model is unreliable at fine comparisons inside a busy picture
//! and cannot judge drawing quality, but it tells two well separated shapes
//! apart, picks the wider of two boxes, and reads a difference image. So
//! each image here is built for one question: the pair shows A, the
//! current drawing, and B, the proposal, side by side at one scale in
//! their advance boxes; the difference colours what only A has red, what
//! only B has blue, and what both have grey.
//!
//! The drawings are filled by a small scanline rasterizer of this module's
//! own, from outlines in font units, so the images depend on nothing but
//! the glyph data. Resolution is kept low on purpose: an image costs the
//! model about one token per 32 by 32 pixels.

use kurbo::{Affine, BezPath, Rect, Shape as _};
use serde_json::Value;

/// Width of the pair image, in pixels.
pub(super) const PAIR_WIDTH: u32 = 1024;
/// Height of the pair image and of the difference image, in pixels.
pub(super) const HEIGHT: u32 = 512;
/// Width of the difference image, in pixels.
pub(super) const DIFF_WIDTH: u32 = 512;
/// Samples per pixel on each axis while filling, for softer edges.
const SUPERSAMPLE: u32 = 2;
/// Ink at full coverage.
const INK: [u8; 4] = [0, 0, 0, 255];
/// Advance box lines.
const RULE: [u8; 4] = [120, 120, 120, 255];
/// Only in A.
const ONLY_A: [u8; 4] = [200, 40, 30, 255];
/// Only in B.
const ONLY_B: [u8; 4] = [30, 90, 210, 255];
/// In both.
const BOTH: [u8; 4] = [190, 190, 190, 255];

/// One glyph outline in font units, with its advance.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Drawing {
    pub(super) path: BezPath,
    pub(super) advance: f64,
}

impl Drawing {
    /// The drawing of the one glyph in a live `proof` result: its outline
    /// from the SVG the tool returns, in font units, and its advance from
    /// the metrics row.
    pub(super) fn from_proof(result: &Value) -> Result<Self, String> {
        let svg = result
            .get("svg_content")
            .and_then(Value::as_str)
            .ok_or("proof result has no svg_content")?;
        let metrics = result
            .get("metrics")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .ok_or("proof result has no metrics")?;
        let advance = metrics
            .get("advance")
            .and_then(Value::as_f64)
            .ok_or("proof metrics have no advance")?;
        // Each glyph is one `<path ... d="...">`; the sheet draws it with a
        // flip transform, so `d` itself is in font units with y up.
        let data = svg
            .find(" d=\"")
            .map(|at| &svg[at + 4..])
            .and_then(|rest| rest.find('"').map(|end| &rest[..end]))
            .unwrap_or("");
        let path = BezPath::from_svg(data).map_err(|error| format!("proof outline: {error}"))?;
        Ok(Self { path, advance })
    }
}

/// A canvas of RGBA pixels with a white background.
struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![255; (width * height * 4) as usize],
        }
    }

    fn blend(&mut self, x: u32, y: u32, colour: [u8; 4], coverage: f32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let at = ((y * self.width + x) * 4) as usize;
        for (slot, &over) in self.pixels[at..at + 3].iter_mut().zip(&colour[..3]) {
            let under = f32::from(*slot);
            let over = f32::from(over);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a blend of two bytes stays within a byte"
            )]
            let mixed = (under + (over - under) * coverage).round() as u8;
            *slot = mixed;
        }
    }

    /// Fill `path`, already in pixel coordinates with y down, with `colour`.
    fn fill(&mut self, path: &BezPath, colour: [u8; 4]) {
        let coverage = coverage(path, self.width, self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                let value = coverage[(y * self.width + x) as usize];
                if value > 0.0 {
                    self.blend(x, y, colour, value);
                }
            }
        }
    }

    fn fill_rect(&mut self, rect: Rect, colour: [u8; 4]) {
        self.fill(&rect.to_path(0.1), colour);
    }

    fn png(&self) -> Vec<u8> {
        let image = image::RgbaImage::from_vec(self.width, self.height, self.pixels.clone())
            .expect("canvas buffer matches its size");
        let mut bytes = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("PNG encoding of an in-memory image");
        bytes.into_inner()
    }
}

/// Per-pixel coverage of `path` in 0..=1, by sampling each pixel on a
/// `SUPERSAMPLE` by `SUPERSAMPLE` grid with the nonzero winding rule, as a
/// scanline pass over the flattened outline.
fn coverage(path: &BezPath, width: u32, height: u32) -> Vec<f32> {
    let mut edges: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut start = kurbo::Point::ZERO;
    let mut last = kurbo::Point::ZERO;
    kurbo::flatten(path.iter(), 0.1, |element| match element {
        kurbo::PathEl::MoveTo(point) => {
            start = point;
            last = point;
        }
        kurbo::PathEl::LineTo(point) => {
            edges.push((last.x, last.y, point.x, point.y));
            last = point;
        }
        kurbo::PathEl::ClosePath => {
            edges.push((last.x, last.y, start.x, start.y));
            last = start;
        }
        _ => {}
    });
    let samples = f64::from(SUPERSAMPLE);
    let mut result = vec![0.0_f32; (width * height) as usize];
    let mut crossings: Vec<(f64, i32)> = Vec::new();
    for y in 0..height {
        for sub_y in 0..SUPERSAMPLE {
            let sample_y = f64::from(y) + (f64::from(sub_y) + 0.5) / samples;
            crossings.clear();
            for &(x0, y0, x1, y1) in &edges {
                if (y0 <= sample_y) == (y1 <= sample_y) {
                    continue;
                }
                let t = (sample_y - y0) / (y1 - y0);
                let x = x0 + (x1 - x0) * t;
                crossings.push((x, if y1 > y0 { 1 } else { -1 }));
            }
            if crossings.is_empty() {
                continue;
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                if winding == 0 {
                    continue;
                }
                let (from, to) = (pair[0].0, pair[1].0);
                for x in 0..width {
                    for sub_x in 0..SUPERSAMPLE {
                        let sample_x = f64::from(x) + (f64::from(sub_x) + 0.5) / samples;
                        if sample_x >= from && sample_x < to {
                            result[(y * width + x) as usize] += 1.0;
                        }
                    }
                }
            }
        }
    }
    let per_pixel = (SUPERSAMPLE * SUPERSAMPLE) as f32;
    for value in &mut result {
        *value = (*value / per_pixel).min(1.0);
    }
    result
}

/// The transform placing a drawing in a cell `cell_width` wide and
/// `HEIGHT` tall: `scale` pixels per font unit, the baseline at
/// `baseline` pixels from the top, the advance box starting at the left
/// margin.
fn placement(scale: f64, baseline: f64, left: f64) -> Affine {
    Affine::translate((left, baseline)) * Affine::scale_non_uniform(scale, -scale)
}

/// One scale for both drawings: the taller of the two, from the lowest
/// to the highest point or the advance height, fills three quarters of the
/// cell, and the wider advance fits the cell width.
fn shared_scale(a: &Drawing, b: &Drawing, cell_width: f64) -> (f64, f64) {
    let bounds = a.path.bounding_box().union(b.path.bounding_box());
    let top = bounds.y1.max(1.0);
    let bottom = bounds.y0.min(0.0);
    let tallest = (top - bottom).max(1.0);
    let widest = a.advance.max(b.advance).max(bounds.width()).max(1.0);
    let scale = (f64::from(HEIGHT) * 0.75 / tallest).min(cell_width * 0.8 / widest);
    let baseline = f64::from(HEIGHT) * 0.5 + (top + bottom) / 2.0 * scale;
    (scale, baseline)
}

/// A and B side by side in their advance boxes, as PNG.
pub(super) fn pair_png(a: &Drawing, b: &Drawing) -> Vec<u8> {
    let cell = f64::from(PAIR_WIDTH) / 2.0;
    let (scale, baseline) = shared_scale(a, b, cell);
    let mut canvas = Canvas::new(PAIR_WIDTH, HEIGHT);
    for (index, drawing) in [a, b].into_iter().enumerate() {
        let left = cell * index as f64 + (cell - drawing.advance * scale) / 2.0;
        let transform = placement(scale, baseline, left);
        let right = left + drawing.advance * scale;
        let top = f64::from(HEIGHT) * 0.08;
        let bottom = f64::from(HEIGHT) * 0.92;
        canvas.fill_rect(Rect::new(left - 1.0, top, left + 1.0, bottom), RULE);
        canvas.fill_rect(Rect::new(right - 1.0, top, right + 1.0, bottom), RULE);
        canvas.fill_rect(Rect::new(left, baseline - 1.0, right, baseline + 1.0), RULE);
        canvas.fill(&(transform * drawing.path.clone()), INK);
    }
    // A thin divider between the cells.
    canvas.fill_rect(
        Rect::new(cell - 1.0, 0.0, cell + 1.0, f64::from(HEIGHT)),
        RULE,
    );
    canvas.png()
}

/// The difference image and what it counts, so the model is told what the
/// picture shows rather than left to guess it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Difference {
    pub(super) png: Vec<u8>,
    /// Pixels with ink only in A, the current drawing.
    pub(super) only_a: u64,
    /// Pixels with ink only in B, the proposal.
    pub(super) only_b: u64,
    /// Pixels with ink in both.
    pub(super) both: u64,
}

/// A over B at one origin: red where only A has ink, blue where only B
/// has ink, grey where both do, with the pixel counts.
pub(super) fn difference(a: &Drawing, b: &Drawing) -> Difference {
    let cell = f64::from(DIFF_WIDTH);
    let (scale, baseline) = shared_scale(a, b, cell);
    let widest = a.advance.max(b.advance) * scale;
    let left = (cell - widest) / 2.0;
    let transform = placement(scale, baseline, left);
    let cover_a = coverage(&(transform * a.path.clone()), DIFF_WIDTH, HEIGHT);
    let cover_b = coverage(&(transform * b.path.clone()), DIFF_WIDTH, HEIGHT);
    let mut canvas = Canvas::new(DIFF_WIDTH, HEIGHT);
    canvas.fill_rect(
        Rect::new(left, baseline - 1.0, left + widest, baseline + 1.0),
        RULE,
    );
    let (mut only_a, mut only_b, mut shared) = (0, 0, 0);
    for y in 0..HEIGHT {
        for x in 0..DIFF_WIDTH {
            let at = (y * DIFF_WIDTH + x) as usize;
            let (in_a, in_b) = (cover_a[at], cover_b[at]);
            let both = in_a.min(in_b);
            if both > 0.0 {
                canvas.blend(x, y, BOTH, both);
                shared += 1;
            }
            if in_a > both {
                canvas.blend(x, y, ONLY_A, in_a - both);
                only_a += 1;
            }
            if in_b > both {
                canvas.blend(x, y, ONLY_B, in_b - both);
                only_b += 1;
            }
        }
    }
    Difference {
        png: canvas.png(),
        only_a,
        only_b,
        both: shared,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, x1: f64, advance: f64) -> Drawing {
        Drawing {
            path: Rect::new(x0, 0.0, x1, 700.0).to_path(0.1),
            advance,
        }
    }

    fn decode(png: &[u8]) -> image::RgbaImage {
        image::load_from_memory(png).unwrap().to_rgba8()
    }

    #[test]
    fn a_drawing_is_read_from_a_proof_result() {
        let result = serde_json::json!({
            "svg_content": "<svg><path transform=\"translate(10 20) scale(1 -1)\" d=\"M0 0L100 0L100 700L0 700Z\" fill=\"black\"/></svg>",
            "metrics": [{"glyph":"I","advance":160.0}]
        });
        let drawing = Drawing::from_proof(&result).unwrap();
        assert_eq!(drawing.advance, 160.0);
        assert_eq!(
            drawing.path.bounding_box(),
            Rect::new(0.0, 0.0, 100.0, 700.0)
        );
        assert!(Drawing::from_proof(&serde_json::json!({"ok":true})).is_err());
    }

    #[test]
    fn the_pair_has_ink_in_both_cells_and_the_wider_box_on_the_right() {
        let png = pair_png(&square(50.0, 350.0, 400.0), &square(50.0, 350.0, 600.0));
        let image = decode(&png);
        assert_eq!((image.width(), image.height()), (PAIR_WIDTH, HEIGHT));
        let ink = |x0: u32, x1: u32| {
            image
                .enumerate_pixels()
                .filter(|(x, _, p)| *x >= x0 && *x < x1 && p[0] < 60)
                .count()
        };
        let left = ink(0, PAIR_WIDTH / 2);
        let right = ink(PAIR_WIDTH / 2, PAIR_WIDTH);
        assert!(
            left > 1000 && left.abs_diff(right) < left / 20,
            "same shape at one scale in both cells: {left} vs {right}"
        );
        // The rule pixels of the right box are further apart.
        let rules = |x0: u32, x1: u32| {
            let row = HEIGHT / 10;
            (x0..x1)
                .filter(|&x| image.get_pixel(x, row)[0] == RULE[0])
                .collect::<Vec<_>>()
        };
        let span = |xs: &[u32]| xs.last().copied().unwrap_or(0) - xs.first().copied().unwrap_or(0);
        assert!(span(&rules(0, PAIR_WIDTH / 2 - 2)) < span(&rules(PAIR_WIDTH / 2 + 2, PAIR_WIDTH)));
    }

    #[test]
    fn the_difference_colours_only_the_moved_side() {
        let moved = difference(&square(50.0, 350.0, 400.0), &square(50.0, 400.0, 450.0));
        assert_eq!(moved.only_a, 0);
        assert!(moved.only_b > 100 && moved.both > moved.only_b);
        let same = difference(&square(50.0, 350.0, 400.0), &square(50.0, 350.0, 500.0));
        assert_eq!(
            (same.only_a, same.only_b),
            (0, 0),
            "a wider box alone changes no ink"
        );
        let image = decode(&moved.png);
        let count = |colour: [u8; 4]| {
            image
                .pixels()
                .filter(|p| {
                    (p[0] as i32 - colour[0] as i32).abs() < 8
                        && (p[2] as i32 - colour[2] as i32).abs() < 8
                })
                .count()
        };
        assert!(count(ONLY_B) > 100, "the wider B has blue-only pixels");
        assert_eq!(count(ONLY_A), 0, "nothing is only in A");
        assert!(count(BOTH) > count(ONLY_B), "most of the shape is shared");
    }
}
