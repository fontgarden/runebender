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
The Runebender fork carries commit `5a813cb5`, which passes the window system's dropped-file event through the application driver, so a font can be dropped on the window.

## Upstream context

Maintained by the agent.
Mark anything that was not checked as "not checked".

- Upstream commit read: `dc496876` (2026-11-03). A search for `DroppedFile`, `dropped_file` and `HoveredFile` in `masonry_winit` and `xilem/src` found nothing.
- Code in the fork commit: `masonry_winit/src/app_driver.rs`, `masonry_winit/src/event_loop_runner.rs`, `xilem/src/driver.rs`, `xilem/src/app.rs`.
- Related issues and pull requests: not checked. Drag and drop is a known missing feature in many toolkits; an issue and a preferred design probably exist.
- Where maintainers want file drop to live (application driver, window event, or a widget event with a position): not checked. This is a design question for upstream.
- Checks upstream runs on a pull request (`.github/workflows/ci.yml` at `dc496876`): `cargo fmt --all --check`, `taplo fmt --check`, `cargo rdme --check` for each crate, Clippy, tests with `cargo nextest` plus `cargo test --doc`, and a `typos` configuration is present. Read the workflow file for the exact commands before running them.
- Changelog or documentation expected in the same pull request: not checked.
- Earlier discussion on Zulip: not checked.
- Statements by maintainers about the direction of this area: not checked.

## Options

Maintained by the agent.

1. Propose the fork's route: an application-level callback with the paths. Smallest, and enough for Runebender.
2. A widget-level event with the pointer position, so a widget can be a drop target. Larger, and probably what a toolkit wants in the end.
Ask upstream which they want before writing code.

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
