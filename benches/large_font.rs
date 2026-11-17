// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! How font-engine operations scale with glyph count; see `docs/BENCHMARKS.md`.
//!
//! Each size is a two-master designspace on disk. Every operation runs once per size, because
//! the point is the growth from one size to the next, not a precise time for one size.
//! An operation whose time per glyph rises with the glyph count does more than linear work.
//!
//! Set `RUNEBENDER_BENCH_GLYPHS` to a comma-separated list of glyph counts to change the sizes.
//! Set `RUNEBENDER_BENCH_KEEP` to a folder to also keep each size's fixture there.

use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use norad::{Anchor, Component, Contour, ContourPoint, Font, Glyph, Name, PointType};
use runebender::font::project::{Project, SourceInput};
use runebender::font::var_model::Location;
use runebender::font::variable::GlyphLayerAddress;

const DEFAULT_SIZES: &[u32] = &[1_000, 4_000, 16_000];

const DESIGNSPACE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<designspace format="5.0">
  <axes><axis name="Weight" tag="wght" minimum="0" default="0" maximum="1"/></axes>
  <sources>
    <source filename="Regular.ufo" familyname="Scaling Benchmark" stylename="Regular">
      <location><dimension name="Weight" xvalue="0"/></location>
    </source>
    <source filename="Heavy.ufo" familyname="Scaling Benchmark" stylename="Heavy">
      <location><dimension name="Weight" xvalue="1"/></location>
    </source>
  </sources>
</designspace>"#;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock follows the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "runebender-large-font-bench-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create isolated benchmark directory");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("could not remove {}: {error}", self.0.display());
        }
    }
}

fn glyph_name(index: u32) -> String {
    format!("glyph{index:05}")
}

/// One master: every glyph has two contours, ten points and two anchors; one in eight has a
/// component. `weight` moves the outlines so the two masters differ but stay compatible.
fn synthetic_font(glyph_count: u32, weight: f64) -> Font {
    let mut font = Font::new();
    font.font_info.family_name = Some("Scaling Benchmark".into());
    font.font_info.style_name = Some(if weight == 0.0 { "Regular" } else { "Heavy" }.into());
    font.font_info.units_per_em = Some(1000_u32.into());
    for index in 0..glyph_count {
        let mut glyph = Glyph::new(&glyph_name(index));
        let offset = f64::from(index % 17) + weight * 40.0;
        glyph.width = 600.0 + offset;
        // Private Use Area code points, so every glyph is encoded.
        if let Some(character) = char::from_u32(0xF_0000 + index) {
            glyph.codepoints.insert(character);
        }
        glyph.contours.push(Contour::new(
            [(0.0, 0.0), (400.0, 0.0), (400.0, 700.0), (0.0, 700.0)]
                .into_iter()
                .map(|(x, y)| ContourPoint::new(x + offset, y, PointType::Line, false, None, None))
                .collect(),
            None,
        ));
        glyph.contours.push(Contour::new(
            [
                (100.0, 100.0, PointType::Curve),
                (100.0, 300.0, PointType::OffCurve),
                (300.0, 300.0, PointType::OffCurve),
                (300.0, 100.0, PointType::Curve),
                (300.0, -100.0, PointType::OffCurve),
                (100.0, -100.0, PointType::OffCurve),
            ]
            .into_iter()
            .map(|(x, y, kind)| ContourPoint::new(x + offset, y, kind, false, None, None))
            .collect(),
            None,
        ));
        for (name, y) in [("top", 750.0), ("bottom", -150.0)] {
            glyph.anchors.push(Anchor::new(
                200.0 + offset,
                y + weight * 10.0,
                Some(Name::new(name).expect("valid anchor name")),
                None,
                None,
            ));
        }
        if index > 0 && index % 8 == 0 {
            glyph.components.push(Component::new(
                Name::new("glyph00000").expect("valid component name"),
                norad::AffineTransform {
                    x_offset: 25.0 + weight * 20.0,
                    ..Default::default()
                },
                None,
            ));
        }
        font.default_layer_mut().insert_glyph(glyph);
    }
    font
}

fn write_fixture(directory: &Path, glyph_count: u32) -> PathBuf {
    fs::create_dir(directory).expect("create workload directory");
    for (filename, weight) in [("Regular.ufo", 0.0), ("Heavy.ufo", 1.0)] {
        let path = directory.join(filename);
        Project::from_source(SourceInput::from_font(
            synthetic_font(glyph_count, weight),
            path,
        ))
        .expect("valid fixture")
        .save()
        .expect("write fixture master");
    }
    let designspace = directory.join("Scaling.designspace");
    fs::write(&designspace, DESIGNSPACE).expect("write fixture designspace");
    designspace
}

fn timed<T>(operation: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let value = black_box(operation());
    (value, start.elapsed())
}

fn report(label: &str, elapsed: Duration, glyph_count: u32) {
    println!(
        "  {label:<34} {:>11.3} ms {:>10.3} us/glyph",
        elapsed.as_secs_f64() * 1_000.0,
        elapsed.as_secs_f64() * 1_000_000.0 / f64::from(glyph_count),
    );
}

fn edit_width(project: &mut Project, glyph: &str, width: f64) {
    let address = GlyphLayerAddress {
        glyph: glyph.to_owned(),
        layer: project
            .document_source(project.source_id(0).expect("default source identity"))
            .expect("default source")
            .default_layer(),
    };
    let mut transaction = project
        .begin_document_layer_transaction(&address)
        .expect("editable fixture layer");
    transaction
        .draft_mut()
        .set_width(width)
        .expect("finite width");
    let outcome = project
        .commit_document_layer_transaction(transaction)
        .expect("commit the width edit");
    black_box(outcome);
}

fn scaling_report(glyph_count: u32, root: &Path) {
    println!("\n{glyph_count} glyphs, 2 masters, 10 points per glyph");
    let designspace = write_fixture(&root.join(format!("size-{glyph_count}")), glyph_count);

    let (mut project, elapsed) =
        timed(|| Project::load(&designspace).expect("load the fixture designspace"));
    report("open designspace", elapsed, glyph_count);

    // The first and last glyph show whether a commit depends on the glyph's position.
    for (label, index) in [
        ("edit first glyph, commit", 0),
        ("edit last glyph, commit", glyph_count - 1),
    ] {
        let name = glyph_name(index);
        let ((), elapsed) = timed(|| edit_width(&mut project, &name, 733.0));
        report(label, elapsed, glyph_count);
    }

    let location: Location = [("Weight".into(), 0.375)].into();
    let last = glyph_name(glyph_count - 1);
    let (outline, elapsed) = timed(|| project.interpolated_outline_at(&last, &location));
    outline.expect("compatible synthetic masters");
    report("interpolate last glyph", elapsed, glyph_count);

    let (snapshot, elapsed) = timed(|| project.babelfont_snapshot());
    snapshot.expect("snapshot the fixture for the compiler");
    report("snapshot for preview compile", elapsed, glyph_count);

    let (saved, elapsed) = timed(|| project.save());
    saved.expect("save the fixture after one edit");
    report("save after two edits", elapsed, glyph_count);
}

fn main() {
    let sizes: Vec<u32> = match std::env::var("RUNEBENDER_BENCH_GLYPHS") {
        Ok(list) => list
            .split(',')
            .map(|size| {
                size.trim()
                    .parse()
                    .expect("RUNEBENDER_BENCH_GLYPHS is a comma-separated list of glyph counts")
            })
            .collect(),
        Err(_) => DEFAULT_SIZES.to_vec(),
    };
    // RUNEBENDER_BENCH_KEEP names a folder that receives each size's fixture before it is
    // edited, for probes that need a large font on disk. Nothing else is written there.
    let keep = std::env::var_os("RUNEBENDER_BENCH_KEEP").map(PathBuf::from);
    if let Some(keep) = &keep {
        fs::create_dir_all(keep).expect("create the folder for kept fixtures");
        for &glyph_count in &sizes {
            let directory = keep.join(format!("size-{glyph_count}"));
            if !directory.exists() {
                write_fixture(&directory, glyph_count);
            }
            println!("kept {}", directory.join("Scaling.designspace").display());
        }
    }
    println!("Font engine scaling: one run per operation and size, single-threaded.");
    println!("Time per glyph that rises with the glyph count marks more than linear work.");
    let scratch = Scratch::new();
    for glyph_count in sizes {
        assert!(glyph_count > 0, "a font needs at least one glyph");
        scaling_report(glyph_count, &scratch.0);
    }
}
