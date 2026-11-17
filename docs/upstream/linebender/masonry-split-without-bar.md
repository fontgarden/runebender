---
item: 4
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

# Masonry: a `Split` that paints no bar

## Problem

Runebender needs a splitter that keeps pointer, keyboard, focus and accessibility behavior and paints nothing, because the panels draw their own edges.
Masonry `Split` always paints a bar.
Runebender carries a copy of `Split` with the painting removed: `src/application/widgets/quiet_split.rs` and `quiet_split_widget.rs`, 791 lines, copied from upstream revision `b81d8d7`.

## Upstream context

Maintained by the agent.
Mark anything that was not checked as "not checked".

- Upstream commit read: `dc496876` (2026-11-03).
- Code: `masonry/src/widgets/split.rs`. It has a `solid: bool` field with `solid_bar(bool)`, and the painters `paint_focus_bar`, `paint_solid_bar` and `paint_stroked_bar`. No option paints nothing.
- Whether a transparent color through a property already gives an invisible bar: not checked. Check this first; it may need no pull request.
- Differences between the Runebender copy (from `b81d8d7`) and current `Split`: not checked.
- The Xilem view for `Split` and where its options live: not checked.
- Related issues and pull requests: not checked.
- Checks upstream runs on a pull request (`.github/workflows/ci.yml` at `dc496876`): `cargo fmt --all --check`, `taplo fmt --check`, `cargo rdme --check` for each crate, Clippy, tests with `cargo nextest` plus `cargo test --doc`, and a `typos` configuration is present. Read the workflow file for the exact commands before running them.
- Changelog or documentation expected in the same pull request: not checked.
- Earlier discussion on Zulip: not checked.
- Statements by maintainers about the direction of this area: not checked.

## Options

Maintained by the agent.

1. A builder method and setter that turns bar painting off.
2. A bar color property, where a transparent color gives the same result. This may fit the property system better.
3. No change, if an existing property already hides the bar.

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
