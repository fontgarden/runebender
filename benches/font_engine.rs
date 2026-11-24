// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Fixed synthetic font-engine workloads; see `docs/BENCHMARKS.md` for timing boundaries.

use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kurbo::Affine;
use norad::{Anchor, Component, Contour, ContourPoint, Font, Glyph, Name, PointType};
use runebender::font::history::HistoryDirection;
use runebender::font::persistence::memory::designspace_from_str;
use runebender::font::project::{DocumentEditOutcome, Project, SourceInput};
use runebender::font::var_model::Location;
use runebender::font::variable::GlyphLayerAddress;

const WARMUPS: usize = 3;
const SAMPLES: usize = 15;

#[derive(Clone, Copy)]
enum BackgroundWorkload {
    Copy,
    CopyNoop,
    Swap,
    Clear,
    ClearNoop,
    Undo,
    Redo,
}
const DESIGNSPACE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<designspace format="5.0">
  <axes><axis name="Weight" tag="wght" minimum="0" default="0" maximum="1"/></axes>
  <sources>
    <source filename="Regular.ufo" stylename="Regular">
      <location><dimension name="Weight" xvalue="0"/></location>
    </source>
    <source filename="Heavy.ufo" stylename="Heavy">
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
            "runebender-font-engine-bench-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create isolated benchmark directory");
        Self(path)
    }

    fn cleanup(self) {
        fs::remove_dir_all(&self.0).expect("remove benchmark files");
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

fn synthetic_font(glyph_count: u32, weight: f64) -> Font {
    let mut font = Font::new();
    font.font_info.family_name = Some("Engine Benchmark".into());
    font.font_info.style_name = Some(if weight == 0.0 { "Regular" } else { "Heavy" }.into());
    for index in 0..glyph_count {
        let mut glyph = Glyph::new(&format!("glyph{index:05}"));
        let offset = f64::from(index % 17) + weight * 40.0;
        glyph.width = 600.0 + offset;
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

fn project(font: &Font, path: &Path) -> Project {
    Project::from_source(SourceInput::from_font(font.clone(), path.to_path_buf()))
        .expect("valid fixture")
}

fn measure<S, T>(
    label: &str,
    units: usize,
    unit: &str,
    mut setup: impl FnMut() -> S,
    mut operation: impl FnMut(&mut S) -> T,
) {
    let mut elapsed = Vec::with_capacity(SAMPLES);
    for sample in 0..WARMUPS + SAMPLES {
        let mut state = setup();
        let start = Instant::now();
        let result = black_box(operation(black_box(&mut state)));
        let duration = start.elapsed();
        // Returned values and setup-owned state are dropped after the timer stops.
        drop(result);
        if sample >= WARMUPS {
            elapsed.push(duration);
        }
    }
    elapsed.sort_unstable();
    let milliseconds = |duration: Duration| duration.as_secs_f64() * 1_000.0;
    let median = elapsed[SAMPLES / 2];
    let p90 = elapsed[(SAMPLES * 9).div_ceil(10) - 1];
    println!(
        "{label:<28} min={:>10.3} ms/op median={:>10.3} ms/op p90={:>10.3} ms/op {:>10.3} us/{unit}",
        milliseconds(elapsed[0]),
        milliseconds(median),
        milliseconds(p90),
        median.as_secs_f64() * 1_000_000.0 / units as f64,
    );
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
    for entry in fs::read_dir(directory).expect("list synthetic UFO") {
        let entry = entry.expect("read synthetic UFO entry");
        let path = entry.path();
        if entry
            .file_type()
            .expect("read synthetic file type")
            .is_dir()
        {
            collect_files(root, &path, files);
        } else {
            files.push((
                path.strip_prefix(root)
                    .expect("file is inside UFO")
                    .to_path_buf(),
                fs::read(&path).expect("capture synthetic UFO bytes"),
            ));
        }
    }
}

fn benchmark_size(label: &str, glyph_count: u32, root: &Path) {
    let directory = root.join(label);
    fs::create_dir(&directory).expect("create workload directory");
    let path = directory.join("Input.ufo");
    let font = synthetic_font(glyph_count, 0.0);
    project(&font, &path)
        .save()
        .expect("write initial UFO fixture");
    let units = usize::try_from(glyph_count).expect("glyph count fits usize");
    println!("\n{label}: {glyph_count} glyphs/source, 2 contours + 10 points + 2 anchors/glyph");

    let mut files = Vec::new();
    collect_files(&path, &path, &mut files);
    files.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    let bytes: usize = files.iter().map(|(_, contents)| contents.len()).sum();
    println!(
        "UFO disk baseline: {} files, {bytes} serialized bytes",
        files.len()
    );
    let baseline = directory.join("RawCopy.ufo");
    for (relative, contents) in &files {
        let destination = baseline.join(relative);
        fs::create_dir_all(destination.parent().expect("file has parent"))
            .expect("create raw baseline directories");
        fs::write(destination, contents).expect("populate raw baseline");
    }
    let read_paths: Vec<_> = files
        .iter()
        .map(|(relative, _)| path.join(relative))
        .collect();
    let write_paths: Vec<_> = files
        .iter()
        .map(|(relative, _)| baseline.join(relative))
        .collect();
    measure(
        "disk_read_files",
        files.len(),
        "file",
        || Vec::with_capacity(files.len()),
        |buffers| {
            for path in &read_paths {
                buffers.push(fs::read(black_box(path)).expect("read baseline file"));
            }
        },
    );
    measure(
        "disk_overwrite_files",
        files.len(),
        "file",
        || (),
        |()| {
            for (path, (_, contents)) in write_paths.iter().zip(&files) {
                fs::write(black_box(path), black_box(contents)).expect("write baseline file");
            }
        },
    );
    measure(
        "canonical_import",
        units,
        "glyph",
        || Some(SourceInput::from_font(font.clone(), path.clone())),
        |input| Project::from_source(input.take().expect("fresh input")).expect("valid fixture"),
    );
    measure(
        "project_load_ufo",
        units,
        "glyph",
        || (),
        |()| Project::load(black_box(&path)).expect("load synthetic UFO"),
    );
    let save_path = directory.join("Saved.ufo");
    project(&font, &save_path)
        .save()
        .expect("populate save destination");
    measure(
        "project_save_ufo",
        units,
        "glyph",
        || project(&font, &save_path),
        |project| project.save().expect("save synthetic UFO"),
    );

    let variable = Project::from_designspace(
        designspace_from_str(DESIGNSPACE).expect("valid benchmark designspace"),
        |filename| {
            let weight = if filename == "Regular.ufo" { 0.0 } else { 1.0 };
            Ok(SourceInput::from_font(
                synthetic_font(glyph_count, weight),
                directory.join(filename),
            ))
        },
    )
    .expect("valid variable fixture");
    let names: Vec<_> = (0..glyph_count)
        .map(|index| format!("glyph{index:05}"))
        .collect();
    let location: Location = [("Weight".into(), 0.375)].into();
    measure(
        "interpolate_all_outlines",
        units,
        "glyph",
        || Vec::with_capacity(units),
        |outlines| {
            for name in &names {
                outlines.push(
                    variable
                        .interpolated_outline_at(black_box(name), black_box(&location))
                        .expect("compatible synthetic masters"),
                );
            }
        },
    );

    for (name, geometry) in [
        ("edit_width_commit", false),
        ("edit_translate_commit", true),
    ] {
        measure(
            name,
            1,
            "edit",
            || {
                let project = project(&font, &path);
                let address = GlyphLayerAddress {
                    glyph: format!("glyph{:05}", glyph_count / 2),
                    layer: project
                        .document_source(project.source_id(0).expect("default source identity"))
                        .expect("default source")
                        .default_layer(),
                };
                (project, address)
            },
            |(project, address)| {
                let mut transaction = project
                    .begin_document_layer_transaction(black_box(address))
                    .expect("editable fixture layer");
                if geometry {
                    transaction
                        .draft_mut()
                        .transform_points(&[], black_box(Affine::translate((1.25, -0.75))))
                        .expect("finite point translation");
                } else {
                    transaction
                        .draft_mut()
                        .set_width(black_box(725.5))
                        .expect("finite width");
                }
                let outcome = project
                    .commit_document_layer_transaction(transaction)
                    .expect("fresh canonical transaction");
                assert!(
                    matches!(outcome, DocumentEditOutcome::Changed { .. }),
                    "benchmark must commit a real change"
                );
                outcome
            },
        );
    }

    use BackgroundWorkload as B;
    for (name, workload) in [
        ("background_copy", B::Copy),
        ("background_copy_noop", B::CopyNoop),
        ("background_swap", B::Swap),
        ("background_clear", B::Clear),
        ("background_clear_noop", B::ClearNoop),
        ("background_undo", B::Undo),
        ("background_redo", B::Redo),
    ] {
        measure(
            name,
            1,
            "command",
            || {
                let mut project = project(&font, &path);
                let address = GlyphLayerAddress {
                    glyph: format!("glyph{:05}", glyph_count / 2),
                    layer: project
                        .document_source(project.source_id(0).expect("default source identity"))
                        .expect("default source")
                        .default_layer(),
                };
                let group = if matches!(workload, B::Copy | B::ClearNoop) {
                    None
                } else {
                    Some(
                        project
                            .copy_document_layer_to_background(&address)
                            .expect("valid background copy")
                            .expect("first copy changes the layer")
                            .0,
                    )
                };
                if matches!(workload, B::Swap) {
                    project
                        .edit_document_layer(&address.glyph, &address.layer, |draft| {
                            draft.transform_points(&[], Affine::translate((10.0, -10.0)))?;
                            Ok(())
                        })
                        .expect("prepare different foreground geometry");
                }
                if matches!(workload, B::Redo) {
                    project
                        .replay_document_edit_history_group(
                            group.expect("copy history"),
                            HistoryDirection::Undo,
                        )
                        .expect("prepare redo");
                }
                (project, address, group)
            },
            |(project, address, group)| {
                let changed = match workload {
                    B::Copy | B::CopyNoop => project
                        .copy_document_layer_to_background(black_box(address))
                        .expect("valid background copy"),
                    B::Swap => project
                        .swap_document_layer_with_background(black_box(address))
                        .expect("valid background swap"),
                    B::Clear | B::ClearNoop => project
                        .clear_document_background(black_box(&address.glyph), address.layer.source)
                        .expect("valid background clear"),
                    B::Undo | B::Redo => {
                        let group = group.expect("retained background history");
                        let replay = project
                            .replay_document_edit_history_group(
                                group,
                                if matches!(workload, B::Redo) {
                                    HistoryDirection::Redo
                                } else {
                                    HistoryDirection::Undo
                                },
                            )
                            .expect("current background history");
                        Some((group, replay.change))
                    }
                };
                assert_eq!(
                    changed.is_some(),
                    !matches!(workload, B::CopyNoop | B::ClearNoop),
                    "background workload must exercise its named changed or unchanged path"
                );
                changed.map(|(_, change)| change)
            },
        );
    }
}

fn main() {
    println!("Font engine: {WARMUPS} warmups + {SAMPLES} measured samples per operation");
    println!("Single-threaded; disk measurements use warm OS caches, without fsync.");
    let scratch = Scratch::new();
    for (label, glyph_count) in [("small", 32), ("large", 1_024)] {
        benchmark_size(label, glyph_count, &scratch.0);
    }
    scratch.cleanup();
}
