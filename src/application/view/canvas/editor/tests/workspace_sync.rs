// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Tests of how the view passes widget edits to the workspace.

use super::*;

#[test]
fn an_outline_edit_refreshes_cached_outlines_for_the_glyph_and_its_composites() {
    use kurbo::Shape as _;
    let path = std::env::temp_dir().join(format!(
        "runebender-editor-refresh-{}-{}.ufo",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the system clock is after the Unix epoch")
            .as_nanos(),
    ));
    let mut font = norad::Font::new();
    font.default_layer_mut()
        .insert_glyph(projected_glyph(&session()));
    let mut composite = norad::Glyph::new("B");
    composite.width = 500.0;
    composite.components.push(norad::Component::new(
        norad::Name::new("A").expect("a valid glyph name"),
        norad::AffineTransform::default(),
        None,
    ));
    font.default_layer_mut().insert_glyph(composite);
    font.save(&path).expect("the fixture saves");
    let mut workspace = Workspace::open(&path).expect("the fixture opens");
    let (a, b) = (
        workspace.font.index_of("A").expect("A exists"),
        workspace.font.index_of("B").expect("B exists"),
    );
    let right_edge = |workspace: &Workspace, index: usize| {
        (
            workspace.font.glyphs[index].outline.bounding_box().x1,
            workspace.cells[index].outline.bounding_box().x1,
        )
    };
    assert_eq!(right_edge(&workspace, a), (400.0, 400.0));
    assert_eq!(right_edge(&workspace, b), (400.0, 400.0));

    workspace.open_glyph(a);
    let point = workspace.session.point_id_at(0, 1).unwrap();
    let mut changed = (*workspace.session).clone();
    changed.selection = HashSet::from([point]);
    changed.begin_point_drag();
    assert!(changed.drag_points_to((20.0, 0.0), false));
    changed.end_point_drag();
    dispatch_editor_event(
        &mut workspace,
        &mut changed,
        EditorEvent::Edited,
        |app, _| app.finish_open_glyph_refresh(),
    );
    // The grid, rail and previews read these caches, not the project.
    assert_eq!(right_edge(&workspace, a), (420.0, 420.0));
    assert_eq!(
        right_edge(&workspace, b),
        (420.0, 420.0),
        "a composite using the edited glyph refreshes too"
    );

    // Undo refreshes the same caches back.
    workspace.undo_open_glyph(false);
    assert_eq!(right_edge(&workspace, a), (400.0, 400.0));
    assert_eq!(right_edge(&workspace, b), (400.0, 400.0));
    let _ = std::fs::remove_dir_all(&path);
}

#[test]
fn unchanged_editor_release_skips_the_real_edited_callback_and_retains_redo() {
    let path = std::env::temp_dir().join(format!(
        "runebender-editor-noop-{}-{}.ufo",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the system clock is after the Unix epoch")
            .as_nanos(),
    ));
    let session = session();
    let mut font = norad::Font::new();
    font.default_layer_mut()
        .insert_glyph(projected_glyph(&session));
    font.save(&path).expect("the fixture saves");
    let mut workspace = Workspace::open(&path).expect("the fixture opens");
    workspace.open_glyph(0);
    let point = workspace.session.point_id_at(0, 0).unwrap();

    let mut changed = (*workspace.session).clone();
    changed.selection.insert(point);
    changed.begin_point_drag();
    assert!(changed.drag_points_to((20.0, 0.0), false));
    changed.end_point_drag();
    dispatch_editor_event(
        &mut workspace,
        &mut changed,
        EditorEvent::Edited,
        |app, _| app.finish_open_glyph_refresh(),
    );
    workspace.undo_open_glyph(false);
    let address = workspace.font.active_layer_address("A").unwrap();
    assert!(workspace.font.project.can_replay_document_layer_history(
        &address,
        runebender::font::history::HistoryDirection::Redo,
    ));

    workspace.modified = false;
    let revision = workspace.font.project.document_revision();
    let undo = workspace.metadata_undo.len();
    let redo = workspace.metadata_redo.len();
    let callbacks = std::cell::Cell::new(0);

    let mut unchanged = (*workspace.session).clone();
    unchanged.selection.insert(point);
    unchanged.begin_point_drag();
    unchanged.end_point_drag();
    dispatch_editor_event(
        &mut workspace,
        &mut unchanged,
        EditorEvent::Edited,
        |app, _| {
            callbacks.set(callbacks.get() + 1);
            app.finish_open_glyph_refresh();
        },
    );

    let mut out_and_back = (*workspace.session).clone();
    out_and_back.selection.insert(point);
    out_and_back.begin_point_drag();
    assert!(out_and_back.drag_points_to((20.0, 0.0), false));
    assert!(out_and_back.drag_points_to((0.0, 0.0), false));
    out_and_back.end_point_drag();
    dispatch_editor_event(
        &mut workspace,
        &mut out_and_back,
        EditorEvent::Edited,
        |app, _| {
            callbacks.set(callbacks.get() + 1);
            app.finish_open_glyph_refresh();
        },
    );

    assert_eq!(callbacks.get(), 0);
    assert!(!workspace.modified);
    assert_eq!(workspace.font.project.document_revision(), revision);
    assert_eq!(workspace.metadata_undo.len(), undo);
    assert_eq!(workspace.metadata_redo.len(), redo);
    assert!(workspace.font.project.can_replay_document_layer_history(
        &address,
        runebender::font::history::HistoryDirection::Redo,
    ));
    std::fs::remove_dir_all(path).expect("the fixture is removed");
}
