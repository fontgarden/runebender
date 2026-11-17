---
item: 8
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

# Masonry: an option for the maximum frame latency

## Problem

`masonry_winit` configures every window surface with `desired_maximum_frame_latency: 2`.
On Metal, `wgpu` sets the `maximumDrawableCount` of the `CAMetalLayer` to the latency plus one, clamped to the supported range, so a Runebender window keeps three drawables, 52 MB.
A value of 1 gives two drawables and is expected to save about 18 MB; this is not measured yet.
Masonry has no option to change the value.

## Upstream context

Maintained by the agent.
Every claim has a link, or a file path at a named commit.
Mark anything that was not checked as "not checked".

- Upstream commit read: `dc496876`.
  `RenderContext::create_surface` in `masonry_winit/src/vello_util.rs` sets the value 2 in its `SurfaceConfiguration`.
- Who set the value 2 and why: in Xilem it arrived with the file itself in [#1393](https://github.com/linebender/xilem/pull/1393) (RagibHasin, merged 2025-09-23), which copied `vello::util` and changed the alpha handling (`git log -S desired_maximum_frame_latency upstream/main`).
  In Vello the value was introduced in [vello#444](https://github.com/linebender/vello/pull/444) "Upgrade wgpu to 0.19" (DJMcNab, 2024-02-19, commit `a8b33b39`), when `wgpu` 0.19 added the field.
  Neither pull request gives a reason for 2.
- What `wgpu` documents (`wgpu-types/src/surface.rs` at `v28.0.0`): 2 is "a reasonable default for most applications"; 1 minimizes latency, does not let the CPU and GPU run in parallel, and is described as a reasonable choice for GUIs that do little GPU work per frame.
  On Metal the value maps to `maximumDrawableCount = latency + 1` (`wgpu-hal/src/metal/surface.rs`).
- Existing API to follow: `MasonryState::set_present_mode(window_id, present_mode)` in `masonry_winit/src/event_loop_runner.rs` changes another `SurfaceConfiguration` field at run time, through `RenderContext::set_present_mode`.
  Surfaces are created with `PresentMode::AutoVsync` in `create_surface`.
- Measured effect of the value 1 on memory and on frame pacing: not measured.
  Item M3 in [the cleanup plan](../../CLEANUP-PLAN.md) is the trial.
- Related issues and pull requests:
  - [#918](https://github.com/linebender/xilem/issues/918) "The memory usage is extremely high!" (open, 2025-04-04): DJMcNab called high memory use "a known problem" and asked to use the issue to find the major sources; later comments point at Vello's fixed buffers, `wgpu` memory hints and Vello's buffer cache.
    Frame latency is not mentioned.
  - [#712](https://github.com/linebender/xilem/pull/712) "Use PresentMode::Mailbox when available." (open since 2024-10-22): another surface setting, not merged.
  - [#1866](https://github.com/linebender/xilem/pull/1866) (open, 2026-11-08) bumps `wgpu` to 30.0.1 and touches the same render setup.
  - Searches for "frame latency", "desired_maximum_frame_latency", "maximum frame latency", "latency", "memory usage" and "GPU memory" on 2026-11-17 found nothing else.
- Earlier discussion on Zulip: searches for "frame latency" and "maximum_frame_latency" found no Masonry or Xilem thread on this setting.
- Statements by maintainers about the direction of this area: none found on frame latency.
  The open refactor issue [#1836](https://github.com/linebender/xilem/issues/1836) "Refactor `masonry_winit`" (PoignardAzur, 2026-08-07) plans to split `MasonryState` and `Window` and calls the current code "pretty messy", so a new option there can conflict with that refactor.
- Checks upstream runs on a pull request: see [What every Xilem pull request needs](README.md#what-every-xilem-pull-request-needs).
  `masonry_winit` has no tests for surface configuration.
- Changelog or documentation expected in the same pull request: no `CHANGELOG.md`; doc comments on any new public method.

## Options

Maintained by the agent.

1. A run-time setter on `MasonryState`, next to `set_present_mode`, with the default unchanged; plus a Xilem window option if Xilem applications need it.
2. A request to change the default to 1 for every application.
   The `wgpu` documentation supports 1 for GUIs, but it trades throughput for latency on every platform, and it needs measurements on more than macOS.
3. No pull request, if the trial shows stutter or no saving.

Do the trial first; without numbers there is nothing to propose.

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
