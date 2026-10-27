// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Proof controls and comparison columns stay inside the center pane as it resizes.

use super::proof_view;
use crate::application::design::stock;
use crate::application::editor::tools::review::ReviewMode;
use crate::application::view::render::panel_resize_tests_support::context;
use crate::application::workspace::Workspace;
use masonry::core::{Widget, WidgetRef, WindowEvent};
use masonry::dpi::PhysicalSize;
use masonry_testing::TestHarness;
use xilem::core::View;
use xilem::view::sized_box;

/// A hidden portal scrollbar has no meaningful layout and is deliberately moved offscreen.
/// All visible content, including nested control labels, must fit the offered pane width.
fn horizontal_overflow(widget: WidgetRef<'_, dyn Widget>, width: f64, failures: &mut Vec<String>) {
    if widget.ctx().is_stashed()
        || (widget.short_type_name() == "ScrollBar" && widget.ctx().is_disabled())
    {
        return;
    }
    let bounds = widget
        .ctx()
        .window_transform()
        .transform_rect_bbox(widget.ctx().border_box());
    if bounds.x0 < -0.5 || bounds.x1 > width + 0.5 {
        failures.push(format!(
            "{} extends from {} to {} in a {width}-pixel pane",
            widget.short_type_name(),
            bounds.x0,
            bounds.x1,
        ));
    }
    for child in widget.children() {
        horizontal_overflow(child, width, failures);
    }
}

fn workspace() -> Workspace {
    let mut app = Workspace::untitled().expect("a new font opens");
    app.set_theme("gray");
    app.open_glyph(app.font.index_of("A").expect("Latin Core contains A"));
    app.pin_review_before();
    app
}

#[test]
fn review_controls_fit_after_mode_changes_and_real_window_resizes() {
    assert_controls_fit(workspace());
}

#[cfg(unix)]
#[test]
fn multi_glyph_proposal_actions_and_long_choices_fit_when_resized() {
    use runebender::automation::agent::ToolCall;

    let mut app = workspace();
    let task = "capital-spacing-review-before-release-with-a-long-proposal-name";
    let source = app.font.project.source_id(app.font.active()).unwrap();
    let edits: Vec<_> = ["A", "B"]
        .into_iter()
        .map(|glyph| {
            let read = app.call_live(&ToolCall {
                name: "read_glyph".into(),
                arguments: serde_json::json!({"glyph": glyph, "source": source.0}),
            });
            serde_json::json!({"glyph": glyph, "expected_revision": read["revision"],
                "operations": [{"op": "adjust_width", "delta": 20.0}]})
        })
        .collect();
    let reply = app.call_live(&ToolCall {
        name: "propose_edits".into(),
        arguments: serde_json::json!({"source": source.0, "task": task,
            "reason": "Exercise review controls for a multi-glyph proposal", "edits": edits}),
    });
    assert_eq!(reply["ok"], true, "{reply}");
    app.refresh_proposals();
    app.select_review_proposal(Some(task.into()));
    let comparison = app.review_comparison();
    assert_eq!(comparison.proposal_task.as_deref(), Some(task));
    assert_eq!(comparison.proposal_glyph_count, 2);
    assert!(
        comparison.can_apply_glyph,
        "the Apply glyph action must be exercised"
    );
    assert_controls_fit(app);
}

fn assert_controls_fit(mut app: Workspace) {
    // Proposal staging has already finished: subsequent changes are presentation only.
    let revision = app.font.project.document_revision();
    let selected_proposal = app.ai.preview_task.clone();
    let mut ctx = context();
    let mut view = sized_box(proof_view(&app));
    let (pod, mut state) = view.build(&mut ctx, &mut app);
    let mut harness = TestHarness::create_with_size(
        stock::widget_properties(&app.palette),
        pod.new_widget,
        (800, 700),
    );
    let mut failures = Vec::new();

    for (mode, chooser_open, show_before, dark) in [
        (ReviewMode::Compare, false, false, false),
        (ReviewMode::Compare, true, false, true),
        (ReviewMode::Toggle, true, false, true),
        (ReviewMode::Toggle, false, true, false),
        (ReviewMode::Toggle, false, false, true),
        (ReviewMode::Compare, false, false, false),
    ] {
        app.review.mode = mode;
        app.review.dark = dark;
        app.review.chooser_open = chooser_open;
        app.review.show_before = show_before;
        let next = sized_box(proof_view(&app));
        harness.edit_root_widget(|root| {
            next.rebuild(&view, &mut state, &mut ctx, root, &mut app);
        });
        view = next;
        for (width, height) in [(800, 700), (300, 550), (800, 700)] {
            harness.process_window_event(WindowEvent::Resize(PhysicalSize::new(width, height)));
            let mut overflow = Vec::new();
            horizontal_overflow(
                harness.root_widget().as_dyn(),
                f64::from(width),
                &mut overflow,
            );
            failures.extend(overflow.into_iter().map(|failure| {
                format!(
                    "{mode:?}, dark={dark}, chooser={chooser_open}, before={show_before}: {failure}"
                )
            }));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(app.ai.preview_task, selected_proposal);
    assert_eq!(
        app.font.project.document_revision(),
        revision,
        "review presentation must not edit the font",
    );
}
