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
Masonry `Split` always paints a bar, and when it is the focus target it also paints a focus rectangle.
Runebender carries a copy of `Split` with an empty `paint`: `src/application/widgets/quiet_split.rs` (233 lines) and `src/application/widgets/quiet_split_widget.rs` (558 lines), 791 lines in total, copied from upstream revision `b81d8d7a`.
Runebender already configures every splitter with `.bar_thickness(Length::ZERO)`, `.min_bar_area(...)` and `.solid_bar(false)` (`src/application/view/render.rs`).

## Upstream context

Maintained by the agent.
Every claim has a link, or a file path at a named commit.
Mark anything that was not checked as "not checked".

- Upstream commit read: `dc496876` (2026-11-03).
- Code: `masonry/src/widgets/split.rs`.
  `paint` calls `paint_solid_bar` or `paint_stroked_bar` with the color from `bar_color`, then `paint_focus_bar` when `ctx.is_focus_target() && self.draggable && !ctx.is_disabled()`.
- Can an existing property or a transparent color hide the bar: no.
  `bar_color` returns the theme constants `theme::ZYNC_500`, `ZYNC_600` or `ZYNC_700` from widget state, and `paint` ignores its `PropertiesRef` argument.
  `Split` reads no color property, so `.prop(...)` cannot change it.
- Can bar thickness zero hide it: partly, from reading the code; not rendered.
  With `bar_thickness(0)` and `solid_bar(true)`, `paint_solid_bar` fills a zero-width rectangle.
  With `solid_bar(false)`, `paint_stroked_bar` strokes two lines with `Stroke::new(0.0)`; #1204 records that a zero-width stroke is not a no-op in this stack.
  In both cases `paint_focus_bar` still strokes a 1 px rectangle (the bar rectangle inset by 2) in `theme::FOCUS_COLOR` when the splitter has focus.
- Differences between the Runebender copy and current `Split`: `masonry/src/widgets/split.rs` is unchanged between `b81d8d7a` (2026-08-28, #1844) and `dc496876` (`git diff --stat` is empty).
  The copy differs only in what Runebender changed.
- The Xilem view: `xilem_masonry/src/view/split.rs` exposes `split_axis`, `split_point`, `split_point_from_start`, `split_point_from_end`, `with_split_point`, `min_lengths`, `bar_thickness`, `min_bar_area`, `draggable` and `solid_bar`.
  A new option needs a matching setter there.
- History of this code: [#1578](https://github.com/linebender/xilem/pull/1578), [#1579](https://github.com/linebender/xilem/pull/1579), [#1580](https://github.com/linebender/xilem/pull/1580) (xStrom, 2026-01-16) cleaned up `Split` and its bar area; [#1602](https://github.com/linebender/xilem/pull/1602) (waywardmonkeys, 2026-01-23) added keyboard and accessibility operation, hover and focus visuals, and interaction tests.
- Related issues and pull requests:
  - [#1581](https://github.com/linebender/xilem/issues/1581) "`Split` bar area doesn't get exclusive pointer events" (open, 2026-01-16, xStrom): the bar area wider than the visual bar is a transparent overlay on the children, and children still get the `Down` event and the cursor icon.
    This affects Runebender's zero-thickness setup with or without a pull request.
  - Searches for "Split", "splitter" and "split bar" on 2026-11-17 found no request for an invisible bar.
- Earlier discussion on Zulip: searches for "splitter" and "Split widget" found no thread on bar visibility.
- Statements by maintainers about the direction of this area: [#1342](https://github.com/linebender/xilem/issues/1342) (open tracking issue for properties) and the Xilem helper-method convention ("in most cases we want helper methods for the properties applicable to views", DJMcNab in #1342) suggest that styling goes through properties with Xilem helpers.
- Snapshot tests: `split.rs` has two render snapshots, `split_columns` and `split_rows` (`masonry/screenshots/`), both with the default stroked bar, and an `edit_splitter` test that compares a builder-configured and a setter-configured splitter with `solid_bar(true)`.
  A new option with an unchanged default leaves the references unchanged; a test for the new option would follow the `edit_splitter` pattern.
- Checks upstream runs on a pull request: see [What every Xilem pull request needs](README.md#what-every-xilem-pull-request-needs).
- Changelog or documentation expected in the same pull request: no `CHANGELOG.md`; doc comments on the new builder, setter and Xilem method.

## Options

Maintained by the agent.

1. A builder method, `WidgetMut` setter and Xilem view method that turn bar painting off, for example a bar style with a "none" value next to solid and stroked.
   The focus indicator needs a decision: keep it (accessible, but visible) or suppress it with the bar.
2. A bar color property that `Split` reads in `paint`, where a transparent color gives an invisible bar.
   This fits the property system and also allows theming; it does not remove the focus rectangle.
3. No upstream change: `bar_thickness(0)` with `solid_bar(true)` already paints nothing while unfocused.
   Runebender would then show the 1 px focus rectangle when a splitter has keyboard focus.
   This needs a rendered check before it is relied on.

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
