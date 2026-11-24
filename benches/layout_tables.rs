// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The cost of the live layout tables of the text line; see `docs/BENCHMARKS.md`.
//!
//! For each font, the harness reads the layout inputs of the first source, builds the tables
//! cold, rebuilds them warm after one anchor change, after one kerning change and with no
//! change, and shapes one line with the result.
//!
//! The repository fixtures `tests/fixtures/arabic` and `tests/fixtures/core` always run.
//! Set `RUNEBENDER_TEST_FONTS` to the Virtua Grotesk `sources` folder and
//! `RUNEBENDER_AMIRI_DESIGNSPACE` to a designspace of Amiri to measure those fonts too.

use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use runebender::font::project::Project;
use runebender::text::layout_tables::{LayoutInputs, LayoutTables};
use runebender::text::shape::ShapingFont;

const WARMUPS: usize = 3;
const SAMPLES: usize = 15;

/// One font to measure and the line to shape with it.
struct Workload {
    label: &'static str,
    path: PathBuf,
    text: &'static str,
    right_to_left: bool,
    script: &'static str,
}

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

fn benchmark(workload: &Workload) {
    let project = match Project::load(&workload.path) {
        Ok(project) => project,
        Err(error) => {
            // Only the optional external fonts can be missing or unreadable; the repository
            // fixtures always load.
            println!(
                "\n{}: not measured, the font does not load: {error}",
                workload.label
            );
            return;
        }
    };
    let source = project
        .document_sources()
        .next()
        .expect("the font has a source")
        .id();
    let mut base = LayoutInputs::from_project(&project, source).expect("the layout inputs read");
    // The tables are built for one location without axes, so a feature file with variable
    // values does not compile. Such a font is measured without its feature file, and the
    // report says so.
    if let Err(error) = LayoutTables::new().build(&base) {
        let first_line = error.lines().next().unwrap_or_default();
        println!(
            "\n{}: the feature file does not compile at one location ({first_line}); \
             measured without it",
            workload.label
        );
        base.features.clear();
    }
    let anchored = base
        .glyphs
        .iter()
        .position(|glyph| !glyph.anchors.is_empty())
        .expect("a glyph with an anchor");
    assert!(!base.kerning.is_empty(), "the font has kerning");
    println!(
        "\n{}: {} glyphs, {} with anchors, {} kerning pairs, {} groups",
        workload.label,
        base.glyphs.len(),
        base.glyphs
            .iter()
            .filter(|glyph| !glyph.anchors.is_empty())
            .count(),
        base.kerning.len(),
        base.groups.len(),
    );

    measure(
        "LayoutInputs::from_project",
        |_| (),
        |()| LayoutInputs::from_project(&project, source).expect("the layout inputs read"),
    );
    measure(
        "cold build",
        |_| LayoutTables::new(),
        |mut tables| tables.build(&base).expect("the tables build"),
    );

    // One builder for the warm rows. Each sample changes the value by a different amount, so
    // every build sees a value that the cache has not seen.
    let mut tables = LayoutTables::new();
    let font = tables.build(&base).expect("the tables build");
    measure(
        "warm, one anchor change",
        |sample| {
            let mut inputs = base.clone();
            inputs.glyphs[anchored].anchors[0].1 += 1.0 + f64::from(sample);
            inputs
        },
        |inputs| tables.build(&inputs).expect("the tables build"),
    );
    measure(
        "warm, one kerning change",
        |sample| {
            let mut inputs = base.clone();
            inputs.kerning[0].2 += 1.0 + f64::from(sample);
            inputs
        },
        |inputs| tables.build(&inputs).expect("the tables build"),
    );
    // Build the unchanged inputs once, so the no-change rows reuse every cached step.
    tables.build(&base).expect("the tables build");
    measure(
        "warm, no change",
        |_| (),
        |()| tables.build(&base).expect("the tables build"),
    );
    measure("shape one line", |_| (), |()| shape(&font, workload));
}

fn shape(font: &ShapingFont, workload: &Workload) -> usize {
    font.shape_with_options(
        workload.text,
        workload.right_to_left,
        &[],
        Some(workload.script),
        None,
    )
    .expect("the line shapes")
    .len()
}

fn main() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut workloads = vec![
        Workload {
            label: "Arabic fixture",
            path: fixtures.join("arabic/Arabic.designspace"),
            text: "بَّسم لَاُ رَا يِ سسس رُه",
            right_to_left: true,
            script: "arab",
        },
        Workload {
            label: "Core fixture",
            path: fixtures.join("core/Core.designspace"),
            text: "AVATAR bred a hog 10. a\u{300}",
            right_to_left: false,
            script: "latn",
        },
    ];
    if let Some(directory) = std::env::var_os("RUNEBENDER_TEST_FONTS") {
        workloads.push(Workload {
            label: "Virtua Grotesk",
            path: PathBuf::from(directory).join("VirtuaGrotesk.designspace"),
            text: "فيرتوا غروتيسك",
            right_to_left: true,
            script: "arab",
        });
    }
    if let Some(path) = std::env::var_os("RUNEBENDER_AMIRI_DESIGNSPACE") {
        workloads.push(Workload {
            label: "Amiri",
            path: PathBuf::from(path),
            text: "بِسْمِ اللَّهِ الرَّحْمَٰنِ الرَّحِيمِ",
            right_to_left: true,
            script: "arab",
        });
    }
    println!("Layout tables: {WARMUPS} warmups + {SAMPLES} measured samples per operation");
    println!("Single-threaded; the inputs are read from the first source of each font.");
    for workload in &workloads {
        benchmark(workload);
    }
}
