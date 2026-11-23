// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! An ignored probe that prints frame timings for a point drag on a full font.

use super::*;

/// Time every step of a point drag on a real glyph and print the budget.
///
/// Run by hand with `cargo test --release frame_budget -- --ignored --nocapture`.
/// `RUNEBENDER_BENCH_GLYPH` picks the glyph, and `RUNEBENDER_SCREENSHOT=1` with
/// `RUNEBENDER_EDITOR_ZOOM=<zoom>` frames the canvas at a chosen zoom, as a screenshot does.
#[test]
#[ignore = "prints frame timings for a point drag on the Virtua Grotesk sources"]
fn frame_budget_report() {
    use crate::application::platform::screenshot::Headless;
    use crate::application::view::render::app_logic;
    use masonry::core::{PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate};
    use std::time::{Duration, Instant};
    use xilem::view::sized_box;

    fn stats(label: &str, samples: &mut [Duration]) {
        samples.sort();
        let micros = |d: Duration| d.as_secs_f64() * 1e6;
        let median = samples[samples.len() / 2];
        let worst = samples[samples.len() - 1];
        let count = u32::try_from(samples.len()).expect("a few hundred samples");
        let mean = samples.iter().sum::<Duration>() / count;
        println!(
            "{label:<28} median {:>9.1} µs   mean {:>9.1} µs   max {:>9.1} µs   (n={})",
            micros(median),
            micros(mean),
            micros(worst),
            samples.len()
        );
    }

    /// Print what each inspector section costs to build, so a slow one stands out.
    fn section_costs(app: &Workspace, editing: bool) {
        use crate::application::view::panels::{editor_info, sections};
        fn cost<V>(label: &str, build: impl FnOnce() -> V) {
            let start = Instant::now();
            let view = build();
            let elapsed = start.elapsed();
            drop(view);
            println!(
                "  section {label:<18} {:>8.1} µs",
                elapsed.as_secs_f64() * 1e6
            );
        }
        println!("inspector sections:");
        cost("layers", || sections::layers_section(app));
        cost("masters", || sections::masters_section(app));
        cost("mark", || sections::mark_section(app));
        if editing {
            cost("coordinates", || sections::coordinates_section(app));
            cost("transformations", || sections::transformations_section(app));
            cost("curves", || sections::curves_section(app));
            cost("path operations", || sections::path_operations_section(app));
            cost("background", || sections::background_section(app));
            cost("shaping", || sections::shaping_section(app));
            cost("related", || editor_info::related_section(app));
            cost("axes", || sections::axes_section(app, "Axes"));
            cost("measure", || sections::measure_section(app));
            cost("interpolation status", || app.interpolation_status());
            cost("preview request", || app.font.preview_font());
        } else {
            cost("font info", || sections::font_info_section(app));
            cost("dimensions", || editor_info::dimensions_section(app));
            cost("font advanced", || sections::font_advanced_section(app));
            cost("kerning", || editor_info::kerning_section(app));
            cost("groups", || editor_info::groups_section(app));
            cost("compare", || editor_info::compare_section(app));
            cost("features", || editor_info::features_section(app));
            cost("filtered cells", || app.filtered_cells());
        }
    }

    let glyph = std::env::var("RUNEBENDER_BENCH_GLYPH").unwrap_or_else(|_| "a".into());
    let sources = std::env::var_os("RUNEBENDER_TEST_FONTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../virtua-grotesk/sources")
        });
    // RUNEBENDER_BENCH_FONT names any designspace or UFO to probe in place of Virtua
    // Grotesk, such as a large font kept by the `large_font` benchmark.
    let path = std::env::var_os("RUNEBENDER_BENCH_FONT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| sources.join("VirtuaGrotesk.designspace"));
    let start = Instant::now();
    let mut app = Workspace::open(&path).expect("the font opens");
    println!(
        "open {} glyphs              {:.2} ms",
        app.font.glyphs.len(),
        start.elapsed().as_secs_f64() * 1e3
    );
    let index = app.font.index_of(&glyph).expect("the glyph exists");
    let logic = |app: &mut Workspace| sized_box(app_logic(app));
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    if std::env::var_os("RUNEBENDER_BENCH_OVERVIEW").is_some() {
        // The overview: the whole glyph grid, repainted when the pointer enters or
        // leaves it.
        let mut headless = Headless::new(app, logic, (1400, 900), 1.0);
        headless.rebuild(logic);
        headless.anim_frame(Duration::from_millis(500));
        let start = Instant::now();
        headless.redraw();
        let first_paint = start.elapsed();
        let grid_center = headless
            .find_widget::<crate::application::view::canvas::grid::GridWidget>()
            .map(|grid| grid.ctx().window_transform() * grid.ctx().border_box().center())
            .expect("the overview has a grid");
        let hover = |at: Point| {
            PointerEvent::Move(PointerUpdate {
                pointer: PRIMARY_MOUSE,
                current: PointerState {
                    position: PhysicalPosition::new(at.x, at.y),
                    ..PointerState::default()
                },
                coalesced: vec![],
                predicted: vec![],
            })
        };
        let mut paints = Vec::new();
        let mut clicks = Vec::new();
        for step in 0..20 {
            let at = if step % 2 == 0 {
                grid_center
            } else {
                Point::new(2.0, 2.0)
            };
            headless.pointer(hover(at));
            let start = Instant::now();
            headless.redraw();
            paints.push(start.elapsed());
        }
        for step in 0..10 {
            // Selecting a cell repaints the grid.
            let at = grid_center + (f64::from(step) * 40.0 - 200.0, 0.0);
            let state = PointerState {
                position: PhysicalPosition::new(at.x, at.y),
                ..PointerState::default()
            };
            headless.pointer(PointerEvent::Down(PointerButtonEvent {
                pointer: PRIMARY_MOUSE,
                button: Some(PointerButton::Primary),
                state: state.clone(),
            }));
            headless.pointer(PointerEvent::Up(PointerButtonEvent {
                pointer: PRIMARY_MOUSE,
                button: Some(PointerButton::Primary),
                state,
            }));
            let start = Instant::now();
            headless.redraw();
            clicks.push(start.elapsed());
        }
        let start = Instant::now();
        headless.rebuild(logic);
        let rebuild = start.elapsed();
        println!("overview: {} glyphs", headless.app.font.glyphs.len());
        let mut opens = Vec::new();
        for name in ["a", "B", "ampersand", "g", "at"] {
            let Some(index) = headless.app.font.index_of(name) else {
                continue;
            };
            let start = Instant::now();
            headless.app.open_glyph(index);
            let opened = start.elapsed();
            let start = Instant::now();
            headless.rebuild(logic);
            let rebuilt = start.elapsed();
            headless.redraw();
            println!(
                "  open {name:<10} session {:>7.2} ms   rebuild {:>7.2} ms",
                ms(opened),
                ms(rebuilt)
            );
            opens.push(opened + rebuilt);
            headless.app.back_to_overview();
            headless.rebuild(logic);
        }
        stats("open glyph + rebuild + paint", &mut opens);
        let masters = headless.app.font.master_count();
        if masters > 1 {
            let start = Instant::now();
            headless.app.set_master(1);
            let switched = start.elapsed();
            headless.rebuild(logic);
            let rebuilt = start.elapsed();
            headless.redraw();
            println!("switch master + rebuild      {:.2} ms", ms(start.elapsed()));
            println!(
                "  set_master {:.2} ms, rebuild {:.2} ms, paint {:.2} ms",
                ms(switched),
                ms(rebuilt - switched),
                ms(start.elapsed() - rebuilt)
            );
            let start = Instant::now();
            headless.app.font.rebuild_cache();
            let cache = start.elapsed();
            let start = Instant::now();
            let cells = crate::application::view::canvas::grid::cells_of(
                &headless.app.font,
                &headless.app.palette,
            );
            println!(
                "  of set_master: glyph cache {:.2} ms, cells {:.2} ms ({} cells)",
                ms(cache),
                ms(start.elapsed()),
                cells.len()
            );
            headless.app.set_master(0);
            headless.rebuild(logic);
            // RUNEBENDER_BENCH_REPEAT=<count> switches master that many times, so a
            // sampling profiler attached to this process sees the switch.
            if let Some(count) = std::env::var("RUNEBENDER_BENCH_REPEAT")
                .ok()
                .and_then(|count| count.parse::<usize>().ok())
            {
                let start = Instant::now();
                for step in 0..count {
                    headless.app.set_master((step + 1) % masters);
                    headless.rebuild(logic);
                    headless.redraw();
                }
                println!(
                    "repeated master switch       {:.2} ms each",
                    ms(start.elapsed()) / count as f64
                );
                headless.app.set_master(0);
                headless.rebuild(logic);
            }
        }
        section_costs(&headless.app, false);
        println!("overview first paint         {:.2} ms", ms(first_paint));
        stats("overview hover paint pass", &mut paints);
        stats("overview click paint pass", &mut clicks);
        println!("overview rebuild             {:.2} ms", ms(rebuild));
        return;
    }
    app.open_glyph(index);
    let mut headless = Headless::new(app, logic, (1400, 900), 1.0);
    headless.rebuild(logic);
    headless.anim_frame(Duration::from_millis(500));
    headless.redraw();

    let (origin, transform) = {
        let canvas = headless
            .find_widget::<EditorWidget>()
            .expect("the editor canvas is in the tree");
        let transform = canvas.ctx().window_transform();
        let visible = canvas.ctx().border_box().inset(-20.0);
        let widget = canvas.inner();
        let point = widget
            .screen_points()
            .into_iter()
            .find(|(_, at, on_curve, ..)| *on_curve && visible.contains(*at))
            .expect("an on-curve point is on screen");
        println!(
            "glyph {glyph}: {} points, zoom {:.2}",
            widget.session.points().len(),
            widget.session.viewport.zoom
        );
        (transform * point.1, transform)
    };
    let _ = transform;
    let state_at = |at: Point, down: bool| {
        let mut buttons = masonry::ui_events::pointer::PointerButtons::default();
        if down {
            buttons.insert(PointerButton::Primary);
        }
        PointerState {
            position: PhysicalPosition::new(at.x, at.y),
            buttons,
            ..PointerState::default()
        }
    };

    // No change: how long a rebuild and a paint take when nothing happened.
    let mut idle_rebuild = Vec::new();
    let mut idle_paint = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        headless.rebuild(logic);
        idle_rebuild.push(start.elapsed());
        let start = Instant::now();
        headless.redraw();
        idle_paint.push(start.elapsed());
    }

    let start = Instant::now();
    headless.pointer(PointerEvent::Move(PointerUpdate {
        pointer: PRIMARY_MOUSE,
        current: state_at(origin, false),
        coalesced: vec![],
        predicted: vec![],
    }));
    headless.pointer(PointerEvent::Down(PointerButtonEvent {
        pointer: PRIMARY_MOUSE,
        button: Some(PointerButton::Primary),
        state: state_at(origin, true),
    }));
    let press = start.elapsed();

    let moves = 240;
    let mut move_handling = Vec::with_capacity(moves);
    let mut move_paint = Vec::with_capacity(moves);
    for step in 1..=moves {
        let at = origin + (step as f64 * 0.5, (step as f64 * 0.25).sin() * 3.0);
        let start = Instant::now();
        headless.pointer(PointerEvent::Move(PointerUpdate {
            pointer: PRIMARY_MOUSE,
            current: state_at(at, true),
            coalesced: vec![],
            predicted: vec![],
        }));
        move_handling.push(start.elapsed());
        let start = Instant::now();
        headless.redraw();
        move_paint.push(start.elapsed());
    }

    let end = origin + (moves as f64 * 0.5, 0.0);
    let start = Instant::now();
    headless.pointer(PointerEvent::Up(PointerButtonEvent {
        pointer: PRIMARY_MOUSE,
        button: Some(PointerButton::Primary),
        state: state_at(end, false),
    }));
    let release = start.elapsed();

    // The window delivers the widget's actions to the app; do that by hand.
    let mut session = headless
        .find_widget::<EditorWidget>()
        .expect("the editor canvas is in the tree")
        .inner()
        .session
        .clone();
    let start = Instant::now();
    let outcome = headless.app.sync_session_from(&mut session);
    let sync = start.elapsed();
    assert_eq!(
        outcome,
        SessionSyncOutcome::Changed,
        "the drag moved the point"
    );
    let start = Instant::now();
    headless.app.finish_open_glyph_refresh();
    let refresh = start.elapsed();
    // Tests compile the preview on the calling thread when a rebuild asks for it; the
    // window compiles it on a worker. Compile it here, outside the timed rebuild, so the
    // rebuild figure is the work the window's interface thread does after an edit.
    let start = Instant::now();
    assert!(headless.app.font.project.compiled_preview().is_ok());
    let compile = start.elapsed();
    let start = Instant::now();
    headless.rebuild(logic);
    let rebuild = start.elapsed();
    let start = Instant::now();
    headless.redraw();
    let paint = start.elapsed();
    let start = Instant::now();
    headless.rebuild(logic);
    let settled = start.elapsed();
    let start = Instant::now();
    headless.app.undo_open_glyph(false);
    let undo = start.elapsed();
    assert!(headless.app.font.project.compiled_preview().is_ok());
    let start = Instant::now();
    headless.rebuild(logic);
    let undo_rebuild = start.elapsed();
    // RUNEBENDER_BENCH_REPEAT=<count> repeats redo and undo with their rebuilds, so a
    // sampling profiler attached to this process sees mostly the work after an edit.
    if let Some(count) = std::env::var("RUNEBENDER_BENCH_REPEAT")
        .ok()
        .and_then(|count| count.parse::<usize>().ok())
    {
        let start = Instant::now();
        for step in 0..count {
            headless.app.undo_open_glyph(step % 2 == 0);
            assert!(headless.app.font.project.compiled_preview().is_ok());
            headless.rebuild(logic);
        }
        println!(
            "repeated rebuild after edit  {:.2} ms each, compile included",
            ms(start.elapsed()) / count as f64
        );
    }
    let start = Instant::now();
    let snapshot = headless.app.font.project.babelfont_snapshot();
    let snapshot_time = start.elapsed();
    assert!(snapshot.is_ok());

    section_costs(&headless.app, true);
    if !headless.app.preview_text.is_empty() {
        use crate::application::editor::tools::text::{TextInputs, TextState};
        let app = &headless.app;
        let start = Instant::now();
        let inputs = TextInputs::new(&app.font);
        let new = start.elapsed();
        let start = Instant::now();
        let inputs = inputs.with_location(&app.font, &app.axis_values);
        let location = start.elapsed();
        let start = Instant::now();
        let placed = TextState::new(&inputs.with_text(&app.preview_text)).placed();
        let shaped = start.elapsed();
        println!();
        println!("proof text inputs            {:.2} ms", ms(new));
        println!("proof text location          {:.2} ms", ms(location));
        println!(
            "proof text shape and place   {:.2} ms ({} sorts)",
            ms(shaped),
            placed.len()
        );
        if let Ok(Some(compiled)) = app.font.preview_font() {
            let start = Instant::now();
            let font = runebender::text::shape::ShapingFont::from_bytes((*compiled.bytes).clone());
            let parsed = start.elapsed();
            assert!(font.is_ok());
            println!("proof shaping font parse     {:.2} ms", ms(parsed));
        }
    }

    // A drag of every point of the glyph: the edit path without the widget.
    {
        let mut session = headless
            .find_widget::<EditorWidget>()
            .expect("the editor canvas is in the tree")
            .inner()
            .session
            .clone();
        session.select_all();
        let selected = session.selection.len();
        session.begin_point_drag();
        let moves = 120;
        let start = Instant::now();
        for step in 1..=moves {
            session.drag_points_to((f64::from(step), f64::from(step % 7)), false);
        }
        let each = start.elapsed().as_secs_f64() * 1e6 / f64::from(moves);
        session.end_point_drag();
        println!();
        println!("select-all drag move         {each:>9.1} µs   ({selected} points)");
        let start = Instant::now();
        for _ in 0..20 {
            drop(session.clone());
        }
        println!(
            "session clone                {:>9.1} µs",
            start.elapsed().as_secs_f64() * 1e6 / 20.0
        );
    }

    // RUNEBENDER_BENCH_TEXT=<text> times the text line the canvas reads on each paint and
    // each pointer move while a composition is on the tab.
    if let Ok(text) = std::env::var("RUNEBENDER_BENCH_TEXT") {
        use crate::application::editor::tools::text::{TextInputs, TextState};
        let app = &headless.app;
        let inputs = TextInputs::new(&app.font)
            .with_location(&app.font, &app.axis_values)
            .with_text(&text);
        let state = TextState::new(&inputs);
        let time = |label: &str, work: &dyn Fn()| {
            let rounds = 200;
            let start = Instant::now();
            for _ in 0..rounds {
                work();
            }
            println!(
                "{label:<28} {:>9.1} µs",
                start.elapsed().as_secs_f64() * 1e6 / f64::from(rounds)
            );
        };
        println!();
        println!("text line, {} characters:", text.chars().count());
        time("text placed", &|| drop(state.placed()));
        time("text caret", &|| {
            std::hint::black_box(state.caret());
        });
        time("text active origin", &|| {
            std::hint::black_box(state.active_origin());
        });
        time("text kern hit", &|| {
            std::hint::black_box(state.kern_hit(Point::new(300.0, 300.0)));
        });
    }

    println!();
    stats("idle rebuild", &mut idle_rebuild);
    stats("idle paint pass", &mut idle_paint);
    stats("drag move handling", &mut move_handling);
    stats("drag move paint pass", &mut move_paint);
    println!("press                        {:.2} ms", ms(press));
    println!("release (widget)             {:.2} ms", ms(release));
    println!("release sync_session_from    {:.2} ms", ms(sync));
    println!("release glyph refresh        {:.2} ms", ms(refresh));
    println!("release rebuild              {:.2} ms", ms(rebuild));
    println!("release paint pass           {:.2} ms", ms(paint));
    println!("next rebuild                 {:.2} ms", ms(settled));
    println!("undo                         {:.2} ms", ms(undo));
    println!("undo rebuild                 {:.2} ms", ms(undo_rebuild));
    println!("font snapshot for preview    {:.2} ms", ms(snapshot_time));
    println!("preview compile (worker)     {:.2} ms", ms(compile));
}
