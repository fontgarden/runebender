# Overview of planned pull requests

Runebender carries changes and workarounds that belong in its dependencies.
This document lists each one as a planned pull request to the upstream project.
When a pull request merges, update the pinned revision in `Cargo.toml` and delete the workaround here.

Status on 2026-11-17: nothing in this list has been proposed upstream yet.
The facts below were checked against `linebender/xilem` `main` at `dc496876` (2026-11-03).
Check each item against current `main` again before starting it.

The rules and the steps are in [README.md](README.md).
Each item below has its own working file:

| Item | File |
| --- | --- |
| 1 | [xilem-dropped-files.md](xilem-dropped-files.md) |
| 2 | [masonry-concentric-borders.md](masonry-concentric-borders.md) |
| 3 | [wgpu-metal-premultiplied-alpha.md](wgpu-metal-premultiplied-alpha.md) |
| 4 | [masonry-split-without-bar.md](masonry-split-without-bar.md) |
| 5 | [masonry-portal-scrollbar-visibility.md](masonry-portal-scrollbar-visibility.md) |
| 6 | [xilem-text-input-color-on-rebuild.md](xilem-text-input-color-on-rebuild.md) |
| 7 | [xilem-prose-font-family.md](xilem-prose-font-family.md) |
| 8 | [masonry-winit-frame-latency.md](masonry-winit-frame-latency.md) |
| 9 | [vello-buffer-sizes.md](vello-buffer-sizes.md) |
| 10 | [spline-update-kurbo.md](spline-update-kurbo.md) |

## Changes the fork carries today

The fork is `eliheuer/xilem`, pinned at `0b0b5e75`.
It is three commits and 78 lines ahead of its upstream base, `271a27a6`.

### 1. Deliver dropped files to the application

- Fork commit: `5a813cb5`.
- Files: `masonry_winit/src/app_driver.rs`, `masonry_winit/src/event_loop_runner.rs`, `xilem/src/driver.rs`, `xilem/src/app.rs`.
- What it does: passes the window system's dropped-file event to the application, so a font file can be dropped on the window.
- Upstream state: `main` has no dropped-file handling in `masonry_winit` or `xilem`.
- Before the pull request: ask upstream where file drop belongs (application driver, or a widget event), because the fork takes the smallest route.
- Runebender code removed after merge: none. The fork commit is dropped.

### 2. Concentric borders

- Fork commit: `0b0b5e75`.
- Files: `masonry_core/src/core/widget_paint.rs`, `masonry_core/src/properties/border_width.rs`.
- What it does: strokes a rounded border at the corner radius minus half the border width, and fills to the centerline, so the inner and outer edges of the border are concentric.
- Before the pull request: make a small picture that shows the difference at a large border width, and check that upstream has not changed border painting since `271a27a6`.
- Runebender code removed after merge: none. The fork commit is dropped.

### 3. Premultiplied presentation on Metal

- Fork commit: `881d2dae`.
- Files: `masonry_winit/src/vello_util.rs`, and the window option in `xilem/src/window_options.rs` and `xilem/src/window_view.rs` (confirm which commit owns these lines).
- What it does: premultiplies the presented image on Metal, a workaround for the alpha mode that `wgpu` reports. See [Xilem #1852](https://github.com/linebender/xilem/issues/1852), which is closed with the findings.
- This is probably not a Xilem pull request. The correction belongs in `wgpu`. Action: find or open the `wgpu` issue, link it here, and keep the fork commit until a `wgpu` release contains the correction.

## Options Runebender needs, with a workaround in this repository

Each of these workarounds is a file in `src/application/widgets/`.

### 4. A splitter that paints no bar

- Workaround: `quiet_split.rs` and `quiet_split_widget.rs`, 791 lines. They are a copy of Masonry `Split` from an older revision (`b81d8d7`) with the bar painting removed.
- Upstream state: `Split` has `solid_bar(bool)` and paints a solid, stroked or focus bar. It has no way to paint nothing.
- Proposed change: a way to turn bar painting off, as a builder method or a property. The pointer, keyboard, focus and accessibility behavior stays the same.
- Risk: the copy is old. Compare it with current `Split` first, because upstream behavior can have changed.
- Runebender code removed after merge: both files.

### 5. A scroll area with hidden scrollbars

- Workaround: `scroll_viewport.rs`, 389 lines. It moves the scrollbar widgets of `Portal` outside the clip after layout, because a zero-size `ScrollBar` still paints.
- Upstream state: `Portal` has private `scrollbar_horizontal_visible` and `scrollbar_vertical_visible` fields that it computes itself. No public policy exists.
- Proposed change: a scrollbar visibility policy on `Portal` (for example automatic, always, never), and the matching option on the Xilem `portal` view.
- Runebender code removed after merge: most of the file. Check `constrain_vertical`, which is never set, and delete it either way.

### 6. Text input color and placeholder font

- Workaround: `input_typography.rs`, 450 lines, of which 290 are tests.
- Problem as recorded in the file: the Xilem `TextInput` view sets `ContentColor` on the outer input, but Masonry reads that property from the inner `TextArea`, so a color change on rebuild has no effect. The placeholder label also does not take the font of the input.
- Upstream state, confirmed by reading `main` at `dc496876`: the view is now `xilem_masonry/src/view/text_input.rs`. `build` puts `ContentColor` in the property set of the inner `TextArea`. `rebuild` calls `element.insert_prop(ContentColor { color })` and `element.remove_prop::<ContentColor>()` on the outer `TextInput`, under a `TODO` comment. `TextArea` reads the property from itself (`masonry/src/widgets/text_area.rs`). So a color that changes after the first build has no effect.
- The first step is a small upstream test that changes `text_color` on rebuild and shows the old color. The placeholder font part was not checked against `main`.
- This is a defect report with a correction, not new API, so it is the best first pull request.
- Runebender code removed after merge: the file.

### 7. A font family option on `prose`

- Workaround: `selectable_text.rs`, 114 lines.
- Problem: the Xilem `prose` view gives selection and wrapping but has no font family setter, so prose cannot use the bundled interface font.
- Upstream state, confirmed by reading `main` at `dc496876`: the view is now `xilem_masonry/src/view/prose.rs`. Its setters are `text_color`, `text_alignment`, `text_size`, `line_break_mode` and `weight`. It has no font family setter. The `text_input` view in the same folder has one (`font`), which is the pattern to follow.
- Proposed change: a font family setter on the `prose` view, matching the one on `label`.
- Runebender code removed after merge: the file.

## Changes for memory use

### 8. Frame latency

- Upstream state: `masonry_winit/src/vello_util.rs` sets `desired_maximum_frame_latency: 2`.
- Effect: the window keeps three drawables, 52 MB for a Runebender window. A value of 1 is expected to save about 18 MB.
- First step: item M3 in [the cleanup plan](CLEANUP-PLAN.md) tries the value 1 in the fork and checks for stutter.
- Proposed change, if the trial is good: a window option for the latency, with the default unchanged. A request to change the default for every application is a separate and harder discussion.

### 9. Vello buffer sizes

- Project: `linebender/vello`, not Xilem.
- Problem: `vello_encoding/src/config.rs` allocates seven bump buffers at fixed sizes (`1 << 21` and `1 << 23` elements), 166 MB of GPU memory for any scene. `imaging_vello` exposes no option.
- First step: read the Vello issues and Zulip for existing work on dynamic buffer sizes. This is a known topic, and a design probably exists.
- This is the largest item and it changes a renderer. Do it last, and only after a discussion with the Vello maintainers.
- A further 168 MB is held in 42 Metal blocks of 4 MB. The cause is not known. Find it before proposing anything.

## Other projects

### 10. `spline`: update to the current `kurbo`

- Project: `linebender/spline`.
- Problem: `spline` depends on `kurbo` 0.9.5, so Runebender builds two `kurbo` versions and converts between them in `src/outline/path/hyper.rs`.
- Proposed change: update the dependency and correct any API differences.
- Runebender code removed after merge: the `kurbo_09` dependency in `Cargo.toml` and the conversions.

## Suggested order

1. Item 6: a defect with a test, small, no new API.
2. Item 7: one setter that follows an existing pattern.
3. Item 10: a dependency update.
4. Item 2: a small painting correction with a picture.
5. Item 5, then item 4: new options on existing widgets. Item 4 removes the most code here.
6. Item 1: needs a design decision upstream.
7. Item 8, after the trial.
8. Item 9, after discussion.
9. Item 3 is tracked, not proposed to Xilem.

## Record

Add one line per pull request when it is opened: item number, link, date, state.

- None yet.
