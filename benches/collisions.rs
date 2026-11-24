// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The cost of the collision test of one shaped line; see `docs/BENCHMARKS.md`.
//!
//! The line has 20 letters in right-to-left order.
//! Each letter is a curved body with a dot above and a dot below, and carries a mark above;
//! most also carry a mark below.
//! Neighbors overlap by 20 units, as joined letters do.
//! Each letter has its own outline key, as each glyph of a real line has its own name and
//! revision.

use std::hint::black_box;
use std::time::{Duration, Instant};

use kurbo::{BezPath, Circle, Ellipse, Point, RoundedRect, Shape as _};
use runebender::analysis::collisions::{
    CollisionCache, CollisionOptions, PlacedGlyph, find_collisions,
};

const WARMUPS: usize = 3;
const SAMPLES: usize = 15;
const LETTERS: usize = 20;

/// Run `setup` outside the timer and `operation` inside it, and print the minimum, the median
/// and the nearest-rank 90th percentile in milliseconds.
fn measure<S, T>(label: &str, mut setup: impl FnMut(u32) -> S, mut operation: impl FnMut(S) -> T) {
    let mut elapsed = Vec::with_capacity(SAMPLES);
    for sample in 0..WARMUPS + SAMPLES {
        let input = setup(u32::try_from(sample).expect("the sample count is small"));
        let start = Instant::now();
        let output = black_box(operation(input));
        let duration = start.elapsed();
        drop(output);
        if sample >= WARMUPS {
            elapsed.push(duration);
        }
    }
    elapsed.sort_unstable();
    let milliseconds = |duration: Duration| duration.as_secs_f64() * 1_000.0;
    println!(
        "  {label:<26} min={:>9.3} ms median={:>9.3} ms p90={:>9.3} ms",
        milliseconds(elapsed[0]),
        milliseconds(elapsed[SAMPLES / 2]),
        milliseconds(elapsed[(SAMPLES * 9).div_ceil(10) - 1]),
    );
}

fn joined(parts: &[BezPath]) -> BezPath {
    let mut path = BezPath::new();
    for part in parts {
        path.extend(part.elements().iter().copied());
    }
    path
}

/// The outlines of the line: one letter body and one mark for each letter.
fn outlines() -> Vec<(BezPath, BezPath)> {
    (0..LETTERS)
        .map(|index| {
            let wide = 320.0 + f64::from(u32::try_from(index % 3).expect("small")) * 4.0;
            let body = joined(&[
                RoundedRect::new(0.0, 0.0, wide, 120.0, 40.0).to_path(0.01),
                Circle::new((160.0, 260.0), 45.0).to_path(0.01),
                Circle::new((160.0, -110.0), 35.0).to_path(0.01),
            ]);
            let mut mark = Ellipse::new((0.0, 0.0), (80.0, 25.0), 0.3).to_path(0.01);
            // A kurbo ellipse is an open path; a font contour is closed.
            mark.close_path();
            (body, mark)
        })
        .collect()
}

/// The placed glyphs; `lift` raises the mark above the letter at index 10.
fn line(outlines: &[(BezPath, BezPath)], lift: f64) -> Vec<PlacedGlyph<'_, usize>> {
    let mut items = Vec::new();
    for (index, (body, mark)) in outlines.iter().enumerate() {
        let x = f64::from(u32::try_from(LETTERS - 1 - index).expect("small")) * 300.0;
        items.push(PlacedGlyph {
            key: index * 2,
            outline: body,
            origin: Point::new(x, 0.0),
            cluster: index,
            is_mark: false,
        });
        let above = if index == 10 { 430.0 + lift } else { 430.0 };
        items.push(PlacedGlyph {
            key: index * 2 + 1,
            outline: mark,
            origin: Point::new(x + 220.0, above),
            cluster: index,
            is_mark: true,
        });
        // The mark below the second of every four letters sits under the lower dot of the
        // previous letter, which has no mark below.
        let below = match index % 4 {
            0 => continue,
            1 => Point::new(x + 460.0, -150.0),
            _ => Point::new(x + 160.0, -200.0),
        };
        items.push(PlacedGlyph {
            key: index * 2 + 1,
            outline: mark,
            origin: below,
            cluster: index,
            is_mark: true,
        });
    }
    items
}

fn main() {
    let outlines = outlines();
    let options = CollisionOptions::for_units_per_em(1000.0);
    let items = line(&outlines, 0.0);
    let found = find_collisions(&items, &options);
    println!(
        "\nCollisions of one line: {LETTERS} letters, {} glyphs, {} collisions",
        items.len(),
        found.len()
    );
    measure(
        "find_collisions",
        |_| (),
        |()| find_collisions(&items, &options),
    );
    measure(
        "cache, cold",
        |_| CollisionCache::new(),
        |mut cache| cache.find_collisions(&items, &options),
    );
    let mut cache = CollisionCache::new();
    cache.find_collisions(&items, &options);
    measure(
        "cache, no change",
        |_| (),
        |()| cache.find_collisions(&items, &options),
    );
    // Each sample moves the mark by a different amount, so no pair of it is in the cache.
    let lines: Vec<Vec<PlacedGlyph<'_, usize>>> = (0..WARMUPS + SAMPLES)
        .map(|sample| {
            line(
                &outlines,
                f64::from(u32::try_from(sample + 1).expect("small")),
            )
        })
        .collect();
    measure(
        "cache, one mark moved",
        |sample| &lines[usize::try_from(sample).expect("small")],
        |moved| cache.find_collisions(moved, &options),
    );
}
