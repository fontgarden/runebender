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

The Runebender fork of Xilem premultiplies the presented image on Metal (commit `881d2dae`), because of the alpha mode that `wgpu` reports for the surface.
The correction belongs in `wgpu`, not in Xilem.
This file tracks the workaround until a `wgpu` release contains the correction.
`gfx-rs` is not a Linebender project; read its own contribution rules before any pull request.

## Upstream context

Maintained by the agent.
Mark anything that was not checked as "not checked".

- Xilem issue with the findings: <https://github.com/linebender/xilem/issues/1852>, closed.
- The matching `wgpu` issue: not checked. Find it or decide with the owner whether to report it.
- Code in the fork commit: `masonry_winit/src/vello_util.rs`; the window option in `xilem/src/window_options.rs` and `xilem/src/window_view.rs` may belong to this commit or to another, not checked.

## Options

Maintained by the agent.

1. Keep the fork commit until `wgpu` is corrected, then drop it.
2. Propose the workaround to Xilem behind a comment that names the `wgpu` issue, if maintainers want it.

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
