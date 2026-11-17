---
item: 2
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

# Masonry: concentric rounded borders

## Problem

A rounded border in Masonry is stroked at the full corner radius, so at a large border width the inner and outer edges of the border are not concentric.
The Runebender fork carries a correction in commit `0b0b5e75`: the stroke uses the corner radius minus half the border width, and the fill extends to the centerline.

## Upstream context

Maintained by the agent.
Mark anything that was not checked as "not checked".

- Upstream commit read: the fork base `271a27a6`. Current `main` (`dc496876`): not checked.
- Code in the fork commit: `masonry_core/src/core/widget_paint.rs` and `masonry_core/src/properties/border_width.rs`, 19 lines changed.
- Whether upstream changed border painting after `271a27a6`: not checked.
- Related issues and pull requests: not checked.
- Screenshot tests upstream that this changes: not checked. Masonry has snapshot tests, and a painting change probably needs updated snapshots.
- Checks upstream runs on a pull request (`.github/workflows/ci.yml` at `dc496876`): `cargo fmt --all --check`, `taplo fmt --check`, `cargo rdme --check` for each crate, Clippy, tests with `cargo nextest` plus `cargo test --doc`, and a `typos` configuration is present. Read the workflow file for the exact commands before running them.
- Changelog or documentation expected in the same pull request: not checked.
- Earlier discussion on Zulip: not checked.
- Statements by maintainers about the direction of this area: not checked.

## Options

Maintained by the agent.

1. Propose the fork commit, rebased on `main`, with a before and after picture and updated snapshots.
2. If upstream prefers the current look as the default, propose it as an option.

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
