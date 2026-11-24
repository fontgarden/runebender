---
item: 10
upstream_repository: linebender/spline
base_branch: main
head_repository: eliheuer/spline
head_branch:
state: research
owner_decided_design: no
owner_reviewed_commit:
owner_wrote_text: no
llm_disclosure: undecided
pull_request:
---

# `spline`: update to the current `kurbo`

## Problem

`spline` depends on `kurbo` 0.9 (`kurbo = "0.9"` in its `Cargo.toml`).
Runebender uses `spline` for hyperbezier curves and the current `kurbo` for everything else, so it builds two `kurbo` versions and converts between them in `src/outline/path/hyper.rs`.
`Cargo.toml` pins `spline` at revision `83f46cbf` and lists the old version as `kurbo_09`.

## Upstream context

Maintained by the agent.
Every claim has a link, or a file path at a named commit.
Mark anything that was not checked as "not checked".

The coordinating session checked the first six facts on 2026-11-17; this file records them without repeating the checks.

- Upstream commit read: `linebender/spline` `main` is `83f46cbf` (2023-03-14), the revision Runebender pins.
  Its `Cargo.toml` has `kurbo = "0.9"`.
  So `main` does not already use a newer `kurbo`, and a new pinned revision alone does not solve this item.
- Repository state: not archived; last push 2024-11-18 (`gh api repos/linebender/spline`).
- Merged pull requests: the most recent is [spline#27](https://github.com/linebender/spline/pull/27) (ctrlcctrlv, merged 2023-03), which moved to `kurbo` 0.9.
  Before it, [spline#24](https://github.com/linebender/spline/pull/24) "Update kurbo" (cmyr, 2021-03-28).
  Dependency updates have been accepted before.
- Open pull requests: [spline#28](https://github.com/linebender/spline/pull/28) "Add new hyperbezier math" (raphlinus, 2024-03-09), [spline#29](https://github.com/linebender/spline/pull/29) "Add Javascript testbed for parameter fitting" (draft, raphlinus, 2024-11-18) and [spline#13](https://github.com/linebender/spline/pull/13) "splinetoy + wasm" (cmyr, 2021-01-04).
- Overlap with spline#28, checked with `gh pr diff 28`: that pull request also changes `Cargo.toml`, to `kurbo = "0.11"` and `edition = "2021"`, and adds `src/hyperbezier.rs`, `src/hyperbezier_old.rs` and `src/simple_spline.rs`.
  A separate `kurbo` update conflicts with it in `Cargo.toml` and duplicates part of it.
- Related issue: [spline#17](https://github.com/linebender/spline/issues/17) "Implement appropriate kurbo ParamCurve traits" (open).
  No issue asks for a `kurbo` update.
- CI: the repository has no `.github/workflows`, so no checks run on a pull request.
- Fork: `eliheuer/spline` exists, with `main` at `83f46cbf`.
- Trial in a scratch copy (coordinating session): changing only `kurbo = "0.9"` to `"0.13"` builds and tests cleanly against `kurbo` 0.13.1.
  `cargo check --lib`, `cargo check --features serde` and `cargo test` (3 tests) pass, and no source change is needed.
  One rustc warning, "hiding a lifetime", exists before and after the change.
- Earlier discussion on Zulip:
  - [#office hours > 2025-11-20](https://xi.zulipchat.com/#narrow/channel/359642-office-hours/topic/2025-11-20/near/558269130) (2025-11-19): the owner wrote that `spline` needs to be updated to the latest `kurbo`; RagibHasin replied that he works on `spline` from time to time with Raph Levien's permission, in [RagibHasin/try_hyperbez](https://github.com/RagibHasin/try_hyperbez/); Raph Levien wrote that the hyperbezier work was "in a pretty stuck state" and that `spline` `main` holds the first draft of the curve family ([message](https://xi.zulipchat.com/#narrow/channel/359642-office-hours/topic/2025-11-20/near/558274811)).
  - [#kurbo > New spline from Aurimas Gasiulis](https://xi.zulipchat.com/#narrow/channel/260979-kurbo/topic/New.20spline.20from.20Aurimas.20Gasiulis/near/640883999) (2026-11-09 to 2026-11-17): Raph Levien wrote that a hyperbezier variant of the new spline is "very much shippable as part of our hb release" ([message](https://xi.zulipchat.com/#narrow/channel/260979-kurbo/topic/New.20spline.20from.20Aurimas.20Gasiulis/near/639047415)), and offered to collaborate in the `spline` repository and release a crate under that name.
- Statements by maintainers about the direction of this area: the Linebender blog post [The mathematical beauty of hyperbezier curves](https://linebender.org/blog/hyperbezier/) (Raph Levien, 2026-08-08) says he hopes to land better implementations in the `spline` repository, without a schedule.
  Together with the Zulip messages above, this is a stated plan for new work in the repository, with no date.
- Changelog or documentation expected in the same pull request: the repository has no `CHANGELOG.md`.
  The `README.md` at `83f46cbf` does not mention `kurbo`, so no documentation changes with the version.

## Options

Maintained by the agent.

1. A one-line pull request that changes `kurbo = "0.9"` to `"0.13"`.
   It needs no source change, as the trial shows.
   It conflicts with spline#28 in `Cargo.toml`.
2. Ask Raph Levien first, in the owner's own words, on Zulip `#kurbo` or `#runebender`, whether he wants the update as its own pull request or folded into spline#28 or the coming hyperbezier release.
   This avoids a pull request that duplicates part of spline#28.
3. Keep `kurbo_09` until the hyperbezier release lands, and update Runebender to that release.

The facts above suggest option 2 first: the change is trivial, so the only real question is how it fits the maintainer's plans.

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
