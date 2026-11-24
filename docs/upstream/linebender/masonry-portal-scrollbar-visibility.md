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

Runebender needs scroll areas that scroll with the wheel, keyboard and accessibility actions but show no scrollbars.
Masonry `Portal` shows a scrollbar on an axis whenever the content overflows and that axis is not constrained, and there is no public way to hide a visible scrollbar.
Constraining an axis hides its scrollbar, but it also fits the content to the viewport and ignores scroll deltas on that axis, so it is not a way to hide the bar while keeping scrolling.
Runebender carries a wrapper that moves the scrollbar widgets outside the clip after layout: `src/application/widgets/scroll_viewport.rs`, 389 lines.
Its module comment also states that a zero-size `ScrollBar` still paints its cursor rectangle; that was not reproduced for this file.

## Upstream context

Maintained by the agent.
Every claim has a link, or a file path at a named commit.
Mark anything that was not checked as "not checked".

- Upstream commit read: `dc496876` (2026-11-03).
- Code: `masonry/src/widgets/portal.rs`.
  In `layout`, `scrollbar_horizontal_visible = !constrain_horizontal && size.width < content_size.width` (and the same for vertical); a scrollbar that is not visible is stashed with `ctx.set_stashed`, so it is not laid out or painted.
  In `on_pointer_event`, scroll deltas on a constrained axis are set to zero ([#1583](https://github.com/linebender/xilem/pull/1583), "Disable pointer scroll in `Portal` when constrained").
- Does a hidden scrollbar still paint: a scrollbar that `Portal` hides is stashed and does not paint.
  A visible scrollbar paints in `ScrollBar::paint` (`masonry/src/widgets/scroll_bar.rs`) with the theme constants `SCROLLBAR_COLOR` and `SCROLLBAR_BORDER_COLOR`; no color property exists.
  What a zero-size visible scrollbar paints: not checked by rendering.
- Existing visibility API:
  - `Portal` has no public visibility policy.
    Public items are `constrain_horizontal`, `constrain_vertical`, `content_must_fill`, the matching setters, `horizontal_scrollbar_mut`, `vertical_scrollbar_mut` and the viewport methods.
  - The `AutoHideScrollBar` property on `Portal` (`masonry/src/properties/portal.rs`, [#1773](https://github.com/linebender/xilem/pull/1773), RagibHasin, merged 2026-05-15) fades the scrollbars out after the pointer stops and fades them in on pointer movement.
    The `Collapsible` property on `ScrollBar` ([#1771](https://github.com/linebender/xilem/pull/1771), merged 2026-05-08) draws a thinner bar until hovered.
    Both default to `false`.
    Neither keeps the bar hidden.
  - From Xilem, `portal(child).prop(AutoHideScrollBar(true))` works through the generic `WidgetView::prop` method (`xilem_masonry/src/widget_view.rs`), because `Portal` implements `UsesProperty<AutoHideScrollBar>`.
  - The Xilem `portal` view (`xilem_masonry/src/view/portal.rs`) has `constrain_horizontal`, `constrain_vertical` and `must_fill`, added in [#1672](https://github.com/linebender/xilem/pull/1672).
    It has no scrollbar visibility option.
- Related issues and pull requests:
  - [#1344](https://github.com/linebender/xilem/issues/1344) "Tracking issue for Portal/Scroll widget" (open, 2025-08-15, PoignardAzur): lists a rename to `Scroll`, removing the type parameter and a refactor of the scrollbar logic.
    The owner asked there on 2025-11-02 about `portal(content).scrollbar_visible(false)`; ricvelozo replied "It need to be implemented".
  - [#1073](https://github.com/linebender/xilem/issues/1073) and [#1063](https://github.com/linebender/xilem/issues/1063) (open): Portal layout defects, not visibility.
  - No open pull request adds a visibility policy (open pull request list on 2026-11-17).
- Earlier discussion on Zulip:
  - [#xilem > How should portal/scrollbar styling work?](https://xi.zulipchat.com/#narrow/channel/354396-xilem/topic/How.20should.20portal.2Fscrollbar.20styling.20work.3F/near/553500724) (2025-11-02 to 2025-11-03): the owner asked for a visibility toggle; ricvelozo pointed at the computed visibility fields and suggested a second field for the preference; Olivier FAURE wrote that "a PR that lets you hide scrollbars would be an easy win".
  - [#masonry > Scrollbar niceties](https://xi.zulipchat.com/#narrow/channel/317477-masonry/topic/Scrollbar.20niceties/near/590843578) (2026-04-25 to 2026-04-28): RagibHasin's plan that became #1771 and #1773.
- Statements by maintainers about the direction of this area: the Zulip message above supports a hide option. #1344 says the Portal code is "some of the oldest and most cursed code in Masonry" and plans a rename and refactor, so a new option can conflict with that later work.
- Snapshot tests: `portal.rs` has render snapshots (`masonry/screenshots/portal_*`).
  An option with an unchanged default leaves them unchanged.
- Checks upstream runs on a pull request: see [What every Xilem pull request needs](README.md#what-every-xilem-pull-request-needs).
- Changelog or documentation expected in the same pull request: no `CHANGELOG.md`; doc comments on the new items.
- Runebender cleanup noted earlier: the wrapper's `constrain_vertical` field has no setter in `scroll_viewport.rs`, so it is always `false`.
  Delete it either way.

## Options

Maintained by the agent.

1. A visibility policy per axis on `Portal` (for example automatic, always, never) and the matching option on the Xilem `portal` view.
   The `never` value keeps the scrollbar stashed while scrolling stays enabled.
2. A property, in the style of `AutoHideScrollBar`, that keeps the scrollbars hidden.
   This fits how #1771 and #1773 added scrollbar behavior and needs no new Xilem method, because `.prop(...)` already works.
3. Two booleans that hide each bar.
   Smaller and less general.

The Zulip reply from Olivier FAURE favours a pull request over a discussion; the choice between a builder option and a property is the owner's to raise in the pull request or on Zulip.

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
