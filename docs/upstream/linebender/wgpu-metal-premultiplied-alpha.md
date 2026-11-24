---
item: 3
upstream_repository: gfx-rs/wgpu
base_branch: main
head_repository: eliheuer/wgpu
head_branch:
state: research
owner_decided_design: no
owner_reviewed_commit:
owner_wrote_text: no
llm_disclosure: undecided
pull_request:
---

# `wgpu`: alpha mode on Metal (tracked, not a Linebender pull request)

## Problem

Every released `wgpu` up to 30.0.2 advertises `CompositeAlphaMode::PostMultiplied` for a Metal surface, but Core Animation composites a non-opaque `CAMetalLayer` as premultiplied alpha.
Masonry picks `PostMultiplied` first when the surface offers it and blits Vello's output unchanged (`masonry_winit/src/vello_util.rs` at `dc496876`), so translucent windows on macOS show bright, jagged edges.
The Runebender fork of Xilem carries commit `881d2dae` (13 added and 4 removed lines, `masonry_winit/src/vello_util.rs` only): on the Metal backend it keeps `PostMultiplied` but blits with the premultiplying blend state.
The correction belongs in `wgpu`, and it is merged there.
This file tracks the workaround until Xilem uses a `wgpu` release that contains the correction.
`gfx-rs` is not a Linebender project; no `wgpu` pull request is needed.

## Upstream context

Maintained by the agent.
Every claim has a link, or a file path at a named commit.
Mark anything that was not checked as "not checked".

- The `wgpu` issue: [gfx-rs/wgpu#9896](https://github.com/gfx-rs/wgpu/issues/9896) "Why does metal backend advertise PostMultiplied rather than PreMultiplied alpha?" (opened 2026-07-17 by nicoburns, closed 2026-08-18).
- The `wgpu` correction: [gfx-rs/wgpu#9922](https://github.com/gfx-rs/wgpu/pull/9922) "Advertise PreMultipled rather than PostMultiplied alpha for metal backend", merged into `trunk` on 2026-08-18 as `f32f76dc`.
- Release state, checked on 2026-11-17 with `gh api repos/gfx-rs/wgpu/compare`: `f32f76dc` is not in `v30.0.0`, `v30.0.1` or `v30.0.2` (2026-11-12).
  The `trunk` `CHANGELOG.md` lists it under "Unreleased" as a BREAKING change: code that requests `PostMultiplied` on macOS gets `UnsupportedAlphaMode` and must move to `PreMultiplied`.
  It is not listed in the `v30.0.2` backports.
- Xilem's `wgpu` version: `wgpu = "28.0.0"` in `Cargo.toml` at `dc496876`.
  Open [#1866](https://github.com/linebender/xilem/pull/1866) (tomcur, 2026-11-08) bumps to `wgpu` 30.0.1, which does not contain the correction either.
- Masonry code after the correction: with Metal advertising `PreMultiplied`, the existing `else if` branch in `RenderContext::create_surface` selects `PreMultiplied` and the premultiplying blitter, which is what the fork commit does by hand.
  So once Xilem moves to a `wgpu` release with #9922, the fork commit can be dropped without a Xilem change.
  This is from reading the code; not tested.
- Caveat for that future upgrade: Nico Burns reported that "Vello Classic outputs straight alpha, and Vello Hybrid outputs premultipled alpha" ([#vello > Accounting for PreMultipled vs PostMultiplied alpha mode](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/Accounting.20for.20PreMultipled.20vs.20PostMultiplied.20alpha.20mode/near/611052316), 2026-07-16).
  Whether Masonry's blitter choice is right for the `imaging_vello_hybrid` backend: not checked.
- Whether the fork commit still applies: `git merge-tree --write-tree --merge-base=881d2dae^ upstream/main 881d2dae` reports no conflict on 2026-11-17, although `dc496876` (#1851) changed imports in the same file.
- History of this code: `masonry_winit/src/vello_util.rs` was added in [#1393](https://github.com/linebender/xilem/pull/1393) (RagibHasin, 2025-09-23) as a copy of `vello::util` with transparent-surface handling; DJMcNab's review asked for alpha mode selection from `SurfaceCapabilities::alpha_modes`, and the discussion was in [#masonry > Transparency woes](https://xi.zulipchat.com/#narrow/channel/317477-masonry/topic/Transparency.20woes).
- Xilem issue: [#1852](https://github.com/linebender/xilem/issues/1852) (opened and closed by the owner on 2026-09-30).
  The issue text says it was AI-generated and edited, and the closing comment says it was written with assistance from an LLM.
  Under the current [LLM policy](https://linebender.org/wiki/llm-policy/), AI-generated analyses in GitHub comments are to be avoided even when vetted, so later posts on this topic need to be the owner's own text.

## Options

Maintained by the agent.

1. Keep the fork commit until Xilem depends on a `wgpu` release that contains #9922, then drop it.
   No pull request.
2. Propose the Metal special case to Xilem behind a comment that names gfx-rs/wgpu#9922.
   The fact that the upstream correction is merged, and that the existing `PreMultiplied` branch already handles the corrected capabilities, makes this unlikely to be wanted.

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
