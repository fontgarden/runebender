---
item: 1
upstream_repository: linebender/xilem
base_branch: main
head_repository: eliheuer/xilem
head_branch:
state: research
owner_decided_design: no
owner_reviewed_commit:
owner_wrote_text: no
llm_disclosure: undecided
pull_request:
---

# Xilem: deliver dropped files to the application

## Problem

A file dropped on a Xilem window does not reach the application.
`masonry_winit` ignores the winit `WindowEvent::DroppedFile` event on `linebender/xilem` `main` at `dc496876`.
The Runebender fork carries commit `5a813cb5`, 49 added lines in six files.
It adds `AppDriver::on_file_dropped` in `masonry_winit/src/app_driver.rs`, dispatches `DroppedFile` to it in `masonry_winit/src/event_loop_runner.rs`, and adds a `WindowOptions::on_file_drop` callback in `xilem/src/window_options.rs`, `xilem/src/window_view.rs`, `xilem/src/driver.rs` and `xilem/src/app.rs`.
The callback receives one path per event and no pointer position, because winit 0.30 reports only the window and the path.

## Upstream context

Maintained by the agent.
Every claim has a link, or a file path at a named commit.
Mark anything that was not checked as "not checked".

- Upstream commit read: `dc496876` (2026-11-03), the head of `linebender/xilem` `main` on 2026-11-17 (`git ls-remote` and `gh api repos/linebender/xilem/commits/main`).
- Code: `git grep -i` for `DroppedFile`, `HoveredFile`, `dropped`, `drag.and.drop` on `upstream/main` finds no drop handling.
  The only related item is the Windows-only `drag_and_drop: bool` window attribute in `xilem/src/window_options.rs`, added in #1380.
- Workspace dependency: `winit = "0.30.13"` in `Cargo.toml` at `dc496876`.
- Whether the fork commit still applies: `git merge-tree --write-tree --merge-base=5a813cb5^ upstream/main 5a813cb5` reports no conflict on 2026-11-17.
  None of the six files changed between the fork base `271a27a6` and `dc496876` except `masonry_winit/src/vello_util.rs`, which this commit does not touch.
- Correction to the earlier notes: `xilem/src/window_options.rs` and `xilem/src/window_view.rs` belong to this commit (`5a813cb5`), not to `881d2dae`.
- Tests: no test on `main` covers window events in `masonry_winit` or the Xilem driver, and no snapshot test is affected (`git grep` for `TestHarness` outside `masonry*` finds no Xilem driver tests).
- History of this code: `AppDriver` hooks such as `on_close_requested` and `on_wgpu_ready` are in `masonry_winit/src/app_driver.rs`; the fork follows the `on_close` pattern from `xilem/src/window_options.rs`.
- Related issues and pull requests: `gh search issues --include-prs` in `linebender/xilem` for "drag and drop", "drop file", "file drop", "DroppedFile", "HoveredFile", "dropped files", "drag drop", "drop", "drag" on 2026-11-17 found no issue or pull request about file drop.
  The org-wide search found only old Druid issues ([druid#1742](https://github.com/linebender/druid/issues/1742), [druid#1872](https://github.com/linebender/druid/issues/1872), both open since 2021) and the closed [druid#2262](https://github.com/linebender/druid/issues/2262).
- Upstream winit: `v0.31.0-beta.4` (2026-10-24) replaces the 0.30 file events with `DragEntered`, `DragMoved`, `DragDropped` and `DragLeft`, which carry a data transfer id and a position (`winit-core/src/event.rs` at that tag).
  The stable release is still `v0.30.13`.
- Earlier discussion on Zulip:
  - [#xilem > Drag and Drop?](https://xi.zulipchat.com/#narrow/channel/354396-xilem/topic/Drag.20and.20Drop.3F/near/446872815) (2024-06-25): Daniel McNab pointed to winit `DroppedFile` and said they would integrate drag and drop if winit supported it.
  - [#rust ui > Winit: Drag & Drop and Clipboard support](https://xi.zulipchat.com/#narrow/channel/422907-rust-ui/topic/Winit.3A.20Drag.20.26.20Drop.20and.20Clipboard.20support/near/634985217) (2026-08-10 to 2026-10-26): Nico Burns reported the new winit drag and drop API (winit#4571); Olivier FAURE wrote that he was "tempted to have masonry_winit depend on Winit 0.31-beta.4".
  - [#masonry > Linebender 2026 roadmap - Masonry](https://xi.zulipchat.com/#narrow/channel/317477-masonry/topic/Linebender.202026.20roadmap.20-.20Masonry/near/636319344) (2026-10-30): Olivier FAURE first added the winit 0.31 upgrade to the release list, then moved it to the long-term roadmap.
- Statements by maintainers about the direction of this area: the same roadmap thread sets a hard deadline for a major Masonry release before 2026-11-27 and limits the work before it to the layer system, display scaling and a resource system ([message](https://xi.zulipchat.com/#narrow/channel/317477-masonry/topic/Linebender.202026.20roadmap.20-.20Masonry/near/636301246)).
- Checks upstream runs on a pull request: see [What every Xilem pull request needs](README.md#what-every-xilem-pull-request-needs).
  This change touches `masonry_winit` and `xilem`, so the Android `cargo apk check` and the MSRV checks build it.
- Changelog or documentation expected in the same pull request: `linebender/xilem` has no `CHANGELOG.md` at `dc496876`.
  New public items need doc comments, because CI runs `cargo doc` with `-D warnings` and the Linebender lint set.

## Options

Maintained by the agent.

1. Propose the fork's route on winit 0.30: an `AppDriver` hook and a `WindowOptions` callback with one path per event.
   Small, and enough for Runebender.
   A maintainer could object that the API changes again when Masonry moves to winit 0.31, where drops carry a position and a data transfer id.
2. A widget-level drop event with a pointer position, so a widget can be a drop target.
   This fits the winit 0.31 API better, and it is larger and touches Masonry's event passes.
3. Wait for the winit 0.31 upgrade, which a maintainer has put on the long-term roadmap, and design file drop on that API.

The facts above suggest raising this as a discussion first, in the owner's own words, on Zulip `#masonry` or in an issue.
Timing: the Masonry release deadline is 2026-11-27, and file drop is not one of the three areas named for work before it.

## Owner's decision

Written by the owner.

## Code record

One line for each commit on the branch: hash, what it does, and who wrote it (owner, agent draft reviewed by owner, or mixed with detail).
This record is the source for the disclosure.

## Verification

Commands and results, with the date and the commit.

- Upstream checks:
- Runebender built against the branch:
- Workaround removed in Runebender and tests pass:

## Pull request text

Written by the owner.
An agent does not write, draft or reword anything between the markers.
The script sends exactly what is between each pair of markers.

### Title

<!-- title:start -->
TODO(owner)
<!-- title:end -->

### Description

Include the LLM disclosure here when one applies.

<!-- body:start -->
TODO(owner)
<!-- body:end -->

## Record

- Opened:
- Review notes:
- Merged:
- Runebender follow-up (pinned revision, workaround deleted):
