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
It builds a `widgets::Prose` around a `TextArea` with the interface font and puts `ContentColor` on the inner `TextArea`, as the upstream view does.

## Upstream context

Maintained by the agent.
Every claim has a link, or a file path at a named commit.
Mark anything that was not checked as "not checked".

- Upstream commit read: `dc496876` (2026-11-03).
- Code: `xilem_masonry/src/view/prose.rs`.
  The setters on `Prose` are `text_color`, `text_alignment`, `text_size`, `line_break_mode` and `weight`.
  No font setter exists, and `build` sets no `FontFamily` style.
- Font setters on other views, all with the same signature `pub fn font(mut self, font: impl Into<FontFamily<'static>>) -> Self`: `label` (`xilem_masonry/src/view/label.rs`), `text_input`, `checkbox` and `variable_label`.
  Each defaults to `FontFamily::Single(FontFamilyName::Generic(GenericFamily::SystemUi))` ([#1479](https://github.com/linebender/xilem/pull/1479) for `label`, [#1831](https://github.com/linebender/xilem/pull/1831) for `checkbox` and `text_input`).
  `prose` was not part of either change, so it uses the `TextArea` default family.
- History of `prose.rs`: moved into `xilem_masonry` in [#1503](https://github.com/linebender/xilem/pull/1503) (2025-12-02); later commits are refactors (#1515, #1666, #1667, #1691, #1712).
  `weight` was added in [#917](https://github.com/linebender/xilem/pull/917) (2025-05-08), whose author added only the one setter needed.
- Related issues and pull requests:
  - [#1795](https://github.com/linebender/xilem/issues/1795) "Consistent API between `Label` and `Prose`" (open, 2026-05-14, BigWingBeat).
    Its table lists `font` as present on `Label` and missing on `Prose`, with `enable_hinting`, `line_height`, `letter_spacing`, `word_spacing` and `color`.
    RagibHasin and waywardmonkeys discussed attributed text in Parley (parley#495, parley#631).
    PoignardAzur replied on 2026-05-28 that they "should go for a 'worse is better' approach" toward feature parity and that he would look into it.
    No pull request is linked to the issue on 2026-11-17.
  - [#1230](https://github.com/linebender/xilem/pull/1230) "DRAFT: Add text style properties" (open draft, PoignardAzur, 2025-07-24): paused because, per its description, text styling through properties may not be the right direction.
  - Searches for "prose font", "prose", "Prose color", "font family" and "FontFamily" on 2026-11-17 found no other pull request.
- New on `main`, found while reading for this item: #1857 (merged 2026-11-03) made `widgets::Prose` copy its own resolved `ContentColor` into the inner `TextArea` on `Update::WidgetAdded` and in `property_changed`, and added a theme default `ContentColor::new(TEXT_COLOR)` for `Prose` (`masonry/src/widgets/prose.rs`, `masonry/src/theme.rs`).
  The Xilem `prose` view still puts `text_color` on the inner `TextArea` in `build`.
  Reading the code, the `WidgetAdded` copy overwrites that color with the theme default, so `prose(...).text_color(c)` shows the default color until `c` changes on a rebuild.
  This is not tested.
  Upstream examples use the pattern (`xilem/examples/http_cats.rs`, `variable_clock.rs`, `emoji_picker.rs`).
  Runebender's `selectable_text.rs` uses the same pattern and would be affected when Runebender moves its pin past `dc496876`.
- Earlier discussion on Zulip: searches for "Prose font" and "font family prose" found nothing relevant.
- Statements by maintainers about the direction of this area: PoignardAzur's reply in #1795 favours parity between `Label` and `Prose` now; RagibHasin and waywardmonkeys expect a larger rework once attributed text lands in Parley.
- Checks upstream runs on a pull request: see [What every Xilem pull request needs](README.md#what-every-xilem-pull-request-needs).
- Changelog or documentation expected in the same pull request: no `CHANGELOG.md`; a doc comment on the setter, copied in form from `label`.

## Options

Maintained by the agent.

1. Add `font` to `Prose` with the same name, argument type, default and rebuild handling as `label`.
   Choosing the `SystemUi` default changes the default font of every `prose` view, which matches #1479 and #1831 but is a visible change and may change snapshots.
   Choosing "no family unless set" keeps today's output.
2. Comment on #1795 first, in the owner's own words, because PoignardAzur said he would look into parity.
   A pull request that adds only `font` partly overlaps that work; one that adds all missing setters from the #1795 table is larger.
3. Report the `text_color` effect of #1857 on the `prose` view separately, after the owner confirms it by running an example.
   If confirmed, it is a small defect with the same shape as item 6 and could go first.

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
