---
item: 6
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

# Xilem: `TextInput` text color has no effect after the first build

## Problem

The Xilem `text_input` view accepts a text color.
The color works on the first build and has no effect when it changes on a later rebuild.
Runebender changes the color with the theme, so it carries a workaround: `src/application/widgets/input_typography.rs`, 450 lines, of which 290 are tests.
The workaround also makes the placeholder label use the font of the input; whether that part is still needed upstream is not checked.

## Upstream context

Maintained by the agent.
Mark anything that was not checked as "not checked".

- Upstream commit read: `dc496876` (2026-11-03).
- Code: `xilem_masonry/src/view/text_input.rs`.
  In `build`, the color goes into the property set of the inner `TextArea`: `props.insert(ContentColor { color })`, then `NewWidget::new(text_area).with_props(props)`.
  In `rebuild`, the color goes to the outer `TextInput`: `element.insert_prop(ContentColor { color })` and `element.remove_prop::<ContentColor>()`.
  Both sites carry the comment `TODO - Replace this with properties on the TextInput view`.
  The doc comment on `text_color` says the value "overwrites the default `ContentColor` property for the inner `TextArea` widget".
- Where the property is read: `masonry/src/widgets/text_area.rs` reads `ContentColor` from its own properties (`props.get::<ContentColor>(cache)`).
  Whether the cascade added in #1691 makes a property on the outer `TextInput` reach the inner `TextArea` is not checked. This decides whether the defect still exists, so check it first with a test.
- History: the `ContentColor` property came from #1295 (2025-08-07, "Merge `TextColor` and `SpinnerColor` into `ContentColor`"). The lines were last touched by #1691 (2026-04-02, "Refactor properties to implement cascading") and #1667 (2026-02-24, rename to `PropertySet`).
- Related issues and pull requests: four `gh search` queries on 2026-11-17 ("TextInput text color", "text_input text_color", "ContentColor TextArea", "prose font") returned nothing. Search by hand once more before opening.
- The owner's earlier merged pull request in this area: #1504, "Fix `TextInput` placeholder text alignment".
- Checks upstream runs on a pull request (`.github/workflows/ci.yml` at `dc496876`): `cargo fmt --all --check`, `taplo fmt --check`, `cargo rdme --check` for each crate, Clippy, tests with `cargo nextest` plus `cargo test --doc`, and a `typos` configuration is present. Read the workflow file for the exact commands before running them.
- Changelog or documentation expected in the same pull request: not checked.
- Earlier discussion on Zulip: not checked.
- Statements by maintainers about the direction of this area: not checked.

## Options

Maintained by the agent.

1. Apply the color to the inner `TextArea` in `rebuild`, through `widgets::TextInput::text_mut(&mut element)`, as `build` does. Smallest change. A maintainer could ask for the `TODO` to be resolved properly instead.
2. Resolve the `TODO`: make `text_color` a property on the `TextInput` view and rely on the cascade from #1691. Larger, and needs the cascade to carry `ContentColor` from the input to the area.
3. If a test shows the cascade already makes the current code work, there is no defect: delete the Runebender workaround and close this item.

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
