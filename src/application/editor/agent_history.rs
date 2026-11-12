// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared application history and cache refresh for canonical command and agent edit groups.

use std::collections::BTreeSet;
use std::sync::Arc;

use runebender::font::history::HistoryDirection;
use runebender::font::project::{
    DocumentChange, DocumentEditHistoryReplayOutcome, EditHistoryGroupId,
};

use crate::application::view::canvas::grid::cells_of;
use crate::application::workspace::{DocumentGroupScope, MetadataEdit, Mode, Workspace};

impl Workspace {
    pub(crate) fn record_document_group(
        &mut self,
        group: EditHistoryGroupId,
        change: &DocumentChange,
        scope: DocumentGroupScope,
    ) {
        self.metadata_undo.retain(|edit| match edit {
            MetadataEdit::DocumentGroup { group, .. } => self
                .font
                .project
                .document_edit_history_group_state(*group)
                .is_some(),
            _ => true,
        });
        self.metadata_undo.push(MetadataEdit::DocumentGroup {
            group,
            addresses: change.affected_layers().to_vec(),
            revision: self.font.project.document_revision(),
            scope,
        });
        self.metadata_redo.clear();
        self.overview_redo.clear();
        self.refresh_document_group_change(change);
    }

    /// Refresh only active-source views; switching sources rebuilds other source views canonically.
    /// Refresh the cached glyph entries a document change touched in the active source,
    /// including composites that use a changed glyph, and reload other tabs showing them.
    ///
    /// The grid, rail, previews and text inputs read these entries, so every committed
    /// outline edit must pass through here. The open glyph's own session is left to its
    /// caller. Returns the refreshed glyph names.
    pub(crate) fn refresh_changed_glyphs(&mut self, change: &DocumentChange) -> BTreeSet<String> {
        let active_source = self.font.project.source_id(self.font.active());
        let addresses = change
            .affected_layers()
            .iter()
            .chain(change.dependent_layers());
        let names = addresses
            .filter(|address| Some(address.layer.source) == active_source)
            .filter(|address| {
                self.font.active_layer_address(&address.glyph).as_ref() == Some(*address)
            })
            .map(|address| address.glyph.clone())
            .collect::<BTreeSet<_>>();
        for name in &names {
            if let Some(index) = self.font.index_of(name) {
                self.refresh_glyph_entry(index);
            }
        }
        for tab in &mut self.tabs {
            if tab.session.glyph_name != self.session.glyph_name
                && names.contains(&tab.session.glyph_name)
                && let Some(address) = self.font.active_layer_address(&tab.session.glyph_name)
            {
                Arc::make_mut(&mut tab.session).reload_from_project(&self.font.project, &address);
            }
        }
        names
    }

    /// Re-read one glyph's cached entry and its grid cell after its layer changed.
    ///
    /// The grid and rail cells mirror the entries one to one; refreshing only the changed
    /// one avoids measuring every caption in the font again.
    pub(crate) fn refresh_glyph_entry(&mut self, index: usize) {
        self.font.refresh_entry(index);
        if index < self.cells.len() {
            Arc::make_mut(&mut self.cells)[index] =
                crate::application::view::canvas::grid::cell_of(&self.font, index, &self.palette);
        }
    }

    /// Rebuild every grid cell when the glyph list changed shape; a commit has already
    /// refreshed the cells of the glyphs it touched.
    pub(crate) fn refresh_cells_if_reshaped(&mut self) {
        if self.cells.len() != self.font.glyphs.len() {
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
        }
    }

    fn refresh_document_group_change(&mut self, change: &DocumentChange) {
        let names = self.refresh_changed_glyphs(change);
        for tab in &mut self.tabs {
            if tab.session.glyph_name == self.session.glyph_name
                && names.contains(&tab.session.glyph_name)
                && let Some(address) = self.font.active_layer_address(&tab.session.glyph_name)
            {
                Arc::make_mut(&mut tab.session).reload_from_project(&self.font.project, &address);
            }
        }
        if names.contains(&self.session.glyph_name)
            && let Some(address) = self.font.active_layer_address(&self.session.glyph_name)
        {
            Arc::make_mut(&mut self.session).reload_from_project(&self.font.project, &address);
            if let Some(tab) = self.tabs.get_mut(self.active_tab)
                && tab.session.glyph_name == self.session.glyph_name
            {
                tab.session = self.session.clone();
            }
            self.selected_points = self.session.selection.len();
            self.refresh_metric_bufs();
            self.refresh_coord_bufs();
        }
        if !names.is_empty() {
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
        }
        self.modified |= self.font.project.is_modified();
    }

    pub(crate) fn document_group_in_context(&self, edit: &MetadataEdit) -> bool {
        let MetadataEdit::DocumentGroup {
            addresses, scope, ..
        } = edit
        else {
            return false;
        };
        if *scope == DocumentGroupScope::Document || matches!(self.mode, Mode::Graph) {
            return true;
        }
        let glyph = match self.mode {
            Mode::Editor(_) => Some(self.session.glyph_name.as_str()),
            Mode::Overview => self
                .selected
                .and_then(|index| self.font.glyphs.get(index))
                .map(|glyph| glyph.name.as_str()),
            Mode::Graph => None,
        };
        let Some(address) = glyph.and_then(|glyph| self.font.active_layer_address(glyph)) else {
            return false;
        };
        addresses.iter().any(|target| {
            target.glyph == address.glyph && target.layer.source == address.layer.source
        })
    }

    pub(crate) fn can_document_group_step(&self, edit: &MetadataEdit, redo: bool) -> bool {
        let MetadataEdit::DocumentGroup { group, .. } = edit else {
            return false;
        };
        !self.session.gesture_in_progress()
            && self.document_group_in_context(edit)
            && self
                .font
                .project
                .check_document_edit_history_group(*group, direction(redo))
                .is_ok()
    }

    pub(crate) fn document_group_step(&mut self, edit: &MetadataEdit, redo: bool) -> bool {
        if !self.document_group_in_context(edit) {
            return false;
        }
        if self.session.gesture_in_progress() {
            self.note = "Finish the canvas gesture before history replay".into();
            return true;
        }
        let MetadataEdit::DocumentGroup { group, .. } = edit else {
            return false;
        };
        if let Err(error) = self.replay_document_group(*group, direction(redo)) {
            self.note = error;
        }
        // A stale newest group must never fall through to an older per-layer edit.
        true
    }

    /// Targeted and ordinary history both move this same entry after one guarded engine replay.
    pub(crate) fn replay_document_group(
        &mut self,
        group: EditHistoryGroupId,
        direction: HistoryDirection,
    ) -> Result<DocumentEditHistoryReplayOutcome, String> {
        if self.session.gesture_in_progress() {
            return Err("finish the canvas gesture before history replay".into());
        }
        let redo = direction == HistoryDirection::Redo;
        let entries = if redo {
            &self.metadata_redo
        } else {
            &self.metadata_undo
        };
        let position = entries.iter().position(|edit| matches!(edit, MetadataEdit::DocumentGroup { group: candidate, .. } if *candidate == group))
            .ok_or("group has no application history entry in this direction; inspect its current state")?;
        let result = self
            .font
            .project
            .replay_document_edit_history_group(group, direction)
            .map_err(|error| error.to_string())?;
        let edit = if redo {
            self.metadata_redo.remove(position)
        } else {
            self.metadata_undo.remove(position)
        };
        let entries = if redo {
            &mut self.metadata_undo
        } else {
            &mut self.metadata_redo
        };
        let position = if redo {
            entries.partition_point(|candidate| candidate.revision() < edit.revision())
        } else {
            entries.partition_point(|candidate| candidate.revision() > edit.revision())
        };
        entries.insert(position, edit);
        self.refresh_document_group_change(&result.change);
        let label = self
            .font
            .project
            .document_edit_history_group_name(group)
            .unwrap_or("agent edit");
        self.note = format!("{} {label}", if redo { "Redid" } else { "Undid" });
        Ok(result)
    }
}

fn direction(redo: bool) -> HistoryDirection {
    if redo {
        HistoryDirection::Redo
    } else {
        HistoryDirection::Undo
    }
}
