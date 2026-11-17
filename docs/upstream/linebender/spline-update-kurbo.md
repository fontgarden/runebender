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

`spline` depends on `kurbo` 0.9.5.
Runebender uses `spline` for hyperbezier curves and the current `kurbo` for everything else, so it builds two `kurbo` versions and converts between them in `src/outline/path/hyper.rs`.
`Cargo.toml` pins `spline` at revision `83f46cbf` and lists the old version as `kurbo_09`.

## Upstream context

Maintained by the agent.
Mark anything that was not checked as "not checked".

- Upstream commit read: not checked. Runebender pins `83f46cbf`.
- Whether `linebender/spline` `main` already uses a newer `kurbo`: not checked. Check this first; the item may need no pull request, only a new pinned revision.
- Whether the repository is maintained and accepts pull requests: not checked.
- A fork `eliheuer/spline` does not exist yet as far as this file knows: not checked.
- Related issues and pull requests: not checked.
- Checks upstream runs: not checked.

## Options

Maintained by the agent.

1. Update the `kurbo` dependency and correct the API differences. A dependency update with no behavior change.
2. If upstream is not maintained, ask on Zulip who owns it before doing the work.

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
