// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Evaluator, history, sampling, and actual compiled-outline regression proofs.

#![cfg(test)]

use kurbo::{BezPath, ParamCurve, ParamCurveNearest, Point};

use super::tests::{
    assert_glifs_unchanged, files, fixture, open, record, root, saved_files, with_document,
};
use super::*;
use crate::font::compiler::{CompiledFont, HoiBakeOptions};
use crate::font::var_model::Location;

fn location(value: f64) -> Location {
    Location::from([("Weight".into(), value)])
}

fn points(project: &Project, glyph: &str, value: f64) -> Vec<Point> {
    project
        .try_interpolated_layer_at(glyph, &location(value))
        .unwrap()
        .contours()
        .flat_map(|contour| contour.points.iter().map(|point| point.position))
        .collect()
}

fn bent(project: &Project, glyph: &str, lo: usize, hi: usize, pi: usize) -> HoiTrajectory {
    let mut curve = record(project, glyph, lo, hi, pi);
    curve.controls[0][1] += 120.0;
    curve.controls[1][1] += 80.0;
    curve
}

fn assert_point(a: Point, b: Point) {
    assert!(a.distance(b) < 0.0001, "{a:?} != {b:?}");
}

#[test]
fn cubic_and_all_easing_presets_are_exact_and_edits_are_local() {
    let ordinary = fixture();
    for (lo, hi, start, end) in [(0, 1, -1.0, 0.0), (1, 2, 0.0, 1.0)] {
        for easing in [
            HoiEasing::Linear,
            HoiEasing::EaseIn,
            HoiEasing::EaseOut,
            HoiEasing::EaseInOut,
        ] {
            let mut memory = fixture();
            let mut curve = bent(&memory.project, "box", lo, hi, 0);
            curve.easing = easing;
            let [a, b, c, d] = memory.project.hoi_curve_points(&curve).unwrap();
            memory.project.set_hoi_trajectories(&[curve]).unwrap();
            for t in [0.13, 0.37, 0.79] {
                let s: f64 = match easing {
                    HoiEasing::Linear => t,
                    HoiEasing::EaseIn => t * t,
                    HoiEasing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
                    HoiEasing::EaseInOut => 3.0 * t * t - 2.0 * t * t * t,
                };
                let expected = (a.to_vec2() * (1.0 - s).powi(3)
                    + b.to_vec2() * (3.0 * (1.0 - s).powi(2) * s)
                    + c.to_vec2() * (3.0 * (1.0 - s) * s * s)
                    + d.to_vec2() * s.powi(3))
                .to_point();
                let value = start + (end - start) * t;
                let actual = points(&memory.project, "box", value);
                assert_point(actual[0], expected);
                assert_eq!(&actual[1..], &points(&ordinary.project, "box", value)[1..]);
                assert_eq!(
                    points(&memory.project, "dot", value),
                    points(&ordinary.project, "dot", value)
                );
            }
            for value in [-1.0, 0.0, 1.0, if start < 0.0 { 0.5 } else { -0.5 }] {
                assert_eq!(
                    points(&memory.project, "box", value),
                    points(&ordinary.project, "box", value)
                );
            }
        }
    }
}

#[test]
fn connected_cubic_handles_follow_their_own_nodes_once() {
    let mut memory = fixture();
    let before = points(&memory.project, "dot", 0.37);
    let mut second = bent(&memory.project, "dot", 1, 2, 3);
    second.controls[0][0] += 60.0;
    let first = bent(&memory.project, "dot", 1, 2, 0);
    memory
        .project
        .set_hoi_trajectories(&[first, second])
        .unwrap();
    let after = points(&memory.project, "dot", 0.37);
    for (node, handles) in [(0, [11, 1]), (3, [2, 4])] {
        for handle in handles {
            assert_point(after[handle] - (after[node] - before[node]), before[handle]);
        }
    }
    assert_eq!(&before[5..11], &after[5..11]);
    let output = memory
        .project
        .try_interpolated_layer_at("dot", &location(0.37))
        .unwrap();
    assert_eq!(output.width, 1000.0);
}

#[test]
fn coincident_endpoints_can_move_and_return_without_changing_sources() {
    let mut memory = fixture();
    let before = points(&memory.project, "fixed", 0.0);
    let curve = bent(&memory.project, "fixed", 1, 2, 0);
    memory.project.set_hoi_trajectories(&[curve]).unwrap();
    assert_eq!(points(&memory.project, "fixed", 0.0), before);
    assert_eq!(points(&memory.project, "fixed", 1.0), before);
    assert!((points(&memory.project, "fixed", 0.5)[0].y - before[0].y - 75.0).abs() < 1e-6);
}

#[test]
fn history_reopen_and_compile_cache_follow_authoring_revision() {
    let input = files(&root());
    let mut memory = open(&input);
    let initial = memory.project.compiled_preview().unwrap();
    let ordinary = points(&memory.project, "box", 0.37);
    let curve = bent(&memory.project, "box", 1, 2, 0);
    memory
        .project
        .set_hoi_trajectories(std::slice::from_ref(&curve))
        .unwrap();
    let edited = points(&memory.project, "box", 0.37);
    let compiled = memory.project.compiled_preview().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&initial, &compiled));
    assert_ne!(initial.bytes, compiled.bytes);
    assert_eq!(
        memory
            .project
            .replay_hoi_history(HistoryDirection::Undo)
            .unwrap(),
        HistoryReplayOutcome::Applied
    );
    assert_eq!(points(&memory.project, "box", 0.37), ordinary);
    assert!(
        !memory.project.set_hoi_trajectories(&[]).unwrap(),
        "no-op retains redo"
    );
    assert_eq!(
        memory
            .project
            .replay_hoi_history(HistoryDirection::Redo)
            .unwrap(),
        HistoryReplayOutcome::Applied
    );
    assert_eq!(points(&memory.project, "box", 0.37), edited);
    assert_eq!(
        memory.project.compiled_preview().unwrap().bytes,
        compiled.bytes
    );
    let saved = saved_files(&memory, input.clone());
    assert_glifs_unchanged(&input, &saved);
    let reopened = open(&saved);
    assert_eq!(points(&reopened.project, "box", 0.37), edited);
    assert_eq!(reopened.project.compile().unwrap().bytes, compiled.bytes);
}

fn directed_distance(a: &BezPath, b: &BezPath) -> f64 {
    a.segments()
        .flat_map(|segment| (0..=24).map(move |i| segment.eval(f64::from(i) / 24.0)))
        .map(|point| {
            b.segments()
                .map(|segment| segment.nearest(point, 1e-6).distance_sq)
                .fold(f64::INFINITY, f64::min)
                .sqrt()
        })
        .fold(0.0, f64::max)
}

fn compare_compiled(
    project: &Project,
    compiled: &CompiledFont,
    value: f64,
    static_font: bool,
) -> f64 {
    let coordinates = if static_font { vec![] } else { vec![value] };
    let outlines = compiled.outlines(&coordinates).unwrap();
    let advances = compiled.advances(&coordinates).unwrap();
    let mut maximum: f64 = 0.0;
    for (name, path) in outlines {
        let evaluated = project
            .try_interpolated_layer_at(&name, &location(value))
            .unwrap();
        let expected = evaluated.contours_to_bezpath();
        if expected.elements().is_empty() {
            assert!(path.elements().is_empty());
        } else {
            let error =
                directed_distance(&expected, &path).max(directed_distance(&path, &expected));
            assert!(
                error <= 2.0,
                "{name} at {value}, static={static_font}, error={error}"
            );
            maximum = maximum.max(error);
            let contours = |path: &BezPath| {
                path.elements()
                    .iter()
                    .filter(|element| matches!(element, kurbo::PathEl::MoveTo(_)))
                    .count()
            };
            assert_eq!(
                contours(&expected),
                contours(&path),
                "contour count changed for {name}"
            );
        }
        assert!(
            (advances.iter().find(|(glyph, _)| *glyph == name).unwrap().1 - evaluated.width).abs()
                <= 1.0
        );
    }
    maximum
}

#[test]
fn glyph_local_bake_and_static_export_match_saved_curves_at_unsampled_locations() {
    let input = files(&root());
    let mut memory = open(&input);
    let baseline = memory.project.babelfont_snapshot().unwrap();
    let mut curves = vec![
        bent(&memory.project, "dot", 1, 2, 0),
        bent(&memory.project, "ring", 0, 1, 0),
    ];
    for index in 0..4 {
        let mut curve = bent(&memory.project, "box", 1, 2, index);
        curve.easing = HoiEasing::EaseInOut;
        curves.push(curve);
    }
    memory.project.set_hoi_trajectories(&curves).unwrap();
    let saved = saved_files(&memory, input.clone());
    let memory = open(&saved);
    let revision = memory.project.document_revision();
    let (snapshot, report) = memory
        .project
        .babelfont_snapshot_with_hoi(HoiBakeOptions::default())
        .unwrap();
    assert_eq!(
        snapshot.masters, baseline.masters,
        "bake must not create full masters"
    );
    assert!(report.generated_layers > 0);
    assert!(report.source_error_bound <= 0.25);
    for name in ["fixed", "space", ".notdef"] {
        let baked = snapshot
            .glyphs
            .0
            .iter()
            .find(|glyph| glyph.name == name)
            .unwrap();
        let original = memory
            .project
            .variable_font()
            .glyphs
            .0
            .iter()
            .find(|glyph| glyph.name == name)
            .unwrap();
        assert_eq!(baked.layers, original.layers);
    }
    let compiled = CompiledFont::build(snapshot).unwrap();
    assert_eq!(compiled.axis_tags, ["wght"]);
    let mut maximum: f64 = 0.0;
    for value in [-1.0, -0.83, -0.37, 0.0, 0.13, 0.37, 0.79, 1.0] {
        maximum = maximum.max(compare_compiled(&memory.project, &compiled, value, false));
    }
    for value in [-0.37, 0.37] {
        let static_font = memory.project.compile_static(&location(value)).unwrap();
        assert!(static_font.axis_tags.is_empty());
        maximum = maximum.max(compare_compiled(&memory.project, &static_font, value, true));
    }
    write_proof(&memory.project, &compiled, &saved, &report, maximum);
    assert_eq!(memory.project.document_revision(), revision);
    assert_eq!(
        saved_files(&memory, saved.clone()),
        saved,
        "export must not write generated layers into the source package"
    );
    assert_glifs_unchanged(&input, &saved);
}

#[test]
fn sampling_limits_and_invalid_static_locations_fail_explicitly() {
    let mut memory = fixture();
    let curve = bent(&memory.project, "box", 1, 2, 0);
    memory.project.set_hoi_trajectories(&[curve]).unwrap();
    assert!(
        memory
            .project
            .babelfont_snapshot_with_hoi(HoiBakeOptions {
                source_tolerance: 0.001,
                max_generated_layers: 2
            })
            .unwrap_err()
            .contains("limit")
    );
    for tolerance in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            memory
                .project
                .babelfont_snapshot_with_hoi(HoiBakeOptions {
                    source_tolerance: tolerance,
                    ..HoiBakeOptions::default()
                })
                .is_err()
        );
    }
    assert!(
        memory
            .project
            .static_babelfont_snapshot(&location(f64::NAN))
            .is_err()
    );
    assert!(
        memory
            .project
            .static_babelfont_snapshot(&location(1.1))
            .is_err()
    );
    assert!(
        memory
            .project
            .static_babelfont_snapshot(&Location::from([("Wrong".into(), 0.0)]))
            .is_err()
    );
}

#[test]
fn mapped_axis_evaluation_and_compilation_use_design_interval_progress() {
    let mut memory = with_document(|doc| {
        doc.axes[0].map = Some(
            [(100.0, 10.0), (400.0, 40.0), (650.0, 60.0), (900.0, 140.0)]
                .into_iter()
                .map(|(input, output)| norad::designspace::AxisMapping { input, output })
                .collect(),
        );
        for (source, value) in doc.sources.iter_mut().zip([10.0, 40.0, 140.0]) {
            source.location[0].xvalue = Some(value);
        }
    });
    let ordinary = points(&memory.project, "box", 0.2);
    let curve = bent(&memory.project, "box", 1, 2, 0);
    memory.project.set_hoi_trajectories(&[curve]).unwrap();
    let evaluated = points(&memory.project, "box", 0.2);
    assert!(
        (evaluated[0].y
            - ordinary[0].y
            - 3.0 * 0.8 * 0.8 * 0.2 * 120.0
            - 3.0 * 0.8 * 0.2 * 0.2 * 80.0)
            .abs()
            < 1e-5
    );
    let font = memory.project.compile().unwrap();
    compare_compiled(&memory.project, &font, 0.2, false);
    compare_compiled(&memory.project, &font, -0.37, false);
}

#[test]
fn generated_source_interpolation_stays_inside_the_reported_bound() {
    let mut memory = fixture();
    let mut curve = bent(&memory.project, "dot", 1, 2, 0);
    curve.easing = HoiEasing::EaseInOut;
    memory.project.set_hoi_trajectories(&[curve]).unwrap();
    let (font, report) = memory
        .project
        .babelfont_snapshot_with_hoi(HoiBakeOptions::default())
        .unwrap();
    let glyph = font
        .glyphs
        .0
        .iter()
        .find(|glyph| glyph.name == "dot")
        .unwrap();
    let mut samples: Vec<_> = glyph
        .layers
        .iter()
        .filter_map(|layer| {
            let design = layer
                .location
                .as_ref()
                .or_else(|| {
                    if let babelfont::LayerType::DefaultForMaster(id) = &layer.master {
                        font.masters
                            .iter()
                            .find(|master| &master.id == id)
                            .map(|master| &master.location)
                    } else {
                        None
                    }
                })?
                .iter()
                .next()?
                .1
                .to_f64();
            let nodes: Vec<_> = layer
                .shapes
                .iter()
                .filter_map(|shape| shape.as_path())
                .flat_map(|path| path.nodes.iter().map(|node| Point::new(node.x, node.y)))
                .collect();
            Some((design, nodes))
        })
        .collect();
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    for t in [0.017, 0.137, 0.373, 0.791, 0.983] {
        let design = 400.0 + 500.0 * t;
        let pair = samples
            .windows(2)
            .find(|pair| pair[0].0 <= design && design <= pair[1].0)
            .unwrap();
        let s = (design - pair[0].0) / (pair[1].0 - pair[0].0);
        for ((a, b), exact) in
            pair[0]
                .1
                .iter()
                .zip(&pair[1].1)
                .zip(points(&memory.project, "dot", t))
        {
            assert!(a.lerp(*b, s).distance(exact) <= report.source_error_bound + 0.0002);
        }
    }
}

#[test]
fn stale_endpoint_replay_is_atomic_and_does_not_consume_redo() {
    let mut memory = fixture();
    let curve = bent(&memory.project, "box", 1, 2, 0);
    let bound = memory.project.bind_hoi_trajectory(&curve).unwrap();
    memory.project.set_hoi_trajectories(&[curve]).unwrap();
    memory
        .project
        .replay_hoi_history(HistoryDirection::Undo)
        .unwrap();
    let address = crate::font::variable::GlyphLayerAddress {
        glyph: "box".into(),
        layer: bound.lower_layer,
    };
    let original = memory.project.capture_document_layer(&address).unwrap();
    let mut edit = memory
        .project
        .begin_document_layer_transaction(&address)
        .unwrap();
    edit.draft_mut()
        .set_point_position(bound.lower_point, (401.0, 281.0).into())
        .unwrap();
    memory
        .project
        .commit_document_layer_transaction(edit)
        .unwrap();
    let changed = memory.project.capture_document_layer(&address).unwrap();
    let revision = memory.project.document_revision();
    assert!(
        memory
            .project
            .replay_hoi_history(HistoryDirection::Redo)
            .is_err()
    );
    assert_eq!(memory.project.document_revision(), revision);
    assert!(memory.project.hoi_trajectories().unwrap().is_empty());
    memory
        .project
        .restore_document_layer_if_current(&address, &changed, original)
        .unwrap();
    assert_eq!(
        memory
            .project
            .replay_hoi_history(HistoryDirection::Redo)
            .unwrap(),
        HistoryReplayOutcome::Applied
    );
    let mut edit = memory
        .project
        .begin_document_layer_transaction(&address)
        .unwrap();
    edit.draft_mut()
        .set_point_position(bound.lower_point, (401.0, 281.0).into())
        .unwrap();
    memory
        .project
        .commit_document_layer_transaction(edit)
        .unwrap();
    assert!(
        memory
            .project
            .try_interpolated_layer_at("box", &location(0.37))
            .is_err()
    );
    assert!(memory.project.babelfont_snapshot().is_err());
    assert!(
        memory
            .project
            .static_babelfont_snapshot(&location(0.37))
            .is_err()
    );
    assert!(
        memory
            .project
            .replay_hoi_history(HistoryDirection::Undo)
            .is_ok(),
        "removing a suspended curve is allowed"
    );
}

#[test]
fn authored_sparse_boundary_is_retained_by_evaluator_and_compiler() {
    let input = files(&root());
    let mut doc = crate::font::persistence::memory::designspace_from_str(
        std::str::from_utf8(&input["HOIShapes.designspace"]).unwrap(),
    )
    .unwrap();
    let mut sparse = doc.sources[1].clone();
    sparse.layer = Some("middle".into());
    sparse.location[0].xvalue = Some(650.0);
    doc.sources.push(sparse);
    let mut project = Project::from_designspace(doc, |filename| {
        let mut font = norad::Font::load(root().join(filename)).unwrap();
        if filename.contains("Regular") {
            let glyph = font.default_layer().get_glyph("box").unwrap().clone();
            font.layers.new_layer("middle").unwrap().insert_glyph(glyph);
        }
        Ok(crate::font::project::SourceInput::from_font(
            font,
            filename.into(),
        ))
    })
    .unwrap();
    let baseline = points(&project, "box", 0.73);
    let boundary = points(&project, "box", 0.5);
    let ds = project.document_designspace().unwrap();
    let lower = &ds.sources()[1].default_layer;
    let middle = &ds.sparse_sources()[0].layer;
    let point = project
        .document_layer("box", lower)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .next()
        .unwrap()
        .id();
    let mut curve = project
        .prepare_hoi_trajectory("box", ds.axes()[0].id(), lower, middle, point)
        .unwrap();
    curve.controls[0][1] += 120.0;
    curve.controls[1][1] += 80.0;
    project.set_hoi_trajectories(&[curve]).unwrap();
    assert_eq!(points(&project, "box", 0.5), boundary);
    assert_eq!(points(&project, "box", 0.73), baseline);
    let font = project.compile().unwrap();
    for value in [0.137, 0.373, 0.5, 0.73] {
        compare_compiled(&project, &font, value, false);
    }
}

#[test]
fn both_neighboring_intervals_keep_the_shared_master_exact() {
    let mut memory = fixture();
    let middle = points(&memory.project, "box", 0.0);
    let mut curves = [
        bent(&memory.project, "box", 0, 1, 0),
        bent(&memory.project, "box", 1, 2, 0),
    ];
    curves[0].easing = HoiEasing::EaseOut;
    curves[1].easing = HoiEasing::EaseIn;
    memory.project.set_hoi_trajectories(&curves).unwrap();
    assert_eq!(points(&memory.project, "box", 0.0), middle);
    let font = memory.project.compile().unwrap();
    for value in [-0.73, -0.137, 0.0, 0.137, 0.73] {
        compare_compiled(&memory.project, &font, value, false);
    }
}

fn write_proof(
    project: &Project,
    font: &CompiledFont,
    package: &std::collections::BTreeMap<String, Vec<u8>>,
    report: &crate::font::compiler::HoiBakeReport,
    maximum: f64,
) {
    let Some(directory) = std::env::var_os("RUNEBENDER_HOI_PROOF_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("measurements.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "generated_glyph_layers": report.generated_layers,
            "source_error_bound": report.source_error_bound,
            "measured_compiled_outline_error": maximum,
            "compiled_test_tolerance": 2.0,
            "source_tolerance": 0.25,
            "units_per_em": 1000
        }))
        .unwrap(),
    )
    .unwrap();
    for (name, bytes) in package {
        let path = directory.join("source").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    std::fs::write(directory.join("HOIShapes-VF.ttf"), &*font.bytes).unwrap();
    let mut svg = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1420" height="920" viewBox="0 0 1420 920"><rect width="1420" height="920" fill="#f9fafb"/><g font-family="sans-serif" fill="#202630"><text x="40" y="45" font-size="26">HOI Shapes · Phase 2 export proof</text><text x="40" y="78" font-size="16">Gray: ordinary interpolation · Blue: exact evaluator · Orange: compiled variable font</text></g>"##,
    );
    let ordinary = fixture();
    for (column, (value, label)) in [
        (-1.0, "Light 100"),
        (-0.37, "Weight 289"),
        (0.0, "Regular 400"),
        (0.37, "Weight 585"),
        (1.0, "Bold 900"),
    ]
    .into_iter()
    .enumerate()
    {
        let x = 150 + column * 250;
        svg.push_str(&format!(
            r#"<text x="{x}" y="125" font-family="sans-serif" font-size="17">{label}</text>"#
        ));
        let outlines = font.outlines(&[value]).unwrap();
        for (row, glyph) in ["box", "dot", "ring", "fixed"].into_iter().enumerate() {
            let y = 150 + row * 180;
            if column == 0 {
                svg.push_str(&format!(
                    r#"<text x="40" y="{}" font-family="sans-serif" font-size="18">{glyph}</text>"#,
                    y + 80
                ));
            }
            let original = ordinary
                .project
                .try_interpolated_layer_at(glyph, &location(value))
                .unwrap()
                .contours_to_bezpath();
            let exact = project
                .try_interpolated_layer_at(glyph, &location(value))
                .unwrap()
                .contours_to_bezpath();
            let compiled = &outlines.iter().find(|(name, _)| name == glyph).unwrap().1;
            svg.push_str(&format!(r##"<g transform="translate({x},{}) scale(.23,-.23)"><path d="{}" fill="#d8dce2"/><path d="{}" fill="none" stroke="#176bb2" stroke-width="9"/><path d="{}" fill="none" stroke="#e07627" stroke-width="3"/></g>"##, y + 160, original.to_svg(), exact.to_svg(), compiled.to_svg()));
        }
    }
    svg.push_str("</svg>");
    std::fs::write(directory.join("comparison.svg"), svg).unwrap();
    for (name, value) in [("289", -0.37), ("585", 0.37)] {
        std::fs::write(
            directory.join(format!("HOIShapes-Static-{name}.ttf")),
            &*project.compile_static(&location(value)).unwrap().bytes,
        )
        .unwrap();
    }
}

#[test]
fn static_snapshot_interpolates_advances_anchors_metrics_and_kerning() {
    use crate::font::canonical_metadata::KerningParticipant;
    let mut input = files(&root());
    for (source, width, x) in [
        ("Light", 900, 20),
        ("Regular", 1000, 40),
        ("Bold", 1100, 80),
    ] {
        let key = format!("HOIShapes-{source}.ufo/glyphs/dot.glif");
        let glif = String::from_utf8(input[&key].clone())
            .unwrap()
            .replace("width=\"1000\"", &format!("width=\"{width}\""))
            .replace(
                "</glyph>",
                &format!("<anchor name=\"top\" x=\"{x}\" y=\"700\"/></glyph>"),
            );
        input.insert(key, glif.into_bytes());
    }
    let mut memory = open(&input);
    let sources: Vec<_> = memory
        .project
        .document_sources()
        .map(|source| source.id())
        .collect();
    for (source, (ascender, kern)) in
        sources
            .into_iter()
            .zip([(700.0, 0.0), (800.0, -20.0), (1000.0, -100.0)])
    {
        memory
            .project
            .edit_document_source_metadata(source, |draft| {
                let mut info = draft.font_info().clone();
                info.metrics.ascender = Some(ascender);
                draft.set_font_info(info);
                let mut metadata = draft.font_metadata().clone();
                metadata
                    .set_kerning_pair(
                        KerningParticipant::glyph("box").unwrap(),
                        KerningParticipant::glyph("dot").unwrap(),
                        Some(kern),
                    )
                    .unwrap();
                draft.set_font_metadata(metadata);
                Ok(())
            })
            .unwrap();
    }
    let ordinary = memory
        .project
        .try_interpolated_layer_at("dot", &location(0.37))
        .unwrap();
    let curve = bent(&memory.project, "dot", 1, 2, 0);
    memory.project.set_hoi_trajectories(&[curve]).unwrap();
    let evaluated = memory
        .project
        .try_interpolated_layer_at("dot", &location(0.37))
        .unwrap();
    assert_eq!(evaluated.width, ordinary.width);
    assert_eq!(evaluated.anchors, ordinary.anchors);
    assert!((evaluated.width - 1037.0).abs() < 1e-4);
    assert!((evaluated.anchors[0].position.x - 54.8).abs() < 1e-4);
    let snapshot = memory
        .project
        .static_babelfont_snapshot(&location(0.37))
        .unwrap();
    assert_eq!(
        snapshot.masters[0].metrics[&babelfont::MetricType::Ascender],
        874
    );
    assert_eq!(
        snapshot.masters[0].kerning[&("box".into(), "dot".into())],
        -50
    );
    let font = memory.project.compile_static(&location(0.37)).unwrap();
    compare_compiled(&memory.project, &font, 0.37, true);
    let font = memory.project.compile().unwrap();
    compare_compiled(&memory.project, &font, 0.37, false);
}

#[test]
fn overflowing_curvature_never_becomes_a_silent_linear_export() {
    let mut memory = fixture();
    let mut curve = bent(&memory.project, "box", 1, 2, 0);
    curve.controls[0] = [1e308, 1e308];
    curve.controls[1] = [-1e308, -1e308];
    memory.project.set_hoi_trajectories(&[curve]).unwrap();
    assert!(
        memory
            .project
            .babelfont_snapshot()
            .unwrap_err()
            .contains("finite")
    );
}
