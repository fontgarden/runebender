# Quality review, 2026-11-23

This file is a temporary checklist from a full review of the repository at commit `0b11f6d`.
Work through one item at a time, and tick the item when its test passes.
Delete this file when the list is empty, or move the open items into [the cleanup plan](CLEANUP-PLAN.md).

## Method

The review applies the published positions of Raph Levien to the code.
A coding agent wrote it.
It is not a statement by Raph Levien, and he did not read the code.

Six reviewers each read one area: curve geometry, the Xilem architecture, widgets and painting, the font model, the text engine, and the platform and automation code.
The coordinator read the build, the dependencies, CI, the tests and the documents.
The coordinator then opened the cited lines of the top findings in each area.
No reviewer ran the code.

Each item has one of these labels:

- `[confirmed]`: the coordinator read the cited code and the claim holds.
- `[reported]`: one reviewer traced the code path, and the coordinator did not open it.
- `[test first]`: the mechanism is in the code, but the visible effect depends on runtime behavior.
  Write the failing test before the fix.

Line numbers refer to commit `0b11f6d`.
Each item also names the function, because lines move.

[The audit of 2026-11-09](AUDIT-2026-11-09.md) and [the cleanup plan](CLEANUP-PLAN.md) hold earlier findings.
This file does not repeat them, except where a reviewer found that an item is still open and it matters to an item here.
The owner decisions of 2026-11-17 in the cleanup plan stay in force.
"Decisions for the owner" at the end of this file lists the points where this review disagrees with one of them.

## Positions applied

These are the positions the review used, each with the post that states it.
The posts are at <https://raphlinus.github.io/>.

- The view is a function of the state.
  Views hold no shared mutable state.
  Memoization on values with cheap equality skips unchanged subtrees. ("Xilem: an architecture for UI in Rust")
- A change notification says only that something changed, in the manner of a future and its waker.
  Timers that poll are not notifications. ("Towards principled reactive UI")
- Asynchronous designs in an interactive program add races and complexity.
  Data in one process with one owner is simpler. ("xi-editor retrospective")
- Text input is a test of a toolkit: input methods, clipboard and accessibility belong to it.
  Accessibility needs a retained tree with stable identities. ("Advice for the next dozen Rust GUIs")
- Text layout is a hierarchy: paragraph, direction runs, script runs, shaped clusters.
  A caret inside a cluster needs an explicit rule. ("Text layout is a loose hierarchy of segmentation")
- Frame latency is measured, not assumed.
  A frame that changes nothing costs power. ("Swapchains and frame pacing", "The compositor is evil")
- Compile time and binary size are performance.
  Duplicate versions of a crate are a cost, and shared dependencies do not amortize it. ("Thoughts on Rust bloat")
- A curve operation preserves what the designer drew: endpoints, tangents and area.
  A fit of a cubic returns the same cubic. ("Fitting cubic Bézier curves", "Parallel curves of cubic Béziers", "Simplifying Bézier paths")
- One focused application drives the toolkit, and its needs go upstream. ("Rust 2020: GUI and community", "Rust 2021: GUI")

## Main conclusions

1. **CI is red.**
   No push to `main` passed CI after 2026-10-30.
   Five jobs fail, each for a small reason.
   Phase 0 corrects them.
2. **A designer can lose work in five ways** that need no crash of the editor.
   Remove Glyph has no undo.
   Save writes over outside changes.
   The source watcher does not follow a newly opened font.
   Quit from the Dock shows no prompt.
   A stop between two renames leaves a hidden font folder.
   Phase 1 corrects them.
3. **The curve code uses kurbo for hit tests and bounds, and writes its own math for the rest.**
   The copies are mostly correct.
   The defects are in the parts kurbo does not have: knife pairing, embolden, balance on an S curve, and one solver guard that kurbo also lacks.
4. **The view is not a function of the state.**
   View functions start compiles and threads, read the file system, and return a new `Arc` on each call.
   No `memoize` boundary exists.
   Background results arrive through 13 timers, although one worker already pushes through a `MessageProxy`.
5. **The Text tool renders the whole window at the display rate while idle**, because the caret blink requests a frame on each frame.
6. **The text engine takes bidi and shaping from libraries, and writes itemization, direction detection, cluster mapping, hit tests and caret movement by hand.**
   Each hand-written layer has a defect with a concrete input.
7. **The document model stores a layer in two parallel structures and copies it for undo.**
   A point drag is cheap.
   Whole-font operations and kerning edits are not.
8. **About 28% of production code is outside the draw, space and save loop.**
   The owner decided on 2026-11-17 to keep these subsystems in the default build.
   The review records the cost and does not reopen the decision.

## Measured baseline

Record these in [the benchmark guide](BENCHMARKS.md) when item 9.3 is done.
The machine is an Apple M4 Pro with 12 cores.

| Measure | Value |
| --- | --- |
| Rust lines | 210,816 in 399 files |
| Production lines in `src` | 144,659 |
| Test lines in `src` and `tests` | 77,025 |
| Tests | 1,611, of which 10 are ignored |
| Crates, default build | 530 |
| Crates, engine only (`--no-default-features`) | 301 |
| `kurbo` versions | 3 (0.9.5, 0.11.3, 0.13.1) |
| `read-fonts` versions | 3 (0.36, 0.37, 0.43) |
| Clean development build | 3 min 13 s, 1,661 s of processor time |
| Rebuild after a one-function edit in a view file | 2.1 s |
| Rebuild after a one-function edit in an engine file | 4.6 s |
| Binary, `performance` profile | 35.5 MB |
| Rendered snapshot tests | 0 |
| Property or fuzz tests | 0 |

The rebuild times are short.
Build time gives no reason to split the package into crates.

## Phase 0: make CI pass

Run 45284168469 on commit `0b11f6d` fails in five jobs.
Do these first, in one session.

- [x] **0.1 Linux dead code.** `[confirmed]`
  Where: `action_for_key`, `src/application/actions.rs:462`.
  Problem: only the macOS shortcut host calls it, so Linux builds fail with `-D warnings`.
  Change: compile it on macOS and in tests only, as the shortcut widget is.
  Confirm: `cargo clippy --workspace --lib --bins --locked -- -D warnings` in the Linux CI job.
- [x] **0.2 Two documentation links to private items.** `[confirmed]`
  Where: `prune_stale`, `src/automation/live_socket.rs:73` (`PRUNE_MIN_AGE`), and `grown`, `src/outline/piece_assembly.rs:40` (`OVERLAP`).
  Change: write the constant name as code, not as a link.
  Confirm: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`.
- [x] **0.3 Four spelling errors.** `[confirmed]`
  Where: `docs/upstream/linebender/wgpu-metal-premultiplied-alpha.md:33` and `:42` quote upstream titles that contain a misspelled word, and `src/application/view/canvas/editor/tests/text.rs:327` has an Arabic letter name in a test name.
  Change: add the three words to the `typos` configuration, because the first two are exact quotes and the third is a letter name.
  Confirm: `typos`.
- [x] **0.4 Browser wheel zoom check.** `[test first]`
  Where: `web/quality.cjs:111`, "wheel zoom changes painted outline size".
  Problem: the check fails from commit `697a9f3` on.
  Either wheel zoom no longer repaints in the browser, or the check measures the wrong thing.
  Change: build the browser editor with the recipe in `web/README.md`, reproduce, and correct the cause.
  Confirm: the `browser editor` CI job.
- [x] **0.5 Run the CI checks before a commit.** `[confirmed]`
  Problem: the local set was `fmt`, two Clippy commands and the tests, on macOS.
  It does not run `cargo doc`, `typos` or a Linux build, so items 0.1 to 0.3 reached `main`.
  Change: add `cargo doc` with `-D warnings` and `typos` to the local set.
  For platform-specific code, check the Linux job after the push.
- [x] **0.6 Cache step warning.** `[confirmed]`
  Where: the `Swatinem/rust-cache` step in `.github/workflows/ci.yml`.
  Problem: it reads the `web` workspace before `web/prepare.py` runs and logs "failed to load manifest".
  The job continues without a cache for `web`.
  Change: cache the `web` workspace only in the browser job, after the prepare step.

## Phase 1: data safety

Each item is a way to lose work or to write a wrong value into a font.

- [ ] **1.1 Remove Glyph, Add Glyph and Duplicate Glyph cannot be undone.** `[confirmed]`
  Where: `Workspace::command_remove_glyph`, `src/application/editor/commands.rs:217`, and `Project::commit_glyph_transaction`, `src/font/project/glyph_transactions.rs:207`.
  Problem: the command records no history entry and asks for no confirmation.
  The next Save deletes the glyph files in every master.
  Change: the transaction already holds the structure before and after.
  Record both in `source_history`, and add a variant to `MetadataEdit`.
  Confirm: a test removes a glyph, calls Undo, and compares the project with the original.
- [ ] **1.2 Save writes over sources that another program changed.** `[confirmed]`
  Where: `Workspace::save`, `src/application/platform/host.rs:429`.
  Problem: the fingerprint detects the change, the save continues, and the note says so afterward.
  A dirty master is replaced and a clean master keeps the outside change, so the masters can disagree.
  Change: when the fingerprint differs, do not save.
  Offer Revert, Save As, or an explicit overwrite.
  Keep one fingerprint for each source.
  Confirm: a test changes a source file on disk, calls Save, and reads the file unchanged.
- [x] **1.3 The source watcher keeps the paths of the first document.** `[test first]`
  Where: `with_watch`, `src/application/platform/watch.rs:24`, called from `app_logic`, `src/application/view/render.rs:456`.
  Problem: `task_raw` starts its future once, and `Task::rebuild` in the Xilem fork does nothing.
  A font opened with File > Open is not watched, so item 1.2 has no warning before it.
  Change: let the document own the watcher and drop it with the workspace.
  Watch `source_roots`, which include the designspace file.
  Show a note when `watch` fails.
  Confirm: a test opens a second font, changes one of its files, and receives a reload.
- [ ] **1.4 The macOS terminate path skips the prompt and all cleanup.** `[test first]`
  Where: `TimedEventLoop::exiting`, `src/application/launch.rs:266`.
  Problem: Dock > Quit, logout and an update use `-[NSApplication terminate:]`.
  The process exits with no unsaved-changes prompt, and `llama-server` and the socket folder stay.
  Change: call a new `AppState::shutdown()` from `exiting`.
  It stops children and removes the socket.
  The prompt needs an `applicationShouldTerminate:` method, which winit owns, so record it as an upstream need or write an edit journal.
  Confirm: by hand, with the agreement of the owner, because a headless run cannot show it.
- [ ] **1.5 The publish step of a save has no journal.** `[confirmed]`
  Where: `publish`, `src/font/persistence/mod.rs:626`.
  Problem: each artifact is two renames.
  A stop between them leaves `Font.ufo` absent and the font in a hidden backup folder.
  Nothing looks for that folder at the next open.
  Change: write a journal file before the first rename and remove it after the last.
  At open, complete or reverse a journal that remains.
  Put the backup path in each error message.
  Confirm: a test stops after the first rename and opens the family again.
- [ ] **1.6 After Save As, the `data` and `images` stores read from the old UFO.** `[reported]`
  Where: `SourceFormatData`, `src/font/persistence/source_format.rs:19`, and `Project::save_as`, `src/font/project/save_as.rs:52`.
  Problem: the stores stay bound to the first path.
  If the first UFO moves, each later save fails.
  Change: load every store item into memory at import.
  Confirm: a test does Save As, deletes the first UFO, and saves again.
- [x] **1.7 Line breaks and named glyphs are lost when the text buffer is built again.** `[confirmed]`
  Where: `TextState::new`, `src/application/editor/tools/text.rs:560`, and `TextBuffer::insert_characters`, `src/text/buffer/mod.rs:926`.
  Problem: the text is stored as a `String`, and the restore path skips each character the font does not map. `\n` is such a character.
  Change: add `TextBuffer::insert_str` that maps `\n` to a line break and shapes one time.
  Use it in `TextState::new` and in `commit_preedit`.
  Confirm: a test types `A`, Enter, `B`, changes tabs twice, and counts two lines.
- [x] **1.8 Side bearings ignore components.** `[test first]`
  Where: `side_bearings`, `src/analysis/measure.rs:524`, called from `Session::side_bearings`, `src/application/editor/session.rs:1986`.
  Problem: the outline comes from contours only.
  For a glyph made of components, the card shows the glyph as blank, and a typed right bearing sets the advance to that number.
  Change: pass the rendered outline, with components, to `side_bearings_from_points`.
  Confirm: a test with a composite glyph reads the same bearings as the `read_glyph` tool.
- [x] **1.9 A configuration error is discarded with no message.** `[confirmed]`
  Where: `load`, `src/application/platform/config.rs:219`.
  Problem: one unknown key makes the whole file equal to no file, and nothing reports it.
  Change: return the error with the defaults, and show it in the first note.

## Phase 2: defects with a small fix

- [x] **2.1 A context menu can outlive its canvas.** `[confirmed]`
  Where: `open_context_menu`, `src/application/view/canvas/editor/menu.rs:145`, and `open_add_menu`, `src/application/view/canvas/graph.rs:502`.
  Problem: the menu uses `create_layer` and removes itself through its creator.
  If Escape removes the canvas first, the menu stays and no click closes it.
  Change: use `create_attached_layer`, which Masonry removes with the widget.
  Confirm: a harness test opens the menu, sends Escape, and counts zero layers.
- [x] **2.2 The graph canvas lays out children it stashed.** `[confirmed]`
  Where: `GraphWidget::layout`, `src/application/view/canvas/graph.rs:756` and `:798`.
  Problem: the Masonry contract says a parent that stashes a child in layout must not call `run_layout` on it.
  A debug build panics when a proof image is expanded.
  Change: skip `run_layout` and `place_child` while `stashed` is true.
- [ ] **2.3 A popup layer is removed twice.** `[reported]`
  Where: `close`, `src/application/widgets/menu_shell.rs:412`, and `toggle_menu`, `src/application/widgets/tool_group.rs:248`.
  Change: the popup asks, and only the owner removes the layer and clears its identifier first.
  Item 5.11 makes this one widget.
- [ ] **2.4 Balance reverses a handle on an S curve, and Harmonize retracts a handle to zero.** `[confirmed]`
  Status 2026-11-23: `balance` is done. `harmonize` is open.
  Where: `balance`, `src/analysis/curve.rs:463`, and `harmonize`, `:437`.
  Problem: `balance` intersects the two handle lines and does not test that the intersection is in front of both handles.
  The comment says it returns `None` at inflections, and it does not.
  Input: `balance((0,0), (30,40), (60,-40), (100,0))` returns the first handle at (-30,-40).
  Change: return `None` unless both ray parameters are positive.
  In `harmonize`, use signed distances, as `maintain_g2_at` does, and share that function.
  Confirm: tests for an S curve and for a straight cubic that joins a curve.
- [x] **2.5 The tangent of a retracted handle uses the chord.** `[confirmed]`
  Where: `tangent_end` and `tangent_start`, `src/analysis/curve.rs:211`.
  Problem: when `p2 == p3`, the end tangent is `p3 - p1`.
  The code uses `p3 - p0`, so a correct smooth join is reported as a kink.
  Change: fall back in the order `p3 - p2`, `p3 - p1`, `p3 - p0`, as `PathSeg::tangents` in kurbo does.
- [ ] **2.6 The Union button and the Union menu command run different code.** `[confirmed]`
  Status 2026-11-23: not a defect. The panel icon named "union" is the Remove Overlap button, and both commands are in the Path menu. The route through `dispatch` is item 4.8.
  Where: `src/application/view/panels/sections.rs:426` calls `remove_overlap()`, and `src/application/editor/commands.rs:1084` calls `boolean(BoolOp::Union)`.
  Change: panel buttons call `app.dispatch(AppAction::…)`.
  Item 4.8 covers the other buttons.
- [ ] **2.7 Scroll zoom speed doubles on a 2x display, and pinch is ignored.** `[confirmed]`
  Status 2026-11-23: the scale factor, pinch on the edit canvas and the Space key repeat are done. Pinch on the graph canvas is open.
  Where: `src/application/view/canvas/editor/pointer.rs:291` and `src/application/view/canvas/graph.rs:1410`.
  Problem: the grid divides pixel deltas by the scale factor (`grid.rs:1074`), and the two canvases do not.
  `PointerEvent::Gesture` has no arm.
  Change: divide by `ctx.scale_factor()`, send pinch to `zoom_about`, and ignore key repeat for Space in `ShortcutHost`.
- [ ] **2.8 Command-Z in the Text tool reverts an outline edit.** `[confirmed]`
  Status 2026-11-23: not done. Kern drags in the Text tool use the same Undo path, so the key cannot be consumed before the text history of item 6.8 exists.
  Where: `handle_text_event`, `src/application/view/canvas/editor/keyboard.rs:135`.
  Problem: the text has no undo, so the key reaches the general arm.
  Change: in the Text tool, consume the key.
  Item 6.8 adds the text history.
- [ ] **2.9 The preview hold flag can stay set after a right-click.** `[test first]`
  Where: `on_pointer_event`, `src/application/view/canvas/editor/mod.rs:435`.
  Change: set the hold only in the branches that capture the pointer.
- [x] **2.10 The live socket wake flag can lose a request.** `[confirmed]`
  Where: `Server::try_recv`, `src/automation/live_socket.rs:302`.
  Problem: the flag is cleared after the queue reads empty.
  A request that arrives between the two steps waits for the 30 s timeout.
  Change: clear the flag before the drain.
  Item 4.5 removes the flag.
- [ ] **2.11 Quit does not stop child processes.** `[confirmed]` for `TrainState`, `[reported]` for the rest.
  Status 2026-11-23: `Drop for TrainState` is done. Process groups and the other job types are open.
  Where: `TrainState`, `src/application/platform/train.rs:72`, has no `Drop`.
  Five other job types set a flag in `Drop` and do not wait.
  Problem: a training run with a 24 hour deadline continues after the document closes.
  Change: add `Drop for TrainState`.
  Start children with `process_group(0)` and stop the group.
  Item 8.1 gives all jobs one registry.
- [ ] **2.12 A worker panic leaves its job busy forever.** `[reported]`
  Where: the ten detached job threads listed in item 8.1, and the trace worker in `src/application/editor/tools/graph/trace.rs:91`.
  Change: one `spawn_job` helper with a guard that writes an error to the result slot on unwind.
- [ ] **2.13 The `propose` command runs `font-ml` with no deadline.** `[reported]`
  Where: `propose`, `src/application/cli/font_commands/proposals.rs:250`.
  Change: call `process::run` with `ProcessLimits`.
- [ ] **2.14 A `llama-server` start cannot be cancelled.** `[reported]`
  Where: `LlamaServer::start_on`, `src/workflows/llama_server.rs:352`.
  Change: pass the cancellation to the ready loop.
  Stop the server after an idle time, or correct `docs/LOCAL-AI.md`, which says it stops when the panel closes.

## Phase 3: curve geometry

- [ ] **3.1 One line intersection helper with a relative degree test.** `[test first]`
  Where: `solve_cubic`, `src/outline/knife.rs:656`, and the `PathSeg::intersect_line` calls in `src/analysis/measure.rs:276`, `:288`, `:386` and `src/analysis/thickness.rs:283`.
  Problem: a cubic that is an exact elevation of a quadratic has a leading coefficient near 1e-10.
  The knife tests it against the absolute value 1e-9.
  `kurbo::common::solve_cubic` uses the quadratic solver only when the division overflows.
  Input: `QuadBez::new((148.,213.),(1092.,492.),(1040.,791.))`, raised to a cubic, against `Line::new((1217.,-353.),(243.,843.))`.
  The crossing is at t = 0.29631.
  Change: one `intersect_line(seg, line)` in `src/outline`.
  For a cubic, call `solve_quadratic` when `|c3| <= 1e-9 * max(|c0|, |c1|, |c2|)`.
  Confirm: the input as a Rust test, for the helper and for kurbo.
  If kurbo fails it, add an upstream item.
  Note: the counts in the review came from a Python port of both solvers, not from a Rust run.
- [ ] **3.2 One boolean function.** `[reported]`
  Where: seven calls of `linesweeper::binary_op`, in `src/font/babelfont/edit_effects.rs:600`, `:645`, `:669`, `src/outline/effects.rs:70`, `:105` and `src/outline/piece_assembly.rs:96`.
  Problem: one open contour makes the whole operation fail, and the caller maps the error to "no change".
  A successful run on a glyph with no overlap replaces every contour and its identities.
  `linesweeper` can panic, and the interface thread has no guard.
  Change: one `boolean(a, b, op) -> Result<Vec<BezPath>, BooleanError>`.
  Carry open contours through.
  Return `Ok(false)` when the result equals the input.
  Wrap the call in `catch_unwind`.
  Show the error.
  Confirm: tests for an open contour, for a glyph with no overlap, and for the direction of the result.
- [ ] **3.3 The knife pairs crossings by index, not by ink.** `[reported]`
  Where: `slice_path_impl`, `src/outline/knife.rs:217`.
  Input: a U shape cut by a line that starts inside the left stem.
  The cut runs through the counter, and the right stem is not cut.
  Change: for each pair of consecutive hits, find the winding at the midpoint with `Shape::winding`, and cut only the ink intervals.
- [ ] **3.4 Embolden moves each point along a chord normal.** `[confirmed]`
  Where: `outward_normals_for_points`, `src/outline/embolden.rs:51`.
  Problem: the two ends of one straight edge move by different amounts, so vertical and horizontal edges rotate.
  A square of 100 units with offset 10 becomes 114.1 wide, not 120.
  Change: move each leg of the control polygon along its own normal, and put each point at the intersection of its two moved legs, with a miter limit.
  The point count stays the same.
  Confirm: a test that an axis-aligned rectangle stays an axis-aligned rectangle.
  Compare `score_offset` on the fixture masters before and after.
- [x] **3.5 Offset uses round joins.** `[confirmed]`
  Where: `offset_paths`, `src/outline/effects.rs:76`.
  `Stroke::new` sets `Join::Round`.
  Change: `with_join(Join::Miter)` with a miter limit, and make the join a parameter.
  Assert the corner coordinates in the test.
- [ ] **3.6 One rounding step.** `[reported]`
  Where: `replacement_contour_from_path`, `src/font/babelfont/edit_replacements.rs:139`.
  Problem: three policies exist.
  Drag snaps to the grid.
  Offset and Harmonize round to integers.
  Remove Overlap, the knife and embolden store raw fractions.
  Change: one `quantize(path, grid)` at the replacement functions, with the grid of the draft.
- [ ] **3.7 Replace copies of kurbo functions.** `[reported]`
  Where: `Cubic` in `src/analysis/curve.rs:145`, `cubics_from_path` at `:66`, three line intersections with three tolerances, and `subdivide_cubic` in `src/outline/path/segment.rs:72`.
  Change: use `CubicBez` with `ParamCurveDeriv` and `ParamCurveCurvature`, `Line::crossing_point`, `ParamCurve::subsegment` and `Line::nearest`.
  About 250 lines go.
  Note: the curvature sign in kurbo is opposite to the sign in `curve.rs`.
- [ ] **3.8 One segment view of a contour.** `[reported]`
  Problem: eight walkers turn a point list into segments.
  Two of them disagree on a closed contour with one on-curve point, and `to_contour` moves the start point by one (`src/outline/path/bezier.rs:98`).
  Change: one type that yields `kurbo::PathSeg`, the point identities and the smooth flag, for bezier and hyper contours.
  This is items A17 and O5 of the cleanup plan.
  Confirm: first add a test that `to_bezpath().segments()` and `iter_segments()` agree.
- [ ] **3.9 Analysis on the curve, per arc length.** `[reported]`
  Where: `joining_band`, `src/analysis/measure.rs:641`, tests control points as ink.
  `classify`, `src/analysis/curve.rs:329`, compares dκ/dt.
  Change: use segments in `joining_band`.
  Divide dκ/dt by the speed for the G3 test.
- [ ] **3.10 Property tests for geometry.** `[confirmed]`
  Problem: no test in `src/outline` or `src/analysis` uses random input, and some tests learn the expected value from the function under test.
  Change: use the generator in `src/outline/effects.rs:193`, so no dependency is added.
  Properties: the knife keeps the total area.
  Remove Overlap keeps the area, and a second run changes nothing.
  A quadratic and its raised cubic give the same crossings.
  Harmonize gives G2.
  Balance keeps handle directions.
  A circle of radius r has curvature 1/r.
- [ ] **3.11 Earlier items still open in this area.** `[reported]`
  Hyper corner points render smooth (`src/outline/path/hyper.rs:179`).
  `to_cubic` duplicates the start point (`:101`).
  `lighter_master` selects the lightest master (`src/analysis/weight_debt.rs:152`).
  Extrusion omits the area outside each chord (`src/outline/effects.rs:123`).
  Roughen closes open contours (`:222`).
  `realign_component_offsets` ignores the transform (`src/font/composites.rs:96`).

## Phase 4: reactive architecture

- [x] **4.1 The Text tool renders the window on every display refresh while idle.** `[confirmed]`
  Where: `EditorWidget::on_anim_frame`, `src/application/view/canvas/editor/mod.rs:483`.
  Problem: each frame requests the next one, to change a flag each 500 ms.
  An animation frame always causes a full redraw in Masonry.
  Change: blink only while the window and the widget have focus, and stop after 10 s, as the upstream `TextArea` does.
  Start again on input.
  Confirm: the frame time recorder shows no frames after the timeout.
- [ ] **4.2 Values that views compare by pointer must be stable.** `[confirmed]`
  Status 2026-11-23: `filtered_cells` and `interp_preview` are done, each with an idle test. `interpolation_canvas` is open.
  Where: `interp_preview`, `src/application/editor/session.rs:3583`.
  `filtered_cells`, `src/application/editor/sidebar.rs:18`.
  `interpolation_canvas`, `src/application/editor/tools/interpolation.rs:150`.
  Problem: each call returns `Arc::new(..)`, so `Arc::ptr_eq` in the canvas always fails.
  Away from a master, each rebuild interpolates twice and repaints.
  With a category selected, each rebuild repaints the grid.
  Change: compute each value in the command that changes its inputs, and store the `Arc` in the state.
  Confirm: add three cases to the idle rebuild test in `render.rs`: sliders away from a master, a category selected, and the Interpolation tool active.
- [ ] **4.3 The proof strip shapes its text on each rebuild.** `[reported]`
  Where: `outline_strip`, `src/application/view/panels/preview.rs:111`.
  Problem: the key of `keyed_canvas` prevents the paint, and the work that makes the key runs each time.
  Change: `memoize` on identities: revision, text, location, direction, features, palette.
  Build the paths inside the closure.
- [ ] **4.4 Menu state reads the file system on each rebuild.** `[reported]`
  Where: `Entry::enabled`, `src/application/actions.rs:145`, through `FontModel::is_writable`.
  Change: set a `writable` field at open and at save.
  Compare the menu state before a call to AppKit.
- [ ] **4.5 Push background results.
      Do not poll.** `[confirmed]`
  Where: `poll` and its callers, `src/application/view/render.rs:529` to `:765`, and `with_live`, `src/application/platform/live.rs:57`.
  Problem: the comment at `render.rs:527` says Xilem has no hook to post from a thread.
  The thickness worker already posts through a `MessageProxy` (`thickness.rs:227`).
  Seven timers rebuild the whole tree on each tick while a job runs, and `with_live` wakes 25 times a second for the life of the window.
  Change: first, the socket handler wakes the interface when it queues a request.
  Then one `Job<T>` that owns a `MessageProxy`.
  Attach all tasks with one `fork(content, (..))`, which removes the 13 nested wrappers.
- [ ] **4.6 A view function has no side effects.** `[confirmed]`
  Where: `request_preview`, `src/font/compiler/mod.rs:645`, reached from `status` (`chrome.rs:329`), `axes_section` (`sections.rs:284`) and `outline_strip`.
  Also `review_reading_fonts` and `refresh_model`.
  Problem: a call from a view copies the whole font and starts a compile thread.
  `app_logic` also writes two fields of the state (`render.rs:336`, `:441`).
  Change: the command that commits an edit marks the preview as wanted.
  One `worker` view owns the compile queue.
  Views read `Option<Arc<CompiledFont>>` from the state.
- [ ] **4.7 First memoization boundaries.** `[confirmed]`
  Problem: `memoize` has 0 uses.
  About 110 view functions take `&Workspace`, so none can be memoized.
  The idle rebuild is 0.28 ms at 863 glyphs and 1.37 ms at 16,000.
  Change: give `header_tools`, `marks_bar`, `rail_tabs` and `category_sidebar` each a small input struct with `PartialEq`, and wrap each in `memoize`.
- [ ] **4.8 One path for one command.** `[confirmed]`
  Problem: panel buttons call session functions and skip `dispatch`, so they skip the `enabled` rule and the action record.
  Rotate is written twice.
  The canvas handles Select All, Undo and Delete itself.
  Change: buttons call `app.dispatch(action)`.
  Add `EditorEvent::Command(AppAction)` for canvas keys.
- [ ] **4.9 Put the panel rule in a type.** `[reported]`
  Problem: `tests/panel_vocabulary.rs` counts text matches and does not count the aliases `xrow` and `xcolumn`.
  `Palette::role(&str)` panics on an unknown name at 75 call sites.
  The scan in `tests/design_system.rs:179` stops at the first `#[cfg(test)]` line.
  Change: each section function returns a `Section` value of `Row` groups, and one function in `design::recipes` builds the view.
  Replace `role(&str)` with an enum.
  Keep `tests/module_layering.rs`, which parses with `syn`.
- [ ] **4.10 Small work that each rebuild repeats.** `[reported]`
  `document_readout` calls `std::path::absolute` (`chrome.rs:229`).
  `editor_pane` scans all glyphs by name (`panels/editor.rs:48`), where `FontModel::index_of` is a map lookup.
  `short_master_names` runs three times.
  `launch.rs:83` calls AppKit on each rebuild.
  Folded sections still build their bodies (item I2 of the cleanup plan).
- [ ] **4.11 Split the state.** `[confirmed]` Large.
  Problem: `Workspace` has 131 fields.
  The session exists three times: in `Workspace`, in the tab, and in the widget.
  Each canvas action copies it twice.
  Change: this is items E4 and E5 of the cleanup plan.
  Group the fields into document, overview, editor, inspector drafts, proof, and tools.
  Do it after 4.2 to 4.8, which show where the boundaries are.

## Phase 5: painting and input

- [ ] **5.1 Derived frame data for the edit canvas.** `[reported]`
  Where: `paint_outline`, `paint_points`, `comb_samples` and `paint_metrics` in `src/application/view/canvas/editor/`.
  Problem: a pan, a zoom and a hover each derive the outline, the screen points and the side bearings from the layer again.
  `hit_point` rebuilds the screen points up to five times in one press.
  Change: one value owned by the widget, rebuilt when a revision counter of the session changes, and used by paint and by hit tests.
  Draw fills with `.transform(affine)`.
- [ ] **5.2 Cache the text line layout.** `[reported]`
  Where: `TextState::active_origin`, `src/application/editor/tools/text.rs:743`, called from about 35 sites through `glyph_affine`.
  Change: add a revision counter to `TextBuffer`, and cache `TextLayout` for that revision.
- [ ] **5.3 The graph canvas rebuilds its dot grid on each paint.** `[reported]`
  Where: `GraphWidget::paint`, `src/application/view/canvas/graph.rs:837`.
  About 4,000 circle paths at zoom 1.
  Change: reuse the cached dot field of the editor (`editor/marks.rs:25`).
- [ ] **5.4 The glyph grid pushes two clip layers for each cell.** `[reported]`
  Where: `GridWidget::paint`, `src/application/view/canvas/grid.rs:937`.
  Change: store the ink bounds in `Cell`, and push a clip only when the content leaves its rectangle.
- [ ] **5.5 Snap one-pixel rules to device pixels.** `[test first]`
  Where: `horizontal_rule_rect`, `src/application/view/canvas/editor/marks.rs:81`, and `cell_width`, `grid.rs:515`.
  Change: one helper that rounds a rule position with `ctx.scale_factor()`.
  Confirm: a headless capture at a fractional pan offset.
- [ ] **5.6 The label cache empties itself on large fonts.** `[test first]`
  Where: `layout`, `src/application/widgets/text_label.rs:64`, holds 4,096 entries and clears all of them when full.
  Change: keep the three caption widths with the glyph entry.
  Replace clear-all with least-recently-used eviction.
- [ ] **5.7 The chat list keeps every hidden row in every frame.** `[reported]`
  Where: `TailListWidget::layout`, `src/application/widgets/tail_list.rs:192`, places hidden rows at y = -1e6.
  Change: `ctx.set_stashed(row, true)` for rows outside the view.
- [ ] **5.8 The code editor shapes every visible line on each paint.** `[reported]`
  Where: `CodeEditor::paint`, `src/application/widgets/code_editor.rs:474`.
  Change: keep line layouts by text and size, and use `PaintCtx::text_contexts()`.
- [ ] **5.9 Input methods on the canvas.** `[confirmed]` for the missing call, `[test first]` for the platform effects.
  Problem: no code calls `set_ime_area`, so the candidate window is placed for the whole canvas.
  `typing_key` applies keys while a composition is open.
  Both canvases accept text input in every tool, and the graph canvas handles no input method event.
  A metric field cannot receive pasted text.
  Change: set the area to the caret rectangle.
  Ignore keys while `preedit` is not empty.
  Send paste and commit to the active field first.
  Return `false` from `GraphWidget::accepts_text_input`.
- [ ] **5.10 Minimal accessibility nodes.** `[reported]`
  Change: editor: label "Glyph editor", value with glyph, point count, selection count and tool.
  Grid: the selected name and its position.
  Graph: node count and selected title.
  Popups: the row pattern that `MenuPopup` has.
- [ ] **5.11 One popup list widget.** `[reported]`
  Problem: `menu_shell.rs`, `tool_group.rs` and `context_menu.rs` repeat row layout, hover and dismissal.
  Change: one widget on `create_attached_layer`, with focus, arrow and Escape keys, accessible rows and a window-edge clamp.
  It closes items 2.1 and 2.3.
- [ ] **5.12 Canvas fields as child widgets.** `[reported]` Large.
  Change: host the metric fields as upstream `TextInput` children of `EditorWidget`, as `GraphWidget` hosts `CodeEditor`.
  This removes the hand-written field in `metrics_card.rs` and closes the key, paste and input method gaps of those fields.

## Phase 6: text engine

- [ ] **6.1 A cluster with two glyphs keeps only the last glyph.** `[test first]`
  Where: `shape_with_font`, `src/text/buffer/shaping.rs:171`, and `apply_shape_updates`, `:423`.
  Problem: one update is stored for each shaped glyph with the sort as the key, so the last one replaces the others.
  Input: beh, fatha, shadda (U+0628 U+064E U+0651).
  Lam, fatha, alef with a ligature that ignores marks.
  A one-to-many substitution.
  Change: set `BufferClusterLevel::Characters`, and let a sort hold a list of shaped glyphs.
- [ ] **6.2 Left-to-right runs have no script.** `[confirmed]`
  Status 2026-11-23: tried and reverted. With a Latin script on the buffer, mark positioning in a compiled font returns no offset (`mark_positioning_tracks_live_anchors_in_both_masters`). Find how the generated mark feature is registered before a second attempt.
  Where: `shape_with_options`, `src/text/shape.rs:312`.
  Problem: with no override, the buffer gets no script, so `harfrust` uses the default shaper.
  Devanagari matras are not reordered.
  Each right-to-left run is shaped as Arabic.
  Change: call `buffer.guess_segment_properties()` after `set_direction`, and delete the Arabic branch.
  Later, split runs by `unicode-script`, which is in `Cargo.lock`.
- [x] **6.3 Line direction comes from a hand-written range table.** `[confirmed]`
  Where: `strong_direction`, `src/text/buffer/mod.rs:45`.
  Seven files define "Arabic" with different ranges.
  Change: use `unicode_bidi::bidi_class`, which is a dependency.
  Write one `script_of(char)` for the seven sites.
- [ ] **6.4 One caret table for hit tests and arrow keys.** `[reported]`
  Where: `nearest_cursor_for_line`, `src/text/buffer/layout.rs:412`, and `move_cursor_visual_left`, `src/text/buffer/mod.rs:1060`.
  Problem: layout and hit test use different rules inside a run of the opposite direction.
  An arrow key can move the caret the wrong way.
  Change: `layout` records the caret x for each boundary.
  The hit test selects the nearest entry, and the arrow keys step through the entries sorted by x.
  Delete `line_width`.
- [ ] **6.5 Reuse the shaper objects.** `[reported]`
  Where: `shape_with_options`, `src/text/shape.rs:286`.
  Problem: each call builds a `ShaperData` and a shape plan, and each new buffer copies the font binary.
  Change: `ShapingFont` holds the `Arc` of the binary, one `ShaperData`, and a plan cache.
- [ ] **6.6 One shaping pass for each edit.** `[reported]`
  Where: `TextState::insert`, `src/application/editor/tools/text.rs:639`.
  Problem: a typed character shapes the buffer three times, and a paste of N characters shapes it 2N times.
  Change: use `insert_str` from item 1.7, and remove the second call in `insert`.
- [ ] **6.7 Replace or delete the hand-written joining table.** `[reported]`
  Where: `src/text/joining.rs` and the fallback in `src/text/buffer/shaping.rs:263`.
  Problem: the table lacks ranges and gives five characters the wrong class.
  Change: take the data from `unicode-joining-type`, or show nominal glyphs with a note when no shaping font exists.
- [ ] **6.8 Text history.** `[reported]`
  Where: `SourceTextArea`, `src/application/widgets/source_text_area.rs:298`, and the Text tool, which has none.
  Problem: each key press is one undo step that stores the whole text, to a limit of 64.
  A character with Control is inserted.
  Tab does not indent.
  Change: store a step as a range with the removed and inserted text, and merge adjacent inserts.
- [x] **6.9 Record why the Text tool does not use Parley.** `[confirmed]`
  Change: one paragraph in the header of `src/text/buffer/mod.rs`: live outlines and advances for each sort, glyph names, and shaping before a binary exists.
- [ ] **6.10 Two script editors.** `[reported]`
  Problem: the Scripts panel uses `SourceTextArea`, and graph script nodes use `CodeEditor` with `vim.rs` (1,327 lines).
  The second places the caret by cell count, which is wrong for wide characters.
  Change: this needs an owner decision.
  See the list at the end.

## Phase 7: document model

- [ ] **7.1 Two small changes to the commit path.** `[confirmed]` for the second.
  Status 2026-11-23: the history reservation is done. The clone in the commit path is open.
  `restore_document_layer_if_current` (`src/font/project.rs:2085`) clones the live layer to compare it, where `layer_matches_snapshot` compares by reference.
  `UndoStack::default` (`src/font/history.rs:73`) reserves 128 slots for each edited layer, where `VecDeque::new()` reserves none.
- [ ] **7.2 A kerning edit copies the metadata of every source twice.** `[reported]`
  Where: `SourceMetadataHistory`, `src/font/history.rs:463`.
  Change: hold feature text, groups, kerning and font info of each source in separate `Arc`s, and record only the sources that changed.
- [ ] **7.3 An HOI record is tied to a hash of every coordinate.** `[reported]`
  Where: `outline_fingerprint`, `src/font/hoi.rs:94`.
  Problem: a move of any point suspends the record, and then the preview and the export of the whole font fail.
  Change: address the endpoint by the GLIF point `identifier`, and guard with the topology.
- [ ] **7.4 Save off the interface thread, changed layers only.** `[reported]`
  Where: `stage_ufo`, `src/font/persistence/mod.rs:564`.
  A save is 2,228 ms at 16,000 glyphs.
  Change: `ExportPlan` owns its data, so `execute` can run on a worker now.
  Then encode only the layers in the change journal.
- [ ] **7.5 Keep original bytes for the plist files.** `[reported]`
  Problem: only GLIF files keep their bytes.
  A one-glyph layer command marks every source dirty, and the next save can change seven plist files in each master.
  Change: apply the `OriginalGlif` method to these files, and mark only the source that changed.
- [ ] **7.6 The designspace reader refuses elements that fontTools writes.** `[reported]`
  Where: `parse`, `src/formats/designspace.rs:35`.
  `source/lib`, `variable-fonts`, `labels` and `mappings` are errors.
  Change: attach each unknown element to its parent as an opaque fragment, and write it back unchanged.
  Read the known-limitations page first.
- [ ] **7.7 Store a layer once, behind `Arc`.** `[reported]` Large.
  Problem: a layer is a `babelfont::Layer` plus a parallel `LayerPreservation`, matched by position.
  About 7,300 lines keep the two in step, with 109 `expect` calls.
  Each point stores its identity in an `IndexMap` of JSON values.
  Change: one owned `Layer` type in `Arc`.
  A snapshot is a pointer, and equality starts with `ptr_eq`.
  Build `babelfont::Font` only at the compiler boundary.
  This is item B11 of the cleanup plan.
- [ ] **7.8 One linear history.** `[reported]` Large.
  Problem: five history stores with different granularity.
  Glyph removal has no store (item 1.1), and source operations copy the whole font.
  Change: after 7.7, a history entry holds the old and new `Arc` of each part that changed.
  This is item B10, and decision 17 fixes the behavior the user sees.

## Phase 8: background work

- [ ] **8.1 One supervisor for jobs and child processes.** `[reported]`
  Problem: 21 `thread::spawn` sites, 9 poll functions and 7 `Drop` types that only set a flag.
  No registry, no process group, no panic guard.
  Change: `Job::spawn` gives each worker a cancellation, a `MessageProxy`, a panic guard and a place in a registry.
  `AppState::shutdown()` cancels, stops the groups, and joins with a deadline.
  It closes items 1.4, 2.11, 2.12 and 4.5.
- [ ] **8.2 Work that blocks the interface thread.** `[reported]`
  `init_graph` runs `font-ml` with a 2 s deadline at each open (`editor/tools/graph/mod.rs:235`).
  `chat_send` connects with timeouts up to 0.9 s (`chat.rs:201`).
  `voice_stop` sleeps 80 ms (`voice.rs:274`).
  `References::refresh` reads and parses each reference font (`platform/reference_fonts.rs:71`).
  The live handler runs 16 agent calls to completion (`platform/live.rs:79`).
  `Server::drop` waits up to 2 s (`live_socket.rs:313`).
  Change: move each to a worker.
  Give the live handler a time budget of about 4 ms for each wake.
- [ ] **8.3 Warnings go to a terminal that a bundled application does not have.** `[reported]`
  Where: 23 `eprintln!` sites in `src/application`, and the note "listed in the terminal" at `platform/host.rs:100`.
  Change: keep the warning list in the workspace and show it.
  Use `tracing::warn!`, which is already in the build.
- [ ] **8.4 Open items of the security section of the audit.** `[reported]`
  Eight of nine are still open:
  - A proposal-only graph run writes to any path.
  - Context files follow symbolic links.
  - The socket list has no owner test.
  - The port selection has a race.
  - Log files are created in the shared temp folder.
  - A proof image path comes from event strings.
  - Client configuration is rewritten in place.
  - Recipe coordinates have no bound.
- [ ] **8.5 Environment reads on a view path.** `[reported]`
  `voice_available` and `graph_choices` read variables and walk folders on each rebuild.
  `EditorWidget::layout` reads two variables (`editor/mod.rs:398`).

## Phase 9: build, tests and documents

- [ ] **9.1 One `kurbo` version.** `[confirmed]`
  `neuraltype-core` pins 0.11.3, and it is an owner repository.
  `spline` pins 0.9.5, which is upstream item 10.
- [ ] **9.2 Make `img2bez` optional in the library.** `[confirmed]`
  It adds 17 crates to the engine build for image tracing.
  Move the `JsonSchema` derive in `font/compiler/proof.rs` to `automation`, so `schemars` leaves the engine too.
- [ ] **9.3 Record the baseline.** `[confirmed]`
  Add the table above to `docs/BENCHMARKS.md`, with the commands that produce it.
- [ ] **9.4 Rendered snapshot tests in CI.** `[confirmed]`
  Problem: screenshots are compared by hand.
  No test fails when a panel changes its appearance.
  Change: capture a small fixed set with the headless path and the CPU renderer, and compare with checked-in images in one CI job.
- [x] **9.5 License headers.** `[confirmed]`
  29 files in `src/application` carry `Apache-2.0` only, and the package is `Apache-2.0 OR MIT`.
  The check in `.github/scripts/copyright.sh` accepts both.
  Use one header and make the check strict.
  This is item W8.
- [ ] **9.6 Retire stale planning documents.** `[confirmed]`
  `docs/` has five planning and review records, 3,153 lines.
  Two contain home paths and the old repository name.
  Close or delete `AI-ARCHITECTURE-CHECKLIST.md` and `VIRTUA-REGULAR-AI-CHECKLIST.md`, and fold this file into the cleanup plan.
- [ ] **9.7 Remove comparisons with GPUI and with "xilem".** `[reported]`
  41 mentions in `src/application` and 22 test names in `src/text/buffer/tests.rs`.
  This is item W7.
- [ ] **9.8 The browser build needs `prepare.py` because of one Tokio feature.** `[reported]`
  `xilem_masonry` in the fork requests `rt-multi-thread`.
  Without it, `prepare.py`, its tests and `web/compat` go.
  This is a fork change, so discuss it first.

## Upstream needs found by this review

These are notes for [the upstream overview](upstream/linebender/OVERVIEW.md), not planned work.
The owner writes all upstream text.
The Masonry release deadline in that overview is 2026-11-27.

- kurbo: `common::solve_cubic` has no relative test for a near-zero leading coefficient (item 3.1, after the Rust test confirms it).
- Masonry: `handle_user_event` does not read the exit flag, so Quit from an async action waits for the next window event.
- Masonry: a timer request for widgets, which removes blink loops (item 4.1).
  Upstream has a `TODO` for it in `text_area.rs`.
- Masonry: `accepts_text_input` that can change with widget state (item 5.9).
- Masonry: a popup layer view, and a `remove_layer` that accepts an unknown identifier (item 5.11).
- Masonry: the paint pass appends children outside the parent clip (item 5.7).
- Masonry: undo history in `TextArea`, and gesture begin and end actions on `Slider`.
- Xilem: `rt-multi-thread` in `xilem_masonry` (item 9.8).
- winit: an `applicationShouldTerminate:` hook (item 1.4).

## Keep as is

- `keyed_canvas`: it records only when the key or the size changes.
- The single action table, the `enabled` match with no wildcard arm, and one dispatcher.
- The thickness field: measured on a worker with a cancel flag, pushed through a `MessageProxy`, painted as an image.
  It is the model for item 4.5.
- The glyph grid: one widget, visible rows only, shared outlines drawn with a transform.
- The staged save with read-back comparison, and `OriginalGlif`.
- Guarded commit and replay: a stale step is refused and never overwrites newer work.
- `LayerView` and `LayerEditDraft` as the only read and write boundary, with norad limited to listed files.
- Hit tests and point insertion on `PathSeg::nearest` and `ParamCurve::subsegment`.
- Bidi from `unicode-bidi`, and shaping from the compiled binary at the location of the outlines.
- `src/workflows/process.rs`: limits on retained bytes, a deadline, and a child that is reaped on unwind.
- The Python recipe boundary: a cleared environment, fixed limits, and a proposal as the only result.
- Hand-written loopback HTTP and JSON-RPC, which add 12 crates and no async client stack.
- `tests/module_layering.rs`, and the idle rebuild test.

## Decisions for the owner

1. **Cargo features for optional subsystems.**
   Decision 14 keeps chat, voice, local models, the graph, scripts and the live socket in the default build with no feature.
   They are about 40,000 production lines, 28% of `src`, and about 38 crates.
   The position on bloat favors optional features.
   The review recommends no change now, and three steps that need no decision: group the 26 tool fields of `Workspace` into one struct, give each subsystem one pump function, and do item 9.2.
2. **A compiler feature for the library.**
   The engine build without the application still compiles `fontc`.
   A library feature for `compiler` is a different question from decision 14.
3. **Scroll on the canvas.**
   Every scroll is a zoom, and the x delta is discarded.
   A trackpad cannot pan without Space or a middle drag.
   Confirm that this is intended.
4. **Tab on the canvas.**
   The canvas consumes Tab, so keyboard focus cannot leave it.
5. **Save over outside changes** (item 1.2).
   The comment says "The editor's version wins".
   The review recommends a refusal with three choices.
6. **One script editor** (item 6.10).
   Keep the Vim keys as a mode of one widget, or remove one of the two editors.
7. **The joining fallback** (item 6.7).
   Take the data from a crate, or delete the path and show nominal glyphs.

## Suggested order

1. Phase 0, in one session.
2. Phase 1, one item for each commit.
   Items 1.1, 1.2, 1.7, 1.8 and 1.9 are small.
3. Phase 2, in any order.
4. Items 4.1 and 4.2, which change what the designer feels while working.
5. Items 3.1 to 3.5, each with its test first.
6. The rest by need.
   Items 4.11, 5.12, 7.7 and 7.8 are large, and each needs a short plan agreed with the owner.

## Not reviewed

No reviewer ran the code, so every runtime claim is from reading.
The reviewers read tests by name only.
These were not read:

- `src/font/project.rs` lines 1200 to 1700
- `src/font/proposal.rs` and `edit_batch.rs` in full
- `src/font/babelfont/edit_*.rs`
- most of `src/formats` except `designspace.rs` and `foreign.rs`
- `src/workflows/graph*.rs` past their headers
- `src/automation/agent*.rs`
- `src/application/view/canvas/proof*`
- most small decorator widgets
- `src/application/design/` past its imports
- `web/` except the manifests and `prepare.py`
- `steamdeck/`
- `scripts/`
- `benches/`

The Windows workflow was not examined.
