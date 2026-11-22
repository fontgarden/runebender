// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Source/layer authoring commands and refresh of source-dependent editor state.

use runebender::font::variable::{GlyphLayerAddress, LayerId};

use crate::application::workspace::{Mode, Workspace};

impl Workspace {
    pub(crate) fn can_manage_glyph_layers(&self) -> bool {
        matches!(self.mode, Mode::Editor(_))
    }

    /// The Layers panel's rows for the open glyph: the master's own layer, then its other
    /// layers.
    pub(crate) fn glyph_layer_rows(&self) -> Vec<GlyphLayerRow> {
        let glyph = &self.session.glyph_name;
        let editing = self
            .font
            .active_layer_address(glyph)
            .map(|address| address.layer);
        self.font
            .glyph_layers(glyph)
            .into_iter()
            .enumerate()
            .map(|(index, layer)| GlyphLayerRow {
                name: if index == 0 {
                    self.font.master_names()[self.font.active()].to_string()
                } else {
                    layer.name.clone()
                },
                master: index == 0,
                editing: editing.as_ref() == Some(&layer),
                visible: self.visible_layers.contains(&GlyphLayerAddress {
                    glyph: glyph.clone(),
                    layer: layer.clone(),
                }),
                layer,
            })
            .collect()
    }

    /// Outlines of the open glyph's layers whose eye is on, except the layer being edited.
    pub(crate) fn visible_layer_outlines(&self) -> Vec<std::sync::Arc<kurbo::BezPath>> {
        let glyph = &self.session.glyph_name;
        self.glyph_layer_rows()
            .into_iter()
            .filter(|row| row.visible && !row.editing)
            .filter_map(|row| self.font.layer_outline(glyph, &row.layer))
            .collect()
    }

    /// Edit `layer` of the open glyph. The master's own layer clears the choice.
    pub(crate) fn edit_glyph_layer(&mut self, layer: &LayerId) {
        let glyph = self.session.glyph_name.clone();
        let master = self.font.glyph_layers(&glyph).first() == Some(layer);
        self.park();
        if master {
            self.font.edit_layers.remove(&glyph);
            self.layer_name_buf.clear();
        } else {
            self.font.edit_layers.insert(glyph, layer.clone());
            self.layer_name_buf.clone_from(&layer.name);
        }
        self.refresh_source_views();
    }

    /// Rename the backup layer being edited to the name typed in the Layers panel.
    pub(crate) fn rename_glyph_layer(&mut self, name: &str) {
        let glyph = self.session.glyph_name.clone();
        let Some(layer) = self
            .font
            .active_layer_address(&glyph)
            .map(|address| address.layer)
        else {
            return;
        };
        let name = name.trim();
        if name == layer.name {
            return;
        }
        self.park();
        match self.font.project.rename_glyph_layer(&glyph, &layer, name) {
            Ok(renamed) => {
                let old = GlyphLayerAddress {
                    glyph: glyph.clone(),
                    layer,
                };
                let new = GlyphLayerAddress {
                    glyph: glyph.clone(),
                    layer: renamed.clone(),
                };
                if self.visible_layers.remove(&old) {
                    self.visible_layers.insert(new);
                }
                self.font.edit_layers.insert(glyph.clone(), renamed.clone());
                self.layer_name_buf.clone_from(&renamed.name);
                // The renamed layer starts a new edit history, at depth 0.
                self.record_layer_structure(&glyph, "layer rename", 0);
                self.refresh_source_views();
                self.modified = true;
                self.note = format!("Renamed the layer to {}", renamed.name);
            }
            Err(error) => self.note = error,
        }
    }

    /// Show or hide a layer of the open glyph behind the layer being edited.
    pub(crate) fn toggle_glyph_layer_visible(&mut self, layer: &LayerId) {
        let address = GlyphLayerAddress {
            glyph: self.session.glyph_name.clone(),
            layer: layer.clone(),
        };
        if !self.visible_layers.remove(&address) {
            self.visible_layers.insert(address);
        }
    }

    /// Copy the layer being edited into a new layer named with the date and time, as the plus
    /// button of the Glyphs Layers panel does. The copy is visible and the edited layer stays.
    pub(crate) fn duplicate_glyph_layer(&mut self) {
        let glyph = self.session.glyph_name.clone();
        let Some(from) = self
            .font
            .active_layer_address(&glyph)
            .map(|address| address.layer)
        else {
            return;
        };
        let stamp = backup_layer_name();
        let mut name = stamp.clone();
        let mut count = 2;
        while self
            .font
            .project
            .document_layer(
                &glyph,
                &LayerId {
                    source: from.source,
                    name: name.clone(),
                },
            )
            .is_some()
        {
            name = format!("{stamp} ({count})");
            count += 1;
        }
        self.park();
        let undo_depth = self
            .font
            .history_depth(&glyph, runebender::font::history::HistoryDirection::Undo);
        match self.font.project.add_glyph_layer(&glyph, &from, &name) {
            Ok(layer) => {
                self.record_layer_structure(&glyph, "new layer", undo_depth);
                self.visible_layers
                    .insert(GlyphLayerAddress { glyph, layer });
                self.refresh_source_views();
                self.modified = true;
                self.note = format!("Copied the layer to {name}");
            }
            Err(error) => self.note = error,
        }
    }

    /// Delete the layer being edited, when it is not the master's own layer, and return to the
    /// master layer.
    pub(crate) fn remove_glyph_layer(&mut self) {
        let glyph = self.session.glyph_name.clone();
        let Some(layer) = self
            .font
            .active_layer_address(&glyph)
            .map(|address| address.layer)
        else {
            return;
        };
        if self.font.glyph_layers(&glyph).first() == Some(&layer) {
            self.note = "A master layer cannot be deleted; select a backup layer".into();
            return;
        }
        self.park();
        self.font.edit_layers.remove(&glyph);
        // Undo reaches this entry from the master layer, where the deletion leaves the editor.
        let undo_depth = self
            .font
            .history_depth(&glyph, runebender::font::history::HistoryDirection::Undo);
        // One source change removes the layer and its folder when no other glyph uses it.
        match self.font.project.delete_glyph_layer(&glyph, &layer) {
            Ok(()) => {
                self.record_layer_structure(&glyph, "layer deletion", undo_depth);
                self.visible_layers.remove(&GlyphLayerAddress {
                    glyph,
                    layer: layer.clone(),
                });
                self.modified = true;
                self.note = format!("Deleted layer {}", layer.name);
            }
            Err(error) => self.note = error,
        }
        self.refresh_source_views();
    }
}

/// One row of the Layers panel.
pub(crate) struct GlyphLayerRow {
    /// The layer's address within the active source.
    pub(crate) layer: LayerId,
    /// The master's name for its own layer, else the layer's name.
    pub(crate) name: String,
    /// Whether this is the master's own layer, which exports and cannot be deleted.
    pub(crate) master: bool,
    /// Whether the canvas is editing this layer.
    pub(crate) editing: bool,
    /// Whether its outline is drawn behind the layer being edited.
    pub(crate) visible: bool,
}

/// A name for a new backup layer: the local date and time, as Glyphs names its backup layers.
/// Falls back to "Backup" where the `date` command is unavailable.
fn backup_layer_name() -> String {
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    if let Ok(output) = std::process::Command::new("date")
        .arg("+%b %-d, %y at %H:%M")
        .output()
        && output.status.success()
        && let Ok(text) = String::from_utf8(output.stdout)
        && !text.trim().is_empty()
    {
        return text.trim().to_owned();
    }
    "Backup".to_owned()
}

impl Workspace {
    /// Put a layer addition or deletion on the editor's undo stack.
    fn record_layer_structure(&mut self, glyph: &str, label: &str, undo_depth: usize) {
        self.metadata_undo.push(
            crate::application::workspace::MetadataEdit::LayerStructure {
                revision: self.font.project.document_revision(),
                glyph: glyph.to_owned(),
                label: label.to_owned(),
                undo_depth,
            },
        );
        self.metadata_redo.clear();
        self.overview_redo.clear();
    }

    /// Make the backup layer being edited the master's drawing: the two layers swap their
    /// contours and widths, so the old master drawing stays in the backup layer. The editor
    /// moves to the master layer. One Undo swaps them back.
    pub(crate) fn use_glyph_layer_as_master(&mut self) {
        let glyph = self.session.glyph_name.clone();
        let Some(address) = self.font.active_layer_address(&glyph) else {
            return;
        };
        let Some(master) = self.font.glyph_layers(&glyph).into_iter().next() else {
            return;
        };
        if address.layer == master {
            self.note = "This is the master layer; select a backup layer".into();
            return;
        }
        self.park();
        match self.font.project.swap_document_layers(&address, &master) {
            Ok(Some((group, change))) => {
                self.record_document_group(
                    group,
                    &change,
                    crate::application::workspace::DocumentGroupScope::Contextual,
                );
                self.modified = true;
                self.note = format!("{} is now the master drawing", address.layer.name);
                self.edit_glyph_layer(&master);
            }
            Ok(None) => self.note = "The layer already matches the master".into(),
            Err(error) => self.note = error,
        }
    }

    /// Whether the open glyph is being edited on a layer other than the master's own.
    pub(crate) fn editing_backup_layer(&self) -> bool {
        self.glyph_layer_rows()
            .iter()
            .any(|row| row.editing && !row.master)
    }
}

#[cfg(test)]
mod layer_tests {
    use runebender::font::project::Project;

    use crate::application::font_model::FontModel;
    use crate::application::workspace::{Mode, Workspace};

    fn workspace(root: &std::path::Path) -> Workspace {
        let mut project = Project::new_font(root.join("Layers.ufo"));
        let layer = project
            .document_source(project.source_id(0).unwrap())
            .unwrap()
            .default_layer();
        project.add_document_glyph("A", 500.0, Some(0x41)).unwrap();
        project
            .edit_document_layer("A", &layer, |draft| {
                draft.add_shape_contour(kurbo::Rect::new(40.0, 0.0, 460.0, 700.0), false)?;
                Ok(())
            })
            .unwrap();
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        app.open_glyph(app.font.index_of("A").unwrap());
        assert!(matches!(app.mode, Mode::Editor(_)));
        app
    }

    #[test]
    fn undo_and_redo_restore_and_repeat_layer_additions_and_deletions() {
        use crate::application::app_action::AppAction;
        let root = std::env::temp_dir().join(format!(
            "runebender-layer-undo-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = workspace(&root);
        app.duplicate_glyph_layer();
        assert_eq!(app.glyph_layer_rows().len(), 2, "{}", app.note);
        let backup = app.glyph_layer_rows()[1].layer.clone();

        // Undo removes the new layer; Redo brings it back with its drawing.
        app.dispatch(AppAction::Undo);
        assert_eq!(app.glyph_layer_rows().len(), 1, "{}", app.note);
        assert!(app.visible_layer_outlines().is_empty());
        app.dispatch(AppAction::Redo);
        assert_eq!(app.glyph_layer_rows().len(), 2, "{}", app.note);

        // A deleted layer comes back with Undo, and the editor is on the master layer.
        app.edit_glyph_layer(&backup);
        app.remove_glyph_layer();
        assert_eq!(app.glyph_layer_rows().len(), 1, "{}", app.note);
        app.dispatch(AppAction::Undo);
        let rows = app.glyph_layer_rows();
        assert_eq!(rows.len(), 2, "{}", app.note);
        assert!(rows[0].editing);
        assert_eq!(
            app.font
                .project
                .document_layer("A", &backup)
                .unwrap()
                .contours()
                .count(),
            1
        );
    }

    #[test]
    fn a_backup_layer_is_copied_edited_alone_saved_and_deleted() {
        let root = std::env::temp_dir().join(format!(
            "runebender-layers-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut app = workspace(&root);
        let rows = app.glyph_layer_rows();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].master && rows[0].editing);

        // Plus copies the edited layer; the master stays the edited layer and the copy shows.
        app.duplicate_glyph_layer();
        let rows = app.glyph_layer_rows();
        assert_eq!(rows.len(), 2, "{}", app.note);
        assert!(rows[0].editing && !rows[1].editing && rows[1].visible);
        assert_eq!(app.visible_layer_outlines().len(), 1);
        let (master, backup) = (rows[0].layer.clone(), rows[1].layer.clone());

        // Clicking the backup edits it: the session and every edit address follow.
        app.edit_glyph_layer(&backup);
        let address = app.font.active_layer_address("A").unwrap();
        assert_eq!(address.layer, backup);
        assert_eq!(app.session.current_layer().unwrap().contours().count(), 1);
        assert!(app.editing_backup_layer());
        assert!(
            app.visible_layer_outlines().is_empty(),
            "the edited layer is not a ghost"
        );
        app.font
            .project
            .edit_document_layer("A", &backup, |draft| {
                draft.add_shape_contour(kurbo::Rect::new(100.0, 100.0, 200.0, 200.0), false)?;
                Ok(())
            })
            .unwrap();
        let contours = |app: &Workspace, layer| {
            app.font
                .project
                .document_layer("A", layer)
                .unwrap()
                .contours()
                .count()
        };
        assert_eq!(contours(&app, &backup), 2);
        assert_eq!(contours(&app, &master), 1, "the master is untouched");

        // The backup is an ordinary UFO layer on disk.
        assert!(app.save(), "{}", app.note);
        let saved = norad::Font::load(root.join("Layers.ufo")).unwrap();
        let layer = saved
            .layers
            .get(&backup.name)
            .expect("the backup layer is saved");
        assert_eq!(layer.get_glyph("A").unwrap().contours.len(), 2);
        assert_eq!(
            saved.default_layer().get_glyph("A").unwrap().contours.len(),
            1
        );

        // Minus deletes the edited backup and returns to the master; the master cannot go.
        app.remove_glyph_layer();
        let rows = app.glyph_layer_rows();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].editing);
        app.remove_glyph_layer();
        assert_eq!(app.glyph_layer_rows().len(), 1);
        assert!(app.save(), "{}", app.note);
        let saved = norad::Font::load(root.join("Layers.ufo")).unwrap();
        assert!(
            saved.layers.get(&backup.name).is_none(),
            "the empty layer folder is removed"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_backup_layer_is_renamed_with_its_drawing_and_eye_and_undo_restores_the_name() {
        use crate::application::app_action::AppAction;
        let root = std::env::temp_dir().join(format!(
            "runebender-layer-rename-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = workspace(&root);
        app.duplicate_glyph_layer();
        let backup = app.glyph_layer_rows()[1].layer.clone();
        app.edit_glyph_layer(&backup);
        assert_eq!(app.layer_name_buf, backup.name);

        app.rename_glyph_layer("  Narrow sketch ");
        let rows = app.glyph_layer_rows();
        assert_eq!(rows.len(), 2, "{}", app.note);
        assert_eq!(rows[1].name, "Narrow sketch");
        assert!(
            rows[1].editing && rows[1].visible,
            "the choice and the eye follow the name"
        );
        assert_eq!(app.session.current_layer().unwrap().contours().count(), 1);

        // Undo restores the old name.
        app.dispatch(AppAction::Undo);
        let names: Vec<_> = app
            .glyph_layer_rows()
            .into_iter()
            .map(|row| row.name)
            .collect();
        assert!(names.contains(&backup.name), "{names:?}: {}", app.note);
        assert!(!names.contains(&"Narrow sketch".to_owned()));
    }

    #[test]
    fn use_as_master_swaps_the_drawings_and_undo_swaps_them_back() {
        use crate::application::app_action::AppAction;
        let root = std::env::temp_dir().join(format!(
            "runebender-layer-master-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = workspace(&root);
        app.duplicate_glyph_layer();
        let rows = app.glyph_layer_rows();
        let (master, backup) = (rows[0].layer.clone(), rows[1].layer.clone());
        app.font
            .project
            .edit_document_layer("A", &backup, |draft| {
                draft.add_shape_contour(kurbo::Rect::new(100.0, 100.0, 200.0, 200.0), false)?;
                Ok(())
            })
            .unwrap();
        let contours = |app: &Workspace, layer| {
            app.font
                .project
                .document_layer("A", layer)
                .unwrap()
                .contours()
                .count()
        };

        app.edit_glyph_layer(&backup);
        app.use_glyph_layer_as_master();
        assert_eq!(contours(&app, &master), 2, "{}", app.note);
        assert_eq!(contours(&app, &backup), 1, "the old master drawing is kept");
        assert!(
            app.glyph_layer_rows()[0].editing,
            "the editor is on the master"
        );
        assert_eq!(app.session.current_layer().unwrap().contours().count(), 2);

        app.dispatch(AppAction::Undo);
        assert_eq!(contours(&app, &master), 1, "{}", app.note);
        assert_eq!(contours(&app, &backup), 2);
    }
}
