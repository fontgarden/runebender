// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Scoped interpolation controls built from the shared panel vocabulary.

use crate::application::design::recipes::rows::{Group, Row, click, section_of};
use crate::application::editor::tools::interpolation::InterpolationEvent;
use crate::application::workspace::{Tool, Workspace};
use runebender::font::model::hoi_trajectories::HoiEasing;
use std::sync::Arc;
use xilem::WidgetView;

pub(crate) fn panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let mut groups = Vec::new();
    match (app.interpolation_intervals(), app.interpolation_canvas()) {
        (Ok(intervals), Ok(canvas)) => {
            let labels = intervals
                .iter()
                .map(|span| {
                    let name = |layer: &runebender::font::variable::LayerId| {
                        let source = app.font.project.document_source(layer.source);
                        let label = source
                            .map(|source| source.name().to_owned())
                            .unwrap_or_default();
                        if source.is_some_and(|source| source.default_layer().name != layer.name) {
                            format!("{label} / {}", layer.name)
                        } else {
                            label
                        }
                    };
                    format!("{} to {}", name(&span.lower), name(&span.upper))
                })
                .collect();
            groups.push(Group::new(
                "Interval",
                vec![
                    Row::Meta(canvas.interval.axis_name.clone()),
                    Row::Picker {
                        choices: labels,
                        chosen: Some(app.interpolation_interval_index(&intervals)),
                        on_pick: Arc::new(Workspace::set_interpolation_interval),
                    },
                ],
            ));
            let easings = [
                HoiEasing::Linear,
                HoiEasing::EaseIn,
                HoiEasing::EaseOut,
                HoiEasing::EaseInOut,
            ];
            let chosen = easings.iter().position(|easing| {
                !canvas.selected.is_empty()
                    && canvas
                        .selected
                        .iter()
                        .all(|&index| canvas.connections[index].record.easing == *easing)
            });
            groups.push(Group::new(
                "Preview",
                vec![
                    Row::Choice {
                        options: vec!["Edit Paths".into(), "Preview".into()],
                        chosen: usize::from(canvas.preview_only),
                        on_pick: Arc::new(Workspace::set_interpolation_preview),
                    },
                    Row::Slider {
                        caption: "Progress".into(),
                        min: 0.0,
                        max: 100.0,
                        value: app.interpolation.progress * 100.0,
                        on_change: Arc::new(Workspace::scrub_interpolation),
                    },
                    Row::Choice {
                        options: vec!["Exact".into(), "Compare Export".into()],
                        chosen: usize::from(app.interpolation.compare),
                        on_pick: Arc::new(|app, index| app.interpolation.compare = index == 1),
                    },
                    Row::Meta(canvas.comparison_status.clone()),
                    Row::Meta("Endpoint drawings locked".into()),
                ],
            ));
            if !canvas.preview_only {
                groups.push(Group::new(
                    "Selected Paths",
                    vec![
                        Row::Meta(format!(
                            "{} of {} selected",
                            canvas.selected.len(),
                            canvas.connections.len()
                        )),
                        Row::Chips {
                            items: vec![
                                "Linear".into(),
                                "Ease In".into(),
                                "Ease Out".into(),
                                "Ease In/Out".into(),
                            ],
                            chosen,
                            on_pick: Arc::new(move |app, index| {
                                app.ease_interpolation(easings[index]);
                            }),
                        },
                        Row::Secondary {
                            text: "Reset Selected".into(),
                            on_click: click(|app| {
                                app.interpolation_event(InterpolationEvent::Reset);
                            }),
                        },
                        Row::Quiet {
                            text: "Select All Paths".into(),
                            on_click: click(|app| {
                                app.interpolation_event(InterpolationEvent::SelectAll);
                            }),
                        },
                    ],
                ));
            }
        }
        (_, Err(error)) | (Err(error), _) => groups.push(Group::new("", vec![Row::Prose(error)])),
    }
    groups.push(Group::new(
        "",
        vec![Row::Primary {
            text: "Done".into(),
            on_click: click(|app| app.select_tool(Tool::Select)),
        }],
    ));
    section_of(app, "Interpolation", "Interpolation", groups)
}
