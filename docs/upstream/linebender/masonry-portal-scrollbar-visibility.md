---
item: 5
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

# Masonry: a scrollbar visibility policy on `Portal`

## Problem

Runebender needs scroll areas with no visible scrollbars.
Masonry `Portal` decides scrollbar visibility itself, and a zero-size `ScrollBar` still paints.
Runebender carries a wrapper that moves the scrollbar widgets outside the clip after layout: `src/application/widgets/scroll_viewport.rs`, 389 lines.

## Upstream context

Maintained by the agent.
Mark anything that was not checked as "not checked".

- Upstream commit read: `dc496876` (2026-11-03).
- Code: `masonry/src/widgets/portal.rs`. Private fields `scrollbar_horizontal_visible` and `scrollbar_vertical_visible` are computed in layout. Public `horizontal_scrollbar_mut` and `vertical_scrollbar_mut` exist. No public visibility policy was found.
- The Xilem `portal` view and its options: not checked.
- Whether a zero-size `ScrollBar` still paints on `main`: not checked.
- Related issues and pull requests: not checked. Scrollbar behavior is a likely topic of earlier discussion; search issues and Zulip before proposing API.
- Checks upstream runs on a pull request (`.github/workflows/ci.yml` at `dc496876`): `cargo fmt --all --check`, `taplo fmt --check`, `cargo rdme --check` for each crate, Clippy, tests with `cargo nextest` plus `cargo test --doc`, and a `typos` configuration is present. Read the workflow file for the exact commands before running them.
- Changelog or documentation expected in the same pull request: not checked.
- Earlier discussion on Zulip: not checked.
- Statements by maintainers about the direction of this area: not checked.

## Options

Maintained by the agent.

1. An enum policy on `Portal` for each axis (automatic, always, never) and the matching Xilem view option.
2. Two booleans that hide each bar. Smaller, less general.

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
