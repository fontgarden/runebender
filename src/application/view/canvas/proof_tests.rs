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

#[test]
fn project_reference_rows_fit_in_every_mode() {
    let mut app = workspace();
    app.review.reading.references.faces.push(
        runebender::font::reference::ReferenceFace::parse(std::sync::Arc::new(
            include_bytes!("../../../../assets/fonts/VirtuaGrotesk-Regular.ttf").to_vec(),
        ))
        .unwrap(),
    );
    assert_controls_fit(app);
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
    let original = runebender::font::compiler::proof::capture(&app.font.project).unwrap();
    let fonts = app.review_reading_fonts();
    let working = fonts[0].bytes.as_ref().expect("Working compiles");
    let proposal = fonts[1].bytes.as_ref().expect("Proposal compiles");
    assert_ne!(
        working.as_slice(),
        proposal.as_slice(),
        "proposal width must reach the compiled specimen"
    );
    let after = runebender::font::compiler::proof::capture(&app.font.project).unwrap();
    assert_eq!(
        original.canonical_input_sha256(),
        after.canonical_input_sha256(),
        "compiling a proposal must leave foreground bytes unchanged"
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

    for (mode, show_before, dark) in [
        (ReviewMode::Compare, false, false),
        (ReviewMode::Compare, false, true),
        (ReviewMode::Toggle, false, true),
        (ReviewMode::Toggle, true, false),
        (ReviewMode::Toggle, false, true),
        (ReviewMode::Text, false, false),
        (ReviewMode::Waterfall, false, true),
        (ReviewMode::Compare, false, false),
    ] {
        app.review.mode = mode;
        app.review.dark = dark;
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
            failures.extend(
                overflow.into_iter().map(|failure| {
                    format!("{mode:?}, dark={dark}, before={show_before}: {failure}")
                }),
            );
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

#[test]
fn revision_buttons_and_repeated_toggle_clicks_stay_in_place() {
    use crate::application::design::controls::control::ControlPress;
    use masonry::core::WidgetId;
    use xilem::core::{DynMessage, Environment, MessageCtx, MessageResult};

    fn has_label(widget: WidgetRef<'_, dyn Widget>, name: &str) -> bool {
        widget
            .downcast::<masonry::widgets::Label>()
            .is_some_and(|label| label.text().as_ref() == name)
            || widget
                .children()
                .into_iter()
                .any(|child| has_label(child, name))
    }
    fn button(widget: WidgetRef<'_, dyn Widget>, name: &str) -> Option<WidgetId> {
        if widget.short_type_name() == "ControlWidget" && has_label(widget, name) {
            return Some(widget.id());
        }
        widget
            .children()
            .into_iter()
            .rev()
            .find_map(|child| button(child, name))
    }
    let mut app = workspace();
    app.select_review_mode(ReviewMode::Toggle);
    let before_advance = app.session.advance();
    std::sync::Arc::make_mut(&mut app.session).set_advance(before_advance + 80.0);
    let revision = app.font.project.document_revision();
    let mut ctx = context();
    let mut view = sized_box(proof_view(&app));
    let (pod, mut state) = view.build(&mut ctx, &mut app);
    let mut harness = TestHarness::create_with_size(
        stock::widget_properties(&app.palette),
        pod.new_widget,
        (800, 900),
    );
    assert!(button(harness.root_widget().as_dyn(), "Set Before").is_none());
    let before_id = button(harness.root_widget().as_dyn(), "Before").unwrap();
    let original_rect = harness
        .get_widget_with_id(before_id)
        .ctx()
        .window_transform()
        .transform_rect_bbox(harness.get_widget_with_id(before_id).ctx().border_box());
    let button_rect = |name| {
        let id = button(harness.root_widget().as_dyn(), name).unwrap();
        let widget = harness.get_widget_with_id(id);
        widget
            .ctx()
            .window_transform()
            .transform_rect_bbox(widget.ctx().border_box())
    };
    let working_rect = button_rect("Working");
    let toggle_rect = button_rect("Toggle");
    let waterfall_rect = button_rect("Waterfall");
    let text_rect = button_rect("Text");
    assert_eq!(original_rect.height(), toggle_rect.height());
    assert_eq!(original_rect.height(), waterfall_rect.height());
    assert_eq!(original_rect.y0, toggle_rect.y0);
    assert_eq!(
        working_rect.x0 - original_rect.x1,
        text_rect.x0 - waterfall_rect.x1
    );
    // Check the visible whitespace on either side of the revision row's rules.
    fn rules(widget: WidgetRef<'_, dyn Widget>, stroke: f64, found: &mut Vec<kurbo::Rect>) {
        let rect = widget
            .ctx()
            .window_transform()
            .transform_rect_bbox(widget.ctx().border_box());
        if widget.short_type_name() == "SizedBox"
            && (rect.height() - stroke).abs() < 0.01
            && rect.width() > 100.0
        {
            found.push(rect);
        }
        for child in widget.children() {
            rules(child, stroke, found);
        }
    }
    let mut dividers = Vec::new();
    rules(
        harness.root_widget().as_dyn(),
        app.palette.stroke_emphasis,
        &mut dividers,
    );
    let above = dividers
        .iter()
        .filter(|rect| rect.y1 <= original_rect.y0)
        .max_by(|a, b| a.y1.total_cmp(&b.y1))
        .unwrap();
    let below = dividers
        .iter()
        .filter(|rect| rect.y0 >= original_rect.y1)
        .min_by(|a, b| a.y0.total_cmp(&b.y0))
        .unwrap();
    let unit = crate::application::design::areas::proof::UNIT;
    assert_eq!(
        original_rect.y0 - above.y1,
        unit,
        "padding above revision row"
    );
    assert_eq!(
        below.y0 - original_rect.y1,
        unit,
        "padding below revision row"
    );
    assert_eq!(above.y0 - waterfall_rect.y1, unit, "padding below mode row");
    for (name, expected_before) in [
        ("Before", true),
        ("Working", false),
        ("Before", true),
        ("Toggle", false),
        ("Toggle", true),
        ("Toggle", false),
    ] {
        let id =
            button(harness.root_widget().as_dyn(), name).expect("revision control stays visible");
        while harness.pop_action_erased().is_some() {}
        harness.mouse_click_on(id, Some(masonry::core::PointerButton::Primary));
        let (action, source) = harness
            .pop_action::<ControlPress>()
            .expect("click emits action");
        let mut message = MessageCtx::new(
            Environment::new(),
            ctx.get_id_path(source).unwrap().clone(),
            DynMessage::new(action),
        );
        harness.edit_root_widget(|root| {
            assert!(matches!(
                view.message(&mut state, &mut message, root, &mut app),
                MessageResult::Action(())
            ));
        });
        assert_eq!(app.review.mode, ReviewMode::Toggle);
        assert_eq!(app.review.show_before, expected_before);
        assert_eq!(
            app.review_comparison().before.unwrap().advance,
            before_advance
        );
        let next = sized_box(proof_view(&app));
        harness.edit_root_widget(|root| next.rebuild(&view, &mut state, &mut ctx, root, &mut app));
        view = next;
        let id = button(harness.root_widget().as_dyn(), "Before").unwrap();
        let widget = harness.get_widget_with_id(id);
        assert_eq!(
            widget
                .ctx()
                .window_transform()
                .transform_rect_bbox(widget.ctx().border_box()),
            original_rect
        );
        assert!(button(harness.root_widget().as_dyn(), "Working").is_some());
        let id = button(harness.root_widget().as_dyn(), "Toggle").unwrap();
        let widget = harness.get_widget_with_id(id);
        assert_eq!(
            widget
                .ctx()
                .window_transform()
                .transform_rect_bbox(widget.ctx().border_box()),
            toggle_rect
        );
    }
    // Compare shows both revisions and has no controls that secretly switch modes.
    for mode in [ReviewMode::Compare, ReviewMode::Toggle, ReviewMode::Compare] {
        let name = if mode == ReviewMode::Compare {
            "Compare"
        } else {
            "Toggle"
        };
        let id = button(harness.root_widget().as_dyn(), name).unwrap();
        while harness.pop_action_erased().is_some() {}
        harness.mouse_click_on(id, Some(masonry::core::PointerButton::Primary));
        let (action, source) = harness
            .pop_action::<ControlPress>()
            .expect("mode click emits action");
        let mut message = MessageCtx::new(
            Environment::new(),
            ctx.get_id_path(source).unwrap().clone(),
            DynMessage::new(action),
        );
        harness.edit_root_widget(|root| {
            assert!(matches!(
                view.message(&mut state, &mut message, root, &mut app),
                MessageResult::Action(())
            ));
        });
        assert_eq!(app.review.mode, mode);
        let next = sized_box(proof_view(&app));
        harness.edit_root_widget(|root| next.rebuild(&view, &mut state, &mut ctx, root, &mut app));
        view = next;
        let root = harness.root_widget().as_dyn();
        assert_eq!(button(root, "Before").is_some(), mode == ReviewMode::Toggle);
        assert_eq!(
            button(root, "Working").is_some(),
            mode == ReviewMode::Toggle
        );
        let toggle = harness.get_widget_with_id(button(root, "Toggle").unwrap());
        let toggle_y = toggle
            .ctx()
            .window_transform()
            .transform_rect_bbox(toggle.ctx().border_box())
            .y0;
        assert_eq!(
            toggle_y,
            if mode == ReviewMode::Toggle {
                toggle_rect.y0
            } else {
                waterfall_rect.y0
            }
        );
    }
    assert_eq!(app.font.project.document_revision(), revision);
}
