---
item: 7
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

# Xilem: a font family setter on the `prose` view

## Problem

The Xilem `prose` view gives selectable, wrapping text.
It has no way to set the font family, so prose cannot use an application font.
Runebender carries a small replacement view: `src/application/widgets/selectable_text.rs`, 114 lines.

## Upstream context

Maintained by the agent.
Mark anything that was not checked as "not checked".

- Upstream commit read: `dc496876` (2026-11-03).
- Code: `xilem_masonry/src/view/prose.rs`. The setters on `Prose` are `text_color`, `text_alignment`, `text_size`, `line_break_mode` and `weight`. No font setter exists.
- The pattern to follow: `xilem_masonry/src/view/text_input.rs` has a `font` field that it passes as `StyleProperty::FontFamily(self.font.clone())`. Whether `label` has the same setter is not checked.
- History of `prose.rs`: not checked.
- Related issues and pull requests: one `gh search` query ("prose font") on 2026-11-17 returned nothing. Search by hand once more before opening.
- Checks upstream runs on a pull request (`.github/workflows/ci.yml` at `dc496876`): `cargo fmt --all --check`, `taplo fmt --check`, `cargo rdme --check` for each crate, Clippy, tests with `cargo nextest` plus `cargo test --doc`, and a `typos` configuration is present. Read the workflow file for the exact commands before running them.
- Changelog or documentation expected in the same pull request: not checked.
- Earlier discussion on Zulip: not checked.
- Statements by maintainers about the direction of this area: not checked.

## Options

Maintained by the agent.

1. Add `font` to `Prose`, with the same name, argument type and rebuild handling as the `text_input` view. A maintainer could ask for the same setter on `label` for consistency, if it is missing there.
2. Wait for a general text style mechanism upstream. Check Zulip for one before choosing option 1.

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
