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
The placeholder label of the input also does not use the font family, size or weight of the input.
Runebender changes the color with the theme and uses its own interface font, so it carries a workaround: `src/application/widgets/input_typography.rs`, 450 lines, of which 290 are tests.

## Upstream context

Maintained by the agent.
Every claim has a link, or a file path at a named commit.
Mark anything that was not checked as "not checked".

- Upstream commit read: `dc496876` (2026-11-03).
- Code path on build, `xilem_masonry/src/view/text_input.rs`: `build` puts `ContentColor { color }` in a `PropertySet` and passes it to the inner `TextArea` with `NewWidget::new(text_area).with_props(props)`.
  The inner `TextArea` paints with `props.get::<ContentColor>(cache)` (`masonry/src/widgets/text_area.rs`).
  The first color is correct.
- Code path on rebuild: `rebuild` calls `element.insert_prop(ContentColor { color })` or `element.remove_prop::<ContentColor>()` on the outer `widgets::TextInput`, under `// TODO - Replace this with properties on the TextInput view`.
- Does the cascade from [#1691](https://github.com/linebender/xilem/pull/1691) carry the outer property to the inner `TextArea`: no.
  `PropertiesRef::get` (`masonry_core/src/core/properties_ref.rs`) looks in the widget's own local properties, then its own property stack (`stack.resolve` with its own `class_set`), then the default properties for its own widget type, then `Property::static_default()`.
  `MutateCtx::get_mut` and `QueryCtx::get` (`masonry_core/src/core/contexts.rs`) build a child's `PropertiesRef` from the child's local set and the child's stack only.
  No step reads the parent's properties.
  `masonry_core/src/doc/masonry_concepts.md` ("Property fallback") documents the same three layers.
- Does Masonry forward it by hand: no.
  `widgets::TextInput::property_changed` and its `Update::WidgetAdded` handler (`masonry/src/widgets/text_input.rs`, under `// FIXME - Find more elegant way to propagate property to child.`) copy `CaretColor` and `SelectionColor` to the inner `TextArea` and `PlaceholderColor` to the placeholder `Label` as `ContentColor`.
  They do not copy `ContentColor`, and `TextInput` does not implement `UsesProperty<ContentColor>`.
- What happens on rebuild, then: the new color is stored in the outer `TextInput`'s local properties and nothing reads it.
  The inner `TextArea` keeps the color from `build`.
  Removing the color (`None`) has no effect either.
  This is from reading the code; no upstream test covers a color change on rebuild.
- Precedent for the correction in Masonry: [#1857](https://github.com/linebender/xilem/pull/1857) (qrilka, merged 2026-11-03, now on `main`) added exactly this forwarding of `ContentColor` to `widgets::Prose`, plus a theme default `ContentColor` for `Prose`.
  The Xilem `prose` view was not changed in that pull request; see [xilem-prose-font-family.md](xilem-prose-font-family.md) for a side effect on that view.
- Placeholder font: `widgets::TextInput::with_placeholder` builds the placeholder as `Label::new(text).with_text_alignment(...)`, with no font styles.
  The Xilem view sends `font`, `text_size` and `weight` only to the inner `TextArea` (`build` with `with_style`, `rebuild` with `TextArea::insert_style`).
  So the placeholder uses the `Label` defaults.
  Still a problem on `main`.
- History: `ContentColor` came from [#1295](https://github.com/linebender/xilem/pull/1295) (2025-08-07).
  The view lines were last touched by #1691 (2026-04-02) and [#1667](https://github.com/linebender/xilem/pull/1667) (2026-02-24).
  The default font of the view became `SystemUi` in [#1831](https://github.com/linebender/xilem/pull/1831) (2026-07-03).
  The forwarding code in `widgets::TextInput` dates from [#1228](https://github.com/linebender/xilem/pull/1228) (placeholder, 2025-07-23).
- Related issues and pull requests:
  - [#1342](https://github.com/linebender/xilem/issues/1342) "Tracking issue for improving Properties" (open, 2025-08-15, PoignardAzur) names this class of problem: setting a text property on a wrapper "doesn't change the child's values by default", and proposes inheritable text properties.
    DJMcNab and Philipp-M replied that inheritance belongs at a higher level such as Xilem's environment system.
  - [#1504](https://github.com/linebender/xilem/pull/1504), the owner's merged "Fix `TextInput` placeholder text alignment".
  - [#1832](https://github.com/linebender/xilem/pull/1832) (merged 2026-08-10) and [#1826](https://github.com/linebender/xilem/pull/1826) (merged 2026-07-28) touched `TextInput` without changing color handling.
  - Searches on 2026-11-17 for "TextInput text_color", "text_input color rebuild", "ContentColor TextInput", "TextInput ContentColor inner", "text input color not updating", "placeholder font", "TextInput placeholder style", "propagate property to child", "TextInput", "text input", "placeholder" and "ContentColor" found no report of this defect.
- Earlier discussion on Zulip: Olivier FAURE wrote on 2026-01-20 that the new fallback system has "no property inheritance" ([#masonry > Plan for property fallback](https://xi.zulipchat.com/#narrow/channel/317477-masonry/topic/Plan.20for.20property.20fallback/near/568998707)), and on 2026-03-18 that better update propagation would help add inherited properties ([#masonry > Masonry property fallback: current state](https://xi.zulipchat.com/#narrow/channel/317477-masonry/topic/Masonry.20property.20fallback.3A.20current.20state/near/580203298)).
  No thread on this defect was found.
- Tests upstream: the Xilem crates have no view-level tests (`git grep TestHarness` finds none in `xilem` or `xilem_masonry`).
  A Masonry-level test can set `ContentColor` on a `widgets::TextInput` after creation and compare a render snapshot, as `masonry/src/widgets/prose.rs` does since #1857.
- Checks upstream runs on a pull request: see [What every Xilem pull request needs](README.md#what-every-xilem-pull-request-needs).
- Changelog or documentation expected in the same pull request: no `CHANGELOG.md`.
  The `text_color` doc comment says the value "overwrites the default `ContentColor` property for the inner `TextArea` widget", which describes the `build` behavior.

## Options

Maintained by the agent.

1. Xilem only: in `rebuild`, set or remove the property on `widgets::TextInput::text_mut(&mut element)`, as `build` does and as the `prose` view's `rebuild` does.
   Smallest change.
   A maintainer could ask for the `TODO` to be resolved instead.
2. Masonry and Xilem, following #1857: make `widgets::TextInput` forward `ContentColor` to the inner `TextArea` in `property_changed` and `Update::WidgetAdded`, add a theme default, implement `UsesProperty<ContentColor>`, and make `text_color` a property on the outer element in both `build` and `rebuild`.
   This matches the newest upstream pattern.
   It must move `build` to the outer element too, because the forwarding on `WidgetAdded` would otherwise overwrite the inner color from `build`.
3. The placeholder font is a separate change: apply `font`, `text_size` and `weight` to the placeholder `Label` in `build` and `rebuild` (Xilem), or forward the styles in `widgets::TextInput` (Masonry).
   It can be a second pull request.

This is a defect with a test and no new API in option 1, so it is still the best first pull request.

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
