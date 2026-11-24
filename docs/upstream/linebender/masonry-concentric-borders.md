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

Masonry strokes a rounded border along a rectangle inset by half the border width, but keeps the full corner radius for that inset rectangle (`BorderWidth::border_rect` in `masonry_core/src/properties/border_width.rs` at `dc496876`).
For a corner radius `r` and border width `w`, the stroke's centerline then has radius `r`, its outer edge `r + w/2` and its inner edge `r - w/2`.
The box shadow uses `r` for the same box (`paint_box_shadow` in `masonry_core/src/core/widget_paint.rs`), and `bg_rect` rounds the background at `r - w`.
So the border corners are not concentric with the box outline or with the background, and the difference grows with `w`.
The fork commit message describes the visible result as a corner that looks heavier than the straight edges, with a sliver of the ground between the stroke and the background.
The Runebender fork carries a correction in commit `0b0b5e75` (16 added and 3 removed lines in two files): `border_rect` uses `radius - width / 2` (clamped at zero), and `paint_background` fills to the border centerline instead of the inner edge when the border width is above zero.

## Upstream context

Maintained by the agent.
Every claim has a link, or a file path at a named commit.
Mark anything that was not checked as "not checked".

- Upstream commit read: `dc496876` (2026-11-03).
- Code: `masonry_core/src/properties/border_width.rs` (`bg_rect`, `border_rect`) and `masonry_core/src/core/widget_paint.rs` (`paint_background`, `paint_border`).
  `paint_border` strokes `border_rect` with `Join::Miter`.
- Whether upstream changed border painting after `271a27a6`: no.
  `git diff 271a27a6 upstream/main` is empty for both files.
- Whether the fork commit still applies: `git merge-tree --write-tree --merge-base=0b0b5e75^ upstream/main 0b0b5e75` reports no conflict on 2026-11-17.
- History of this code: border painting moved into Masonry Core in [#1607](https://github.com/linebender/xilem/pull/1607) and border layout in [#1636](https://github.com/linebender/xilem/pull/1636) (both by xStrom, January 2026).
  [#1697](https://github.com/linebender/xilem/pull/1697) stopped the radio button from stroking its border twice.
- Upstream TODO on this exact code: `paint_background` carries `TODO: Fix remaining issues, see https://github.com/linebender/xilem/issues/1592`, and says the background should go "exactly to the outer border and not beyond".
- Related issues and pull requests:
  - [#1592](https://github.com/linebender/xilem/issues/1592) "`Background` tracking issue" (open, 2026-01-21, xStrom): plans to extend the background to the outer edge of the border once border styles such as dashed exist, possibly by drawing a thicker border clipped to size.
    The fork's change to the background rectangle touches the same plan.
  - [#869](https://github.com/linebender/xilem/issues/869) "Blurry 1px-wide borders on Windows" (open, 2025-02-15): about pixel alignment, not corner geometry.
  - [#1555](https://github.com/linebender/xilem/pull/1555) (closed, 2026-01-06): per-side border widths; closed by its author.
  - Searches for "border radius", "rounded border", "border_rect", "concentric", "border corner" and "border" on 2026-11-17 found no issue about corner concentricity.
- Snapshot tests: the default theme gives `Button`, `Checkbox`, `Switch`, `SelectorButton`, `TextInput`, `ProgressBar`, `RadioButton` and `StepInput` a 1 px border and, except `RadioButton`, a nonzero `CornerRadius` (`masonry/src/theme.rs` at `dc496876`).
  At least 35 of the 218 reference images under `*/screenshots/` are of these widgets (`button_*`, `checkbox_*`, `progress_bar_*`, `radio_button_*`, `step_input_*`, `switch_*`, `text_input_*`), and the example screenshots also show them.
  Since [#1839](https://github.com/linebender/xilem/pull/1839) (2026-08-25) screenshot comparison is exact, so any changed pixel fails a test.
  The exact list of changed images: not checked, because cargo was not run.
- Earlier discussion on Zulip: searches for "border radius" and "concentric" found no thread on this topic.
  [#masonry > Handling generic box properties in core](https://xi.zulipchat.com/#narrow/channel/317477-masonry/topic/Handling.20generic.20box.20properties.20in.20core) (2026-01-20) records the plan that moved box properties into Masonry Core.
- Statements by maintainers about the direction of this area: xStrom's comment in #1592 and the `TODO` above.
  Nothing found on corner radius geometry.
- Checks upstream runs on a pull request: see [What every Xilem pull request needs](README.md#what-every-xilem-pull-request-needs).
  Regenerate the changed references with `MASONRY_TEST_BLESS=1` (`masonry_testing/README.md`) and inspect each one.
- Changelog or documentation expected in the same pull request: no `CHANGELOG.md` exists.
  The doc comment on `border_rect` changes with the code.

## Options

Maintained by the agent.

1. Propose only the `border_rect` radius change (`radius - width / 2`), with a before and after picture at a large border width and the updated snapshots.
   This is a geometry correction with a small diff.
2. Propose both parts of the fork commit.
   The background change (fill to the centerline) is the part a maintainer is most likely to question: with a translucent `BorderColor`, the background then shows under the inner half of the border, and #1592 plans a different end state for the background rectangle.
3. Raise the background part in #1592 first, in the owner's own words, and send the radius correction alone.

The snapshot diff will be large even for a small code change.

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
