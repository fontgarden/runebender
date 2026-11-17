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

`masonry_winit` configures the surface with `desired_maximum_frame_latency: 2`.
A Runebender window then keeps three drawables, 52 MB.
A value of 1 is expected to save about 18 MB; this is not measured yet.

## Upstream context

Maintained by the agent.
Mark anything that was not checked as "not checked".

- Upstream commit read: `dc496876`. `masonry_winit/src/vello_util.rs` line 199 sets the value 2.
- Why upstream chose 2: not checked. Read `git blame` and the pull request that set it.
- Measured effect of the value 1 on memory and on frame pacing: not measured. Item M3 in `docs/CLEANUP-PLAN.md` is the trial.
- Related issues and pull requests: not checked.
- Checks upstream runs on a pull request (`.github/workflows/ci.yml` at `dc496876`): `cargo fmt --all --check`, `taplo fmt --check`, `cargo rdme --check` for each crate, Clippy, tests with `cargo nextest` plus `cargo test --doc`, and a `typos` configuration is present. Read the workflow file for the exact commands before running them.
- Changelog or documentation expected in the same pull request: not checked.
- Earlier discussion on Zulip: not checked.
- Statements by maintainers about the direction of this area: not checked.

## Options

Maintained by the agent.

1. A window option for the latency, default unchanged.
2. No pull request, if the trial shows stutter or no saving.

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
