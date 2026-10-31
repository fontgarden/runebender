// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Scoped HOI contracts exercised against disposable copies of HOI Shapes.

#![cfg(test)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::font::model::hoi::HoiIntermediates;
use crate::font::model::hoi_trajectories::HOI_TRAJECTORIES_KEY;
use crate::font::persistence::memory::{MemoryProject, memory_save_files, open_memory_project};
use crate::font::variable::GlyphLayerAddress;

pub(super) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/font-sources/variable-fonts/HOIShapes")
}

pub(super) fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, path: &Path, result: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, result);
            } else if path
                .extension()
                .is_none_or(|ext| ext != "png" && ext != "md")
            {
                result.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}

pub(super) fn open(files: &BTreeMap<String, Vec<u8>>) -> MemoryProject {
    open_memory_project(
        None,
        &files
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

pub(super) fn fixture() -> MemoryProject {
    open(&files(&root()))
}

pub(super) fn record(
    project: &Project,
    glyph: &str,
    lo: usize,
    hi: usize,
    point: usize,
) -> HoiTrajectory {
    let ds = project.document_designspace().unwrap();
    let lower = &ds.sources()[lo].default_layer;
    let upper = &ds.sources()[hi].default_layer;
    let id = project
        .document_layer(glyph, lower)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .nth(point)
        .unwrap()
        .id();
    project
        .prepare_hoi_trajectory(glyph, ds.axes()[0].id(), lower, upper, id)
        .unwrap()
}

pub(super) fn saved_files(
    memory: &MemoryProject,
    mut input: BTreeMap<String, Vec<u8>>,
) -> BTreeMap<String, Vec<u8>> {
    input.extend(
        memory_save_files(&memory.project, &memory.bookkeeping)
            .unwrap()
            .files,
    );
    input
}

pub(super) fn assert_glifs_unchanged(
    before: &BTreeMap<String, Vec<u8>>,
    after: &BTreeMap<String, Vec<u8>>,
) {
    for (path, bytes) in before.iter().filter(|(path, _)| path.ends_with(".glif")) {
        assert_eq!(after.get(path), Some(bytes), "rewrote {path}");
    }
}

#[test]
fn scoped_records_roundtrip_rebind_and_remove_without_rewriting_drawings() {
    let before = files(&root());
    let mut memory = open(&before);
    let mut curve = record(&memory.project, "dot", 1, 2, 0);
    curve.controls[0][1] += 80.0;
    curve.controls[1][1] -= 50.0;
    curve.easing = HoiEasing::EaseInOut;
    let original_binding = memory.project.bind_hoi_trajectory(&curve).unwrap();
    let revision = memory.project.document_revision();
    assert!(
        memory
            .project
            .set_hoi_trajectories(std::slice::from_ref(&curve))
            .unwrap()
    );
    assert_eq!(memory.project.document_revision(), revision + 1);
    assert!(
        !memory
            .project
            .set_hoi_trajectories(std::slice::from_ref(&curve))
            .unwrap()
    );
    assert_eq!(memory.project.document_revision(), revision + 1);
    for source in memory.project.document_sources() {
        assert_eq!(
            memory.project.document_source_is_modified(source.id()),
            Some(false)
        );
    }
    assert!(memory.project.babelfont_snapshot().is_ok());
    assert!(
        memory
            .project
            .try_interpolated_layer_at("dot", &crate::font::var_model::Location::new())
            .is_ok()
    );
    let saved = saved_files(&memory, before.clone());
    assert_glifs_unchanged(&before, &saved);
    let mut reopened = open(&saved);
    assert_eq!(
        reopened.project.hoi_trajectories().unwrap(),
        vec![curve.clone()]
    );
    let rebound = reopened.project.bind_hoi_trajectory(&curve).unwrap();
    assert_ne!(
        original_binding.lower_point, rebound.lower_point,
        "session IDs must not be persisted"
    );
    assert!(reopened.project.set_hoi_trajectories(&[]).unwrap());
    let removed = saved_files(&reopened, saved);
    assert_glifs_unchanged(&before, &removed);
    let reopened = open(&removed);
    assert!(reopened.project.hoi_trajectories().unwrap().is_empty());
    assert!(
        !reopened
            .project
            .document_designspace()
            .unwrap()
            .to_norad()
            .unwrap()
            .lib
            .contains_key(HOI_TRAJECTORIES_KEY)
    );
    assert!(reopened.project.babelfont_snapshot().is_ok());
}

#[test]
fn no_hoi_save_preserves_original_glifs_and_ordinary_interpolation() {
    let before = files(&root());
    let memory = open(&before);
    let location = crate::font::var_model::Location::from([("Weight".into(), 0.5)]);
    let outline = memory
        .project
        .try_interpolated_layer_at("box", &location)
        .unwrap()
        .contours_to_bezpath();
    let saved = saved_files(&memory, before.clone());
    assert_glifs_unchanged(&before, &saved);
    assert_eq!(
        open(&saved)
            .project
            .try_interpolated_layer_at("box", &location)
            .unwrap()
            .contours_to_bezpath(),
        outline
    );
}

#[test]
fn changed_glyphs_are_serialized_and_unchanged_neighbors_keep_their_bytes() {
    let before = files(&root());
    let mut memory = open(&before);
    let curve = record(&memory.project, "box", 1, 2, 0);
    let binding = memory.project.bind_hoi_trajectory(&curve).unwrap();
    let address = GlyphLayerAddress {
        glyph: "box".into(),
        layer: binding.lower_layer,
    };
    let mut edit = memory
        .project
        .begin_document_layer_transaction(&address)
        .unwrap();
    edit.draft_mut()
        .set_point_position(binding.lower_point, kurbo::Point::new(391.0, 281.0))
        .unwrap();
    memory
        .project
        .commit_document_layer_transaction(edit)
        .unwrap();
    let saved = saved_files(&memory, before.clone());
    let changed: Vec<_> = before
        .iter()
        .filter(|(path, bytes)| path.ends_with(".glif") && saved.get(*path) != Some(*bytes))
        .map(|(path, _)| path.as_str())
        .collect();
    assert_eq!(changed, vec!["HOIShapes-Regular.ufo/glyphs/box.glif"]);
    let reopened = open(&saved);
    let layer = &reopened.project.document_designspace().unwrap().sources()[1].default_layer;
    assert_eq!(
        reopened
            .project
            .document_layer("box", layer)
            .unwrap()
            .contours()
            .next()
            .unwrap()
            .points()
            .next()
            .unwrap()
            .position(),
        kurbo::Point::new(391.0, 281.0)
    );
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "runebender-hoi-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        for (relative, bytes) in files(&root()) {
            let file = path.join(relative);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, bytes).unwrap();
        }
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn filesystem_save_preserves_endpoint_bytes_and_reopens_scoped_metadata() {
    let scratch = Scratch::new();
    let before = files(&scratch.0);
    let path = scratch.0.join("HOIShapes.designspace");
    let mut project = Project::load(&path).unwrap();
    let curve = record(&project, "ring", 0, 1, 0);
    project
        .set_hoi_trajectories(std::slice::from_ref(&curve))
        .unwrap();
    project.save().unwrap();
    assert_glifs_unchanged(&before, &files(&scratch.0));
    let mut reopened = Project::load(&path).unwrap();
    reopened.bind_hoi_trajectory(&curve).unwrap();
    reopened.set_hoi_trajectories(&[]).unwrap();
    reopened.save().unwrap();
    assert_glifs_unchanged(&before, &files(&scratch.0));
}

#[test]
fn missing_identifiers_use_a_guard_without_adding_source_identifiers() {
    let mut input = files(&root());
    for (path, bytes) in input.iter_mut().filter(|(path, _)| path.ends_with(".glif")) {
        let mut text = String::from_utf8(bytes.clone()).unwrap();
        while let Some(start) = text.find(" identifier=\"") {
            let end = start + " identifier=\"".len();
            let end = end + text[end..].find('"').unwrap() + 1;
            text.replace_range(start..end, "");
        }
        assert!(!text.contains("identifier="), "{path}");
        *bytes = text.into_bytes();
    }
    let mut memory = open(&input);
    let curve = record(&memory.project, "box", 0, 1, 0);
    assert!(curve.lower.point_identifier.is_none());
    memory
        .project
        .set_hoi_trajectories(std::slice::from_ref(&curve))
        .unwrap();
    let saved = saved_files(&memory, input.clone());
    assert_glifs_unchanged(&input, &saved);
    open(&saved).project.bind_hoi_trajectory(&curve).unwrap();
}

#[test]
fn source_reordering_rebinds_but_outline_and_topology_changes_suspend_records() {
    let mut memory = fixture();
    let curve = record(&memory.project, "dot", 0, 1, 0);
    memory
        .project
        .set_hoi_trajectories(std::slice::from_ref(&curve))
        .unwrap();
    let binding = memory.project.bind_hoi_trajectory(&curve).unwrap();
    memory
        .project
        .move_source(binding.upper_layer.source, 0)
        .unwrap();
    memory.project.bind_hoi_trajectory(&curve).unwrap();
    let saved = saved_files(&memory, files(&root()));
    open(&saved).project.bind_hoi_trajectory(&curve).unwrap();
    let address = GlyphLayerAddress {
        glyph: curve.glyph.clone(),
        layer: binding.lower_layer.clone(),
    };
    let mut edit = memory
        .project
        .begin_document_layer_transaction(&address)
        .unwrap();
    edit.draft_mut()
        .reverse_contours(&[binding.lower_point])
        .unwrap();
    memory
        .project
        .commit_document_layer_transaction(edit)
        .unwrap();
    assert!(memory.project.bind_hoi_trajectory(&curve).is_err());
    assert_eq!(
        memory.project.hoi_trajectories().unwrap(),
        vec![curve],
        "suspended records must be retained"
    );
    memory.project.set_hoi_trajectories(&[]).unwrap();
}

#[test]
fn non_neighboring_spans_off_curves_duplicate_records_and_invalid_controls_are_rejected() {
    let mut memory = fixture();
    let project = &mut memory.project;
    let ds = project.document_designspace().unwrap();
    let axis = ds.axes()[0].id();
    let layers: Vec<_> = ds
        .sources()
        .iter()
        .map(|s| s.default_layer.clone())
        .collect();
    let points: Vec<_> = project
        .document_layer("dot", &layers[0])
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .map(|p| p.id())
        .collect();
    assert!(
        project
            .prepare_hoi_trajectory("dot", axis, &layers[0], &layers[2], points[0])
            .is_err()
    );
    assert!(
        project
            .prepare_hoi_trajectory("dot", axis, &layers[0], &layers[1], points[1])
            .unwrap_err()
            .contains("on-curve")
    );
    let mut curve = record(project, "fixed", 0, 1, 0);
    assert_eq!(
        curve.controls[0], curve.controls[1],
        "coincident positions are valid"
    );
    let revision = project.document_revision();
    assert!(
        project
            .set_hoi_trajectories(&[curve.clone(), curve.clone()])
            .is_err()
    );
    curve.controls[0][0] = f64::NAN;
    assert!(project.set_hoi_trajectories(&[curve]).is_err());
    assert_eq!(project.document_revision(), revision);
    assert!(!project.is_modified());
}

pub(super) fn with_document(
    edit: impl FnOnce(&mut norad::designspace::DesignSpaceDocument),
) -> MemoryProject {
    let mut input = files(&root());
    let key = "HOIShapes.designspace";
    let mut doc = crate::font::persistence::memory::designspace_from_str(
        std::str::from_utf8(&input[key]).unwrap(),
    )
    .unwrap();
    edit(&mut doc);
    input.insert(
        key.into(),
        crate::formats::designspace::to_xml(&doc)
            .unwrap()
            .into_bytes(),
    );
    open(&input)
}

#[test]
fn unknown_and_malformed_metadata_remain_opaque_and_cannot_be_overwritten() {
    for payload in [
        plist::Value::String("future or damaged data".into()),
        plist::Value::Dictionary(plist::Dictionary::from_iter([(
            String::from("version"),
            plist::Value::Integer(99.into()),
        )])),
        plist::Value::Dictionary(plist::Dictionary::from_iter([
            (String::from("version"), plist::Value::Integer(1.into())),
            (
                String::from("trajectories"),
                plist::Value::Array(Vec::new()),
            ),
            (String::from("futureField"), plist::Value::Boolean(true)),
        ])),
    ] {
        let mut memory = with_document(|doc| {
            doc.lib.insert(HOI_TRAJECTORIES_KEY.into(), payload.clone());
        });
        let before = memory
            .project
            .document_designspace()
            .unwrap()
            .to_norad()
            .unwrap()
            .lib;
        assert!(memory.project.hoi_trajectories().is_err());
        assert!(memory.project.set_hoi_trajectories(&[]).is_err());
        assert!(memory.project.babelfont_snapshot().is_err());
        assert_eq!(
            memory
                .project
                .document_designspace()
                .unwrap()
                .to_norad()
                .unwrap()
                .lib,
            before
        );
        memory.project.ds_dirty = true;
        let saved = saved_files(&memory, files(&root()));
        assert_eq!(
            open(&saved)
                .project
                .document_designspace()
                .unwrap()
                .to_norad()
                .unwrap()
                .lib[HOI_TRAJECTORIES_KEY],
            payload
        );
    }
}

#[test]
fn mapped_axis_uses_design_coordinates_and_axis_changes_invalidate_records() {
    let mut memory = with_document(|doc| {
        doc.axes[0].map = Some(vec![
            norad::designspace::AxisMapping {
                input: 100.0,
                output: 10.0,
            },
            norad::designspace::AxisMapping {
                input: 400.0,
                output: 40.0,
            },
            norad::designspace::AxisMapping {
                input: 900.0,
                output: 140.0,
            },
        ]);
        for (source, value) in doc.sources.iter_mut().zip([10.0, 40.0, 140.0]) {
            source.location[0].xvalue = Some(value);
        }
    });
    let curve = record(&memory.project, "box", 1, 2, 0);
    assert_eq!((curve.lower.location, curve.upper.location), (40.0, 140.0));
    memory
        .project
        .set_hoi_trajectories(std::slice::from_ref(&curve))
        .unwrap();
    let mut ds = memory.project.document_designspace().unwrap().clone();
    let axis = ds.axes()[0].id();
    ds.edit_checked(|ds| {
        ds.axis_mut(axis).unwrap().coordinates.name = "Changed".into();
        Ok(())
    })
    .unwrap();
    memory.project.variable.install_designspace(ds);
    assert!(
        memory
            .project
            .bind_hoi_trajectory(&curve)
            .unwrap_err()
            .contains("axis")
    );
}

#[test]
fn sparse_sources_split_only_the_glyphs_they_supply() {
    let input = files(&root());
    let mut doc = crate::font::persistence::memory::designspace_from_str(
        std::str::from_utf8(&input["HOIShapes.designspace"]).unwrap(),
    )
    .unwrap();
    let mut sparse = doc.sources[1].clone();
    sparse.layer = Some("middle".into());
    sparse.location[0].xvalue = Some(650.0);
    doc.sources.push(sparse);
    let project = Project::from_designspace(doc, |filename| {
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
    let ds = project.document_designspace().unwrap();
    let lower = &ds.sources()[1].default_layer;
    let upper = &ds.sources()[2].default_layer;
    let middle = &ds.sparse_sources()[0].layer;
    let node = project
        .document_layer("box", lower)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .next()
        .unwrap()
        .id();
    assert!(
        project
            .prepare_hoi_trajectory("box", ds.axes()[0].id(), lower, upper, node)
            .unwrap_err()
            .contains("neighboring")
    );
    let curve = project
        .prepare_hoi_trajectory("box", ds.axes()[0].id(), lower, middle, node)
        .unwrap();
    assert_eq!(curve.upper.location, 650.0);
    project.bind_hoi_trajectory(&curve).unwrap();
    project
        .bind_hoi_trajectory(&record(&project, "dot", 1, 2, 0))
        .unwrap();
}

#[test]
fn legacy_midpoint_data_keeps_its_payload_and_existing_evaluation() {
    let mut memory = fixture();
    let lower = memory.project.document_designspace().unwrap().sources()[0]
        .default_layer
        .clone();
    let address = GlyphLayerAddress {
        glyph: "box".into(),
        layer: lower.clone(),
    };
    let legacy = HoiIntermediates::from_points([((0, 0), (420.0, 510.0))]).unwrap();
    let mut edit = memory
        .project
        .begin_document_layer_transaction(&address)
        .unwrap();
    edit.draft_mut().set_hoi_intermediates(legacy.clone());
    memory
        .project
        .commit_document_layer_transaction(edit)
        .unwrap();
    let location = crate::font::var_model::Location::from([("Weight".into(), 0.2)]);
    let mut outline = memory
        .project
        .try_interpolated_layer_at("box", &location)
        .unwrap();
    assert_eq!(
        outline.point_at_mut(0, 0).unwrap().position,
        kurbo::Point::new(420.0, 510.0)
    );
    let ds = memory.project.document_designspace().unwrap();
    let upper = &ds.sources()[1].default_layer;
    let point = memory
        .project
        .document_layer("box", &lower)
        .unwrap()
        .contours()
        .next()
        .unwrap()
        .points()
        .next()
        .unwrap()
        .id();
    assert!(
        memory
            .project
            .prepare_hoi_trajectory("box", ds.axes()[0].id(), &lower, upper, point)
            .unwrap_err()
            .contains("legacy")
    );
    let saved = saved_files(&memory, files(&root()));
    let reopened = open(&saved);
    assert_eq!(
        reopened
            .project
            .document_layer("box", &lower)
            .unwrap()
            .hoi_intermediates(),
        Some(&legacy)
    );
}

#[test]
fn source_location_edits_and_removal_suspend_records_without_discarding_them() {
    let mut memory = fixture();
    let curve = record(&memory.project, "box", 0, 1, 0);
    memory
        .project
        .set_hoi_trajectories(std::slice::from_ref(&curve))
        .unwrap();
    let bound = memory.project.bind_hoi_trajectory(&curve).unwrap();
    let moved = crate::font::var_model::Location::from([("Weight".into(), -0.75)]);
    memory
        .project
        .update_source(bound.lower_layer.source, "Light", &moved)
        .unwrap();
    assert!(memory.project.bind_hoi_trajectory(&curve).is_err());
    assert_eq!(
        memory.project.hoi_trajectories().unwrap(),
        vec![curve.clone()]
    );
    memory.project.undo_sources(false).unwrap();
    memory.project.bind_hoi_trajectory(&curve).unwrap();
    memory
        .project
        .remove_source(bound.lower_layer.source)
        .unwrap();
    assert!(
        memory
            .project
            .bind_hoi_trajectory(&curve)
            .unwrap_err()
            .contains("source")
    );
    assert_eq!(
        memory.project.hoi_trajectories().unwrap(),
        vec![curve.clone()]
    );
    memory.project.undo_sources(false).unwrap();
    memory.project.bind_hoi_trajectory(&curve).unwrap();
}

#[test]
fn second_axis_is_reported_as_unsupported_instead_of_choosing_the_first() {
    let curve = record(&fixture().project, "box", 0, 1, 0);
    let memory = with_document(|doc| {
        let mut width = doc.axes[0].clone();
        width.name = "Width".into();
        width.tag = "wdth".into();
        width.minimum = Some(75.0);
        width.default = 100.0;
        width.maximum = Some(125.0);
        doc.axes.push(width);
    });
    assert!(
        memory
            .project
            .bind_hoi_trajectory(&curve)
            .unwrap_err()
            .contains("single continuous axis")
    );
}

#[test]
fn valid_metadata_edits_keep_unrelated_designspace_data() {
    let mut memory = with_document(|doc| {
        doc.lib.insert(
            "com.example.opaque".into(),
            plist::Value::Data(vec![0, 1, 255]),
        );
    });
    let curve = record(&memory.project, "box", 0, 1, 0);
    memory
        .project
        .set_hoi_trajectories(std::slice::from_ref(&curve))
        .unwrap();
    let saved = saved_files(&memory, files(&root()));
    let mut reopened = open(&saved);
    reopened.project.set_hoi_trajectories(&[]).unwrap();
    assert_eq!(
        reopened
            .project
            .document_designspace()
            .unwrap()
            .to_norad()
            .unwrap()
            .lib["com.example.opaque"],
        plist::Value::Data(vec![0, 1, 255])
    );
}
