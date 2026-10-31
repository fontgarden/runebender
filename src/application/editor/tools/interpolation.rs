// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Interpolation-tool intent and private pointer drafts; Project owns committed trajectories.

use std::collections::BTreeSet;
use std::sync::Arc;

use kurbo::{BezPath, CubicBez, Point, Rect, Shape, Vec2};
use runebender::font::history::HistoryDirection;
use runebender::font::hoi::HoiInterval;
use runebender::font::model::hoi_trajectories::{HoiEasing, HoiTrajectory};
use runebender::font::variable::GlyphLayerAddress;

use crate::application::workspace::{MetadataEdit, Tool, Workspace};

#[derive(Clone, Debug)]
pub(crate) struct InterpolationState {
    pub glyph: String,
    pub interval: usize,
    pub selected: BTreeSet<usize>,
    pub progress: f64,
    pub compare: bool,
    pub preview_only: bool,
    draft: Option<Draft>,
}

impl Default for InterpolationState {
    fn default() -> Self {
        Self {
            glyph: String::new(),
            interval: 0,
            selected: BTreeSet::new(),
            progress: 0.5,
            compare: false,
            preview_only: false,
            draft: None,
        }
    }
}

#[derive(Clone, Debug)]
struct Draft {
    revision: u64,
    glyph: String,
    interval: usize,
    records: Vec<HoiTrajectory>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Connection {
    pub record: HoiTrajectory,
    pub points: [Point; 4],
}

/// Immutable drawing inputs; the widget never receives a mutable font.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InterpolationCanvas {
    pub glyph: String,
    pub revision: u64,
    pub interval: HoiInterval,
    pub connections: Vec<Connection>,
    pub selected: BTreeSet<usize>,
    pub endpoints: [Arc<BezPath>; 2],
    pub preview: Arc<BezPath>,
    pub compiled: Option<Arc<BezPath>>,
    pub comparison_status: String,
    pub preview_only: bool,
}

impl InterpolationCanvas {
    /// A frame independent of the current weight, so scrubbing never rescales the proof.
    pub(crate) fn reference_bounds(&self) -> Rect {
        let mut bounds = self.endpoints[0]
            .bounding_box()
            .union(self.endpoints[1].bounding_box());
        for connection in &self.connections {
            let [a, b, c, d] = connection.points;
            bounds = bounds.union(CubicBez::new(a, b, c, d).bounding_box());
        }
        bounds
    }
}

#[derive(Clone, Debug)]
pub(crate) enum InterpolationEvent {
    Select(Vec<usize>),
    Begin(Vec<usize>),
    Controls(Vec<(usize, [[f64; 2]; 2])>),
    Finish,
    Cancel,
    Nudge { delta: Vec2, control: Option<usize> },
    Reset,
    SelectAll,
}

impl Workspace {
    fn ensure_interpolation(&mut self) {
        if self.interpolation.glyph != self.session.glyph_name {
            let intervals = self
                .font
                .project
                .hoi_intervals(&self.session.glyph_name)
                .unwrap_or_default();
            let source = self.font.project.source_id(self.font.active());
            let interval = intervals
                .iter()
                .position(|span| Some(span.lower.source) == source)
                .unwrap_or_else(|| intervals.len().saturating_sub(1));
            self.interpolation = InterpolationState {
                glyph: self.session.glyph_name.clone(),
                interval,
                ..InterpolationState::default()
            };
        }
    }

    pub(crate) fn begin_interpolation(&mut self) {
        self.ensure_interpolation();
        self.interpolation.draft = None;
        self.has_text_session = false;
        self.right_collapsed = false;
        if let Err(error) = self.interpolation_canvas() {
            self.note = error;
        }
    }

    pub(crate) fn cancel_interpolation_draft(&mut self) -> bool {
        self.interpolation.draft.take().is_some()
    }

    pub(crate) fn interpolation_intervals(&self) -> Result<Vec<HoiInterval>, String> {
        self.font.project.hoi_intervals(&self.session.glyph_name)
    }

    pub(crate) fn interpolation_interval_index(&self, intervals: &[HoiInterval]) -> usize {
        if self.interpolation.glyph == self.session.glyph_name {
            self.interpolation
                .interval
                .min(intervals.len().saturating_sub(1))
        } else {
            let source = self.font.project.source_id(self.font.active());
            intervals
                .iter()
                .position(|span| Some(span.lower.source) == source)
                .unwrap_or_else(|| intervals.len().saturating_sub(1))
        }
    }

    pub(crate) fn interpolation_canvas(&self) -> Result<Arc<InterpolationCanvas>, String> {
        let project = &self.font.project;
        let glyph = &self.session.glyph_name;
        let intervals = self.interpolation_intervals()?;
        let index = self.interpolation_interval_index(&intervals);
        let interval = intervals
            .get(index)
            .ok_or("This glyph needs two contributing sources")?
            .clone();
        let mut records = project.hoi_trajectories()?;
        if let Some(draft) = self
            .interpolation
            .draft
            .as_ref()
            .filter(|draft| &draft.glyph == glyph)
        {
            if draft.revision != project.document_revision() || draft.interval != index {
                return Err(
                    "The sources changed during this gesture; press Escape and try again".into(),
                );
            }
            records.clone_from(&draft.records);
        }
        let mut connections = Vec::new();
        for mut record in project.hoi_connections(glyph, &interval)? {
            if let Some(draft) = records
                .iter()
                .find(|candidate| candidate.same_scope(&record))
            {
                record.clone_from(draft);
            }
            connections.push(Connection {
                points: project.hoi_trajectory_geometry(&record)?,
                record,
            });
        }
        let location = project.hoi_location(&interval, self.interpolation.progress)?;
        let preview = Arc::new(project.preview_hoi_trajectories(glyph, &location, &records)?);
        let endpoint = |layer| {
            project
                .document_layer_path(&GlyphLayerAddress {
                    glyph: glyph.clone(),
                    layer,
                })
                .map(Arc::new)
                .map_err(|error| error.to_string())
        };
        let mut compiled = None;
        let comparison_status = if !self.interpolation.compare {
            "Export comparison off".into()
        } else if self.interpolation.draft.is_some() {
            "Release the pointer to compare the export".into()
        } else {
            match project.request_preview() {
                Ok(Some(_)) if project.preview_pending() => "Compiling export preview…".into(),
                Ok(Some(font)) => {
                    let coordinates = project
                        .document_designspace()
                        .expect("HOI has a Designspace")
                        .axes()
                        .iter()
                        .map(|axis| location.get(&axis.coordinates.name).copied().unwrap_or(0.0))
                        .collect::<Vec<_>>();
                    match font.outlines(&coordinates) {
                        Ok(outlines) => {
                            compiled = outlines
                                .into_iter()
                                .find(|(name, _)| name == glyph)
                                .map(|(_, path)| path);
                            if compiled.is_some() {
                                "Export overlay ready".into()
                            } else {
                                "Glyph is absent from exported font".into()
                            }
                        }
                        Err(error) => format!("Export preview: {error}"),
                    }
                }
                Ok(None) => "Compiling export preview…".into(),
                Err(error) => format!("Export preview: {error}"),
            }
        };
        Ok(Arc::new(InterpolationCanvas {
            glyph: glyph.clone(),
            revision: project.document_revision(),
            endpoints: [
                endpoint(interval.lower.clone())?,
                endpoint(interval.upper.clone())?,
            ],
            interval,
            selected: if self.interpolation.glyph == *glyph {
                self.interpolation
                    .selected
                    .iter()
                    .copied()
                    .filter(|index| *index < connections.len())
                    .collect()
            } else {
                BTreeSet::new()
            },
            connections,
            preview,
            compiled,
            comparison_status,
            preview_only: self.interpolation.preview_only,
        }))
    }

    pub(crate) fn set_interpolation_interval(&mut self, index: usize) {
        self.ensure_interpolation();
        self.interpolation.draft = None;
        self.interpolation.interval = index;
        self.interpolation.selected.clear();
        self.note.clear();
    }

    pub(crate) fn set_interpolation_preview(&mut self, index: usize) {
        self.ensure_interpolation();
        self.interpolation.draft = None;
        self.interpolation.preview_only = index == 1;
    }

    pub(crate) fn scrub_interpolation(&mut self, value: f64) {
        self.ensure_interpolation();
        self.interpolation.progress = (value / 100.0).clamp(0.0, 1.0);
    }

    fn install_interpolation(&mut self, records: &[HoiTrajectory]) -> Result<(), String> {
        let glyph = self.session.glyph_name.clone();
        let undo_depth = self.font.history_depth(&glyph, HistoryDirection::Undo);
        if self.font.project.set_hoi_trajectories(records)? {
            self.metadata_undo
                .push(MetadataEdit::Interpolation { glyph, undo_depth });
            self.metadata_redo.clear();
            self.modified = true;
            self.note = "Interpolation updated".into();
        }
        Ok(())
    }

    pub(crate) fn ease_interpolation(&mut self, easing: HoiEasing) {
        self.ensure_interpolation();
        if self.interpolation.preview_only {
            return;
        }
        self.interpolation.draft = None;
        let result = (|| {
            let canvas = self.interpolation_canvas()?;
            let mut records = self.font.project.hoi_trajectories()?;
            for index in &self.interpolation.selected {
                let Some(connection) = canvas.connections.get(*index) else {
                    continue;
                };
                if connection.record.easing == easing {
                    continue;
                }
                let mut record = connection.record.clone();
                record.easing = easing;
                records.retain(|old| !old.same_scope(&record));
                records.push(record);
            }
            self.install_interpolation(&records)
        })();
        if let Err(error) = result {
            self.note = error;
        }
    }

    pub(crate) fn interpolation_event(&mut self, event: InterpolationEvent) {
        self.ensure_interpolation();
        if self.interpolation.preview_only && !matches!(event, InterpolationEvent::Cancel) {
            return;
        }
        let result = self.apply_interpolation_event(event);
        if let Err(error) = result {
            self.interpolation.draft = None;
            self.note = error;
        }
    }

    fn apply_interpolation_event(&mut self, event: InterpolationEvent) -> Result<(), String> {
        match event {
            InterpolationEvent::Select(selected) => {
                self.interpolation.selected = selected.into_iter().collect();
            }
            InterpolationEvent::SelectAll => {
                self.interpolation.selected =
                    (0..self.interpolation_canvas()?.connections.len()).collect();
            }
            InterpolationEvent::Begin(selected) => {
                self.interpolation.selected = selected.into_iter().collect();
                self.interpolation.draft = Some(Draft {
                    revision: self.font.project.document_revision(),
                    glyph: self.session.glyph_name.clone(),
                    interval: self.interpolation.interval,
                    records: self.font.project.hoi_trajectories()?,
                });
            }
            InterpolationEvent::Controls(updates) => {
                let canvas = self.interpolation_canvas()?;
                let draft = self
                    .interpolation
                    .draft
                    .as_mut()
                    .ok_or("Interpolation gesture is no longer active")?;
                for (index, controls) in updates {
                    if !self.interpolation.selected.contains(&index) {
                        return Err("Unselected trajectory in gesture".into());
                    }
                    let mut record = canvas
                        .connections
                        .get(index)
                        .ok_or("Trajectory is missing")?
                        .record
                        .clone();
                    if record.controls == controls {
                        continue;
                    }
                    record.controls = controls;
                    record.validate()?;
                    draft.records.retain(|old| !old.same_scope(&record));
                    draft.records.push(record);
                }
            }
            InterpolationEvent::Finish => {
                if let Some(draft) = self.interpolation.draft.take() {
                    if draft.revision != self.font.project.document_revision()
                        || draft.glyph != self.session.glyph_name
                        || draft.interval != self.interpolation.interval
                    {
                        return Err(
                            "Interpolation changed during this gesture; the draft was discarded"
                                .into(),
                        );
                    }
                    let interval = &self.interpolation_canvas()?.interval;
                    let originals = self.font.project.hoi_connections(&draft.glyph, interval)?;
                    let committed = self.font.project.hoi_trajectories()?;
                    let mut records = draft.records;
                    records.retain(|record| {
                        committed.iter().any(|old| old.same_scope(record))
                            || !originals.iter().any(|original| original == record)
                    });
                    // Keep record order stable when the final geometry is unchanged.
                    if records.len() != committed.len()
                        || records.iter().any(|record| !committed.contains(record))
                    {
                        self.install_interpolation(&records)?;
                    }
                }
            }
            InterpolationEvent::Cancel => {
                self.interpolation.draft = None;
                self.note.clear();
            }
            InterpolationEvent::Reset => {
                self.interpolation.draft = None;
                let canvas = self.interpolation_canvas()?;
                let mut records = self.font.project.hoi_trajectories()?;
                records.retain(|record| {
                    !self.interpolation.selected.iter().any(|index| {
                        canvas
                            .connections
                            .get(*index)
                            .is_some_and(|connection| record.same_scope(&connection.record))
                    })
                });
                self.install_interpolation(&records)?;
            }
            InterpolationEvent::Nudge { delta, control } => {
                let canvas = self.interpolation_canvas()?;
                let updates = self
                    .interpolation
                    .selected
                    .iter()
                    .filter_map(|&index| {
                        let mut controls = canvas.connections.get(index)?.record.controls;
                        for (i, point) in controls.iter_mut().enumerate() {
                            if control.is_none_or(|control| control == i) {
                                point[0] += delta.x;
                                point[1] += delta.y;
                            }
                        }
                        Some((index, controls))
                    })
                    .collect();
                self.apply_interpolation_event(InterpolationEvent::Begin(
                    self.interpolation.selected.iter().copied().collect(),
                ))?;
                self.apply_interpolation_event(InterpolationEvent::Controls(updates))?;
                self.apply_interpolation_event(InterpolationEvent::Finish)?;
            }
        }
        Ok(())
    }

    pub(crate) fn interpolation_is_active(&self) -> bool {
        self.persistent_tool() == Tool::Interpolation
    }
}

/// Outline-changing commands are unavailable while the endpoint drawings are locked.
pub(crate) fn command_allowed(action: crate::application::widgets::shortcuts::AppAction) -> bool {
    use crate::application::widgets::shortcuts::AppAction as A;
    matches!(
        action,
        A::Quit
            | A::OpenFont
            | A::NewFont
            | A::SaveAs
            | A::RevertToSaved
            | A::ExportFont
            | A::Save
            | A::Undo
            | A::Redo
            | A::Overview
            | A::Tool(_)
            | A::BeginSpacePan
            | A::EndSpacePan
            | A::Copy
            | A::CopySelectedGlyphs
            | A::SelectAll
            | A::DeselectAll
            | A::InvertSelection
            | A::CycleTheme
            | A::Theme(_)
            | A::ToggleTranslucency
            | A::ZoomToFit
            | A::ShowAllMasters
            | A::NextMaster
            | A::PreviousMaster
            | A::NextSampleString
            | A::PreviousSampleString
            | A::GridDots
            | A::GridLines
            | A::MeasureColorize
            | A::MeasureHandles
            | A::MeasureSegments
            | A::MeasureSizes
            | A::MeasureSpans
            | A::MeasureSideBearings
            | A::MeasurePopcount
            | A::MeasureAllOn
            | A::MeasureAllOff
            | A::GraphTab
            | A::GraphNew
            | A::GraphOpen
            | A::GraphSave
            | A::CheckJoining
            | A::ExportGlyphSvg
            | A::SortByName
            | A::SortByUnicode
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::widgets::shortcuts::AppAction;

    fn shapes() -> Workspace {
        let mut app = Workspace::open(std::path::Path::new(
            "assets/font-sources/variable-fonts/HOIShapes/HOIShapes.designspace",
        ))
        .unwrap();
        app.open_glyph(app.font.index_of("box").unwrap());
        app.select_tool(Tool::Interpolation);
        app.set_interpolation_interval(0);
        app
    }

    fn bend(app: &mut Workspace, indices: &[usize], amount: f64) {
        app.interpolation_event(InterpolationEvent::Begin(indices.to_vec()));
        let canvas = app.interpolation_canvas().unwrap();
        let updates = indices
            .iter()
            .map(|&i| {
                let mut controls = canvas.connections[i].record.controls;
                controls[0][1] += amount;
                controls[1][1] += amount;
                (i, controls)
            })
            .collect();
        app.interpolation_event(InterpolationEvent::Controls(updates));
    }

    #[test]
    fn hoi_drafts_cancel_and_commit_once_without_touching_endpoints() {
        let mut app = shapes();
        let original = app.interpolation_canvas().unwrap();
        let revision = app.font.project.document_revision();
        bend(&mut app, &[0, 1], 50.0);
        assert_ne!(
            app.interpolation_canvas().unwrap().preview,
            original.preview
        );
        assert_eq!(app.font.project.document_revision(), revision);
        assert!(!app.modified);
        assert!(app.metadata_undo.is_empty());
        app.interpolation_event(InterpolationEvent::Cancel);
        assert_eq!(
            app.interpolation_canvas().unwrap().preview,
            original.preview
        );
        bend(&mut app, &[0, 1], 60.0);
        let draft = app.interpolation_canvas().unwrap();
        app.interpolation_event(InterpolationEvent::Finish);
        assert_eq!(app.interpolation_canvas().unwrap().preview, draft.preview);
        assert_eq!(app.metadata_undo.len(), 1);
        assert_eq!(app.font.project.hoi_trajectories().unwrap().len(), 2);
        assert_eq!(
            app.interpolation_canvas().unwrap().endpoints,
            original.endpoints
        );
        app.undo_open_glyph(false);
        assert!(app.font.project.hoi_trajectories().unwrap().is_empty());
        app.undo_open_glyph(true);
        assert_eq!(app.interpolation_canvas().unwrap().preview, draft.preview);
        assert_eq!(
            app.interpolation_canvas().unwrap().endpoints,
            original.endpoints
        );
    }

    #[test]
    fn hoi_easing_reset_and_commands_stay_in_the_selected_scope() {
        let mut app = shapes();
        let original = app.interpolation_canvas().unwrap().endpoints.clone();
        app.interpolation_event(InterpolationEvent::Select(vec![0]));
        app.ease_interpolation(HoiEasing::EaseIn);
        let first = app.font.project.hoi_trajectories().unwrap()[0].clone();
        app.set_interpolation_interval(1);
        app.interpolation_event(InterpolationEvent::Select(vec![1]));
        app.ease_interpolation(HoiEasing::EaseOut);
        assert_eq!(app.font.project.hoi_trajectories().unwrap().len(), 2);
        app.interpolation_event(InterpolationEvent::Reset);
        assert_eq!(app.font.project.hoi_trajectories().unwrap(), [first]);
        app.set_interpolation_interval(0);
        app.dispatch(AppAction::SelectAll);
        assert_eq!(app.interpolation.selected.len(), 4);
        for action in [
            AppAction::FlipHorizontal,
            AppAction::RemoveOverlap,
            AppAction::Paste,
        ] {
            app.dispatch(action);
        }
        assert_eq!(app.interpolation_canvas().unwrap().endpoints, original);
        app.dispatch(AppAction::DeselectAll);
        assert!(app.interpolation.selected.is_empty());
        app.scrub_interpolation(0.0);
        assert_eq!(app.interpolation_canvas().unwrap().preview, original[0]);
        app.scrub_interpolation(100.0);
        assert_eq!(app.interpolation_canvas().unwrap().preview, original[1]);
    }

    #[test]
    fn hoi_preview_is_read_only_and_keeps_a_fixed_frame_while_scrubbing() {
        let mut app = Workspace::open(std::path::Path::new(
            "assets/font-sources/variable-fonts/HOIShapesLetters/HOIShapesLetters.designspace",
        ))
        .unwrap();
        app.open_glyph(app.font.index_of("a").unwrap());
        app.select_tool(Tool::Interpolation);
        app.set_interpolation_interval(1);
        let endpoints = app.interpolation_canvas().unwrap().endpoints.clone();
        bend(&mut app, &[0], 30.0);
        app.interpolation_event(InterpolationEvent::Finish);
        let records = app.font.project.hoi_trajectories().unwrap();
        let revision = app.font.project.document_revision();
        let history = app.metadata_undo.len();
        let selected = app.interpolation.selected.clone();
        bend(&mut app, &[0], 80.0);
        app.set_interpolation_preview(1);
        assert!(
            app.interpolation.draft.is_none(),
            "mode switch cancels unfinished edits"
        );
        let bounds = app.interpolation_canvas().unwrap().reference_bounds();
        for progress in [0.0, 25.0, 50.0, 75.0, 100.0] {
            app.scrub_interpolation(progress);
            let canvas = app.interpolation_canvas().unwrap();
            assert!(canvas.preview_only);
            assert_eq!(canvas.reference_bounds(), bounds);
            assert_eq!(canvas.endpoints, endpoints);
        }
        assert_eq!(app.interpolation_canvas().unwrap().preview, endpoints[1]);
        app.ease_interpolation(HoiEasing::EaseOut);
        app.interpolation_event(InterpolationEvent::Reset);
        app.interpolation_event(InterpolationEvent::Nudge {
            delta: Vec2::new(100.0, 0.0),
            control: None,
        });
        app.interpolation_event(InterpolationEvent::Begin(vec![1]));
        assert_eq!(app.font.project.hoi_trajectories().unwrap(), records);
        assert_eq!(app.font.project.document_revision(), revision);
        assert_eq!(app.metadata_undo.len(), history);
        assert_eq!(app.interpolation.selected, selected);
        app.set_interpolation_preview(0);
        assert!(!app.interpolation_canvas().unwrap().preview_only);
        assert_eq!(app.interpolation.selected, selected);
    }

    #[test]
    fn hoi_no_op_and_abandoned_gestures_make_no_history() {
        let mut app = shapes();
        let original = app.interpolation_canvas().unwrap();
        bend(&mut app, &[0], 50.0);
        app.interpolation_event(InterpolationEvent::Controls(vec![(
            0,
            original.connections[0].record.controls,
        )]));
        app.interpolation_event(InterpolationEvent::Finish);
        assert!(app.metadata_undo.is_empty());
        assert!(app.font.project.hoi_trajectories().unwrap().is_empty());
        bend(&mut app, &[0], 50.0);
        app.set_interpolation_interval(1);
        app.interpolation_event(InterpolationEvent::Finish);
        assert!(app.metadata_undo.is_empty());
        bend(&mut app, &[0], 50.0);
        app.select_tool(Tool::Select);
        app.interpolation_event(InterpolationEvent::Finish);
        assert!(app.metadata_undo.is_empty());
    }

    #[test]
    fn hoi_authored_curves_save_and_reopen_in_a_disposable_copy() {
        let mut app = shapes();
        bend(&mut app, &[0, 2], 90.0);
        app.interpolation_event(InterpolationEvent::Finish);
        app.ease_interpolation(HoiEasing::EaseInOut);
        let expected = app.font.project.hoi_trajectories().unwrap();
        let preview = app.interpolation_canvas().unwrap().preview.clone();
        let dir = std::env::temp_dir().join(format!(
            "runebender-hoi-ui-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        assert!(app.save_as_to(&dir), "{}", app.note);
        let mut reopened = Workspace::open(&dir.join("HOIShapes.designspace")).unwrap();
        reopened.open_glyph(reopened.font.index_of("box").unwrap());
        reopened.select_tool(Tool::Interpolation);
        reopened.set_interpolation_interval(0);
        assert_eq!(reopened.font.project.hoi_trajectories().unwrap(), expected);
        assert_eq!(reopened.interpolation_canvas().unwrap().preview, preview);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
