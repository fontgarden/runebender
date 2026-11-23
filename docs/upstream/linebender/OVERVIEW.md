# Overview of planned pull requests

Runebender carries changes and workarounds that belong in its dependencies.
This document lists each one as a planned pull request to the upstream project.
When a pull request merges, update the pinned revision in `Cargo.toml` and delete the workaround here.

Status on 2026-11-17: nothing in this list has been proposed upstream yet.
The facts below were checked on 2026-11-17 against `linebender/xilem` `main` at `dc496876` (2026-11-03), which was still the head of `main` that day.
Vello facts were checked against `linebender/vello` `main` at `84ffc24a` (2026-11-16), and `spline` facts against `linebender/spline` `main` at `83f46cbf`.
Check each item against current `main` again before starting it.
The evidence for each line is in the item's working file.

The rules and the steps are in [README.md](README.md).
Each item below has its own working file:

| Item | File | Status |
| --- | --- | --- |
| 1 | [xilem-dropped-files.md](xilem-dropped-files.md) | Problem confirmed on main at `dc496876`; discuss first. |
| 2 | [masonry-concentric-borders.md](masonry-concentric-borders.md) | Problem confirmed on main at `dc496876`; the background part touches the plan in xilem#1592. |
| 3 | [wgpu-metal-premultiplied-alpha.md](wgpu-metal-premultiplied-alpha.md) | No pull request needed, because gfx-rs/wgpu#9922 is merged; waiting for a `wgpu` release that contains it. |
| 4 | [masonry-split-without-bar.md](masonry-split-without-bar.md) | Problem confirmed on main at `dc496876`; no property hides the bar. |
| 5 | [masonry-portal-scrollbar-visibility.md](masonry-portal-scrollbar-visibility.md) | Problem confirmed on main at `dc496876`; a maintainer called a hide option "an easy win" on Zulip. |
| 6 | [xilem-text-input-color-on-rebuild.md](xilem-text-input-color-on-rebuild.md) | Problem confirmed on main at `dc496876` by reading the code; a test is still needed. |
| 7 | [xilem-prose-font-family.md](xilem-prose-font-family.md) | Problem confirmed on main at `dc496876`; discuss first in xilem#1795. |
| 8 | [masonry-winit-frame-latency.md](masonry-winit-frame-latency.md) | Problem confirmed on main at `dc496876`; measure first. |
| 9 | [vello-buffer-sizes.md](vello-buffer-sizes.md) | No new pull request, because vello#1861 already proposes the option; try the hybrid backend first. |
| 10 | [spline-update-kurbo.md](spline-update-kurbo.md) | Problem confirmed on main at `83f46cbf`; discuss first, because spline#28 also changes the `kurbo` version. |

## Changes the fork carries today

The fork is `eliheuer/xilem`, pinned at `0b0b5e75`.
It is three commits and 78 lines ahead of its upstream base, `271a27a6`.
All three commits apply to `dc496876` without conflict (`git merge-tree`, 2026-11-17).

### 1. Deliver dropped files to the application

- Fork commit: `5a813cb5`.
- Files: `masonry_winit/src/app_driver.rs`, `masonry_winit/src/event_loop_runner.rs`, `xilem/src/driver.rs`, `xilem/src/app.rs`, `xilem/src/window_options.rs`, `xilem/src/window_view.rs`.
- What it does: passes the winit 0.30 `DroppedFile` event to the application, so a font file can be dropped on the window.
- Upstream state: `main` has no dropped-file handling in `masonry_winit` or `xilem`, and no issue or pull request asks for it.
- Upstream direction: winit `v0.31.0-beta.4` replaces the file events with a drag and drop API that carries a position, and a Masonry maintainer has put the winit 0.31 upgrade on the long-term roadmap.
- Before the pull request: ask upstream where file drop belongs and whether to wait for winit 0.31, in the owner's own words.
- Runebender code removed after merge: none.
  The fork commit is dropped.

### 2. Concentric borders

- Fork commit: `0b0b5e75`.
- Files: `masonry_core/src/core/widget_paint.rs`, `masonry_core/src/properties/border_width.rs`.
- What it does: strokes a rounded border at the corner radius minus half the border width, and fills the background to the border centerline.
- Upstream state: border painting is unchanged since `271a27a6`.
  The default theme gives eight widget types a 1 px border, so the change alters many exact-match snapshots.
- Before the pull request: make a small picture that shows the difference at a large border width, and decide whether to send the background part, which xilem#1592 plans differently.
- Runebender code removed after merge: none.
  The fork commit is dropped.

### 3. Premultiplied presentation on Metal

- Fork commit: `881d2dae`.
- Files: `masonry_winit/src/vello_util.rs` only.
  The window option lines belong to `5a813cb5`.
- What it does: premultiplies the presented image on Metal, a workaround for the alpha mode that `wgpu` reports.
  See [Xilem #1852](https://github.com/linebender/xilem/issues/1852).
- Upstream state: the `wgpu` issue is [gfx-rs/wgpu#9896](https://github.com/gfx-rs/wgpu/issues/9896), closed by [gfx-rs/wgpu#9922](https://github.com/gfx-rs/wgpu/pull/9922) (merged 2026-08-18).
  The fix is in the `wgpu` "Unreleased" changelog, not in 30.0.2.
  Xilem uses `wgpu` 28; open xilem#1866 moves to 30.0.1.
- Action: none upstream.
  Drop the fork commit when Xilem uses a `wgpu` release with #9922; Masonry's existing `PreMultiplied` branch then applies.

## Performance candidates

These are not planned pull requests yet, and they have no working file.
They are recorded here so that the facts are not measured twice.
Do not start one before the owner has discussed it in this repository, and do not change the fork for one that upstream is unlikely to accept.

The facts were checked on 2026-11-23 against `linebender/xilem` `main` at `9a78df87` (2026-11-22) and against the pinned fork revision `0b0b5e75`.
The measurements are from an Apple M4 Pro with Metal.
The native measurements used an instrumented copy of `masonry_winit` and a 30 Hz external display.

| Candidate | Size | Upstream state | Evidence |
| --- | --- | --- | --- |
| A. Do not render and present a frame that repaints nothing | Small | Open issue [#1562](https://github.com/linebender/xilem/issues/1562): a maintainer confirms that an animation frame always causes a redraw. | `RenderRootSignal::RequestAnimFrame` is handled as a redraw request (`masonry_winit/src/event_loop_runner.rs`, marked `TODO`). The Text tool caret and a focused `TextArea` request an animation frame on each frame, so the whole window renders at the display rate. |
| B. Replace the wait for the GPU after `present` | Small | No issue found. | `present_surface` calls `device.poll(wgpu::PollType::wait_indefinitely())`. On Metal that wait sleeps in steps of 1 ms (`wgpu-hal` 28.0.1, `metal/device.rs`). One step measured 1.27 ms; a frame costs 1.6 to 3.2 ms with the wait and 0.3 ms of CPU time without it. |
| C. Wait for the drawable off the interface thread, and paint after the wait | Large | No issue found. It changes the frame loop on each platform, so discuss it upstream before any code. | `redraw` runs the paint pass and then calls `get_current_texture`, which blocks until the display releases a drawable. In the measurement above the block was 20 to 30 ms of a 33 ms frame, the CPU work was about 2 ms, and the newest input was 36 ms old at `present`. Without display synchronization and without the GPU wait, the newest input was 6 ms old. |
| D. Run the application logic once for each frame, not once for each action | Medium | No issue found. | `MasonryDriver::on_action` in `xilem/src/driver.rs` runs the logic and the view diff at once for each action. A recorded session held 297 rebuilds in one second of window resizing. |
| E. A `canvas` view that does not record on each rebuild | Small | No issue found. | `Canvas::rebuild` in `xilem_masonry/src/view/canvas.rs` calls `update_scene` without a comparison, and `Canvas::message` answers `CanvasSizeChanged` with `RequestRebuild`. Runebender uses `keyed_canvas` in `src/application/widgets/keyed_canvas.rs` in its place. |
| F. The dense trace log of debug builds | Small | Upstream tracks it in [#1556](https://github.com/linebender/xilem/issues/1556). | `masonry_core/src/app/tracing_backend.rs` writes each trace event to a file in debug builds. A development session of 27 minutes wrote 302,118 lines, 42 MB. The time cost is not measured. |
| G. Report the stages of a redraw to the application | Small | No issue found. | `MasonryState::redraw` runs the paint pass, the render, `get_current_texture`, `present` and the GPU wait in one call, and `AppDriver` has no callback between them. The frame time recorder of Runebender (`src/application/platform/frame_times.rs`) runs its own `ApplicationHandler` over the public `MasonryState` methods, so it times a redraw as one value and cannot separate the wait for the display from the render. |

Damage regions are tracked upstream in [#789](https://github.com/linebender/xilem/issues/789), with no progress.
A measurement of one editor frame showed a small gain from a cached panel layer with the classic Vello backend on this machine: 1.4 ms to about 0.8 ms at a scale factor of 2.
Vello Hybrid (`imaging_vello_hybrid` 0.0.1) built and rendered the editor correctly, at 2.3 to 6.1 ms of CPU time for each frame against 0.3 ms for classic Vello.
Neither is a candidate until a slow GPU is measured.

## Options Runebender needs, with a workaround in this repository

Each of these workarounds is a file in `src/application/widgets/`.

### 4. A splitter that paints no bar

- Workaround: `quiet_split.rs` and `quiet_split_widget.rs`, 791 lines.
  They are a copy of Masonry `Split` from `b81d8d7a` with an empty `paint`.
- Upstream state: `Split` paints a solid or stroked bar in theme colors and a focus rectangle; it reads no color property.
  `split.rs` is unchanged since `b81d8d7a`, so the copy is not out of date.
- Bar thickness zero with `solid_bar(true)` paints nothing while unfocused, from reading the code, but the focus rectangle remains.
- Proposed change: a way to turn bar painting off, as a builder method or a property, with a decision on the focus indicator.
- Runebender code removed after merge: both files.

### 5. A scroll area with hidden scrollbars

- Workaround: `scroll_viewport.rs`, 389 lines.
  It moves the scrollbar widgets of `Portal` outside the clip after layout.
- Upstream state: `Portal` shows a scrollbar whenever content overflows an unconstrained axis; constraining the axis hides the bar but also stops scrolling on it.
  `AutoHideScrollBar` (xilem#1773) fades bars out when the pointer is idle but does not keep them hidden.
- On Zulip in 2025-11, Olivier FAURE wrote that a pull request that lets you hide scrollbars "would be an easy win".
- Proposed change: a scrollbar visibility policy on `Portal`, or a property in the style of `AutoHideScrollBar`, and the matching Xilem option.
- Runebender code removed after merge: most of the file.
  Delete the unused `constrain_vertical` field either way.

### 6. Text input color and placeholder font

- Workaround: `input_typography.rs`, 450 lines, of which 290 are tests.
- Upstream state at `dc496876`: `build` puts `ContentColor` on the inner `TextArea`, and `rebuild` puts it on the outer `TextInput`.
  Property lookup in Masonry never reads a parent's properties, and `widgets::TextInput` forwards `CaretColor`, `SelectionColor` and `PlaceholderColor` but not `ContentColor`.
  So a color change on rebuild has no effect.
- The placeholder `Label` gets no font family, size or weight from the view.
  Also still a problem.
- Precedent: xilem#1857 (merged 2026-11-03) added `ContentColor` forwarding to `widgets::Prose`, the same pattern a Masonry-side fix for `TextInput` would use.
- This is a defect with a correction and no new API, so it is still the best first pull request.
- Runebender code removed after merge: the file.

### 7. A font family option on `prose`

- Workaround: `selectable_text.rs`, 114 lines.
- Upstream state: `prose` has no `font` setter; `label`, `text_input`, `checkbox` and `variable_label` have `font(impl Into<FontFamily<'static>>)`.
- Open issue xilem#1795 asks for parity between `Label` and `Prose`, `font` included, and PoignardAzur replied in 2026-05 that he would look into it.
- New on `main`: xilem#1857 makes `widgets::Prose` copy its own `ContentColor` into the inner `TextArea` when added.
  Reading the code, this overwrites the first `text_color` of the Xilem `prose` view and of Runebender's `selectable_text`.
  Not tested.
- Runebender code removed after merge: the file.

## Changes for memory use

### 8. Frame latency

- Upstream state: `masonry_winit/src/vello_util.rs` sets `desired_maximum_frame_latency: 2`.
  The value came from `vello::util` (vello#444, 2024-02-19) through xilem#1393, with no stated reason.
  The `wgpu` documentation calls 2 a reasonable default and 1 a reasonable choice for GUIs.
- Effect: the window keeps three drawables, 52 MB for a Runebender window.
  A value of 1 is expected to save about 18 MB.
- First step: item M3 in [the cleanup plan](../../CLEANUP-PLAN.md) tries the value 1 in the fork and checks for stutter.
- Proposed change, if the trial is good: a per-window setter next to `MasonryState::set_present_mode`, with the default unchanged.
  A request to change the default for every application is a separate and harder discussion.

### 9. Vello buffer sizes

- Project: `linebender/vello`, not Xilem.
- Problem: Vello Classic allocates seven bump buffers at fixed sizes, 166 MB of GPU memory for any scene.
- Upstream state: the sizes are unchanged on Vello `main`, where Vello Classic now lives under `research/` and the sparse strips renderers are the default.
  Open vello#1861 (2026-08-31) already makes the sizes configurable; it has no review and conflicts with `main`.
- Masonry already offers the `imaging_vello_hybrid` backend (xilem#1847), which does not use these buffers.
- First step: measure Runebender with `imaging_vello_hybrid`, and with `wgpu` memory hints, before any upstream action.
- A further 168 MB is held in 42 Metal blocks of 4 MB.
  The cause is not known.
  Zulip reports point at the `wgpu` allocator's block sizes on Vulkan; Metal is not checked.

## Other projects

### 10. `spline`: update to the current `kurbo`

- Project: `linebender/spline`.
- Problem: `spline` `main` (`83f46cbf`, 2023-03-14) depends on `kurbo` 0.9, so Runebender builds two `kurbo` versions and converts between them in `src/outline/path/hyper.rs`.
- Upstream state: no CI; changing the dependency to `kurbo` 0.13 builds and passes the tests with no source change.
  Open spline#28 (raphlinus) also changes the dependency, to 0.11.
  Raph Levien wrote on Zulip in 2026-11 that he is open to collaborating in the repository and releasing a crate.
- Before the pull request: ask Raph Levien whether he wants the update separately or as part of spline#28 or the hyperbezier release.
- Runebender code removed after merge: the `kurbo_09` dependency in `Cargo.toml` and the conversions.

## Suggested order

1. Item 6: a defect with a test, small, no new API.
2. Item 5: a maintainer has said a hide option is welcome, and the Xilem option can be a property that already works through `.prop(...)`.
3. Item 7: comment in xilem#1795 first; if the `text_color` effect of xilem#1857 is confirmed by running an example, report it as a separate small defect.
4. Item 10: ask Raph Levien first, then a one-line dependency change.
5. Item 2: a painting correction with a picture and many snapshot updates; consider sending the radius part alone.
6. Item 4: a new option on `Split`; it removes the most code here.
7. Item 1: discuss first; the winit 0.31 drag and drop API may change the design.
8. Item 8, after the trial.
9. Item 9: measure the hybrid backend first; take part in vello#1861 instead of a new pull request.
10. Item 3 needs no pull request; drop the fork commit after a `wgpu` release with #9922.

Olivier FAURE has set a deadline of 2026-11-27 for a major Masonry release and limited the work before it to three areas (layers, display scaling, resources); see [README.md](README.md#what-every-xilem-pull-request-needs).

## Record

Add one line per pull request when it is opened: item number, link, date, state.

- None yet.
