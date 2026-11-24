---
item: 0
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

# Short name of the change

The header above is read by `scripts/upstream/submit_pull_request.sh`.
`state` is one of: research, decision, code, review, text, submitted, merged, dropped.
The three `owner_` fields and `llm_disclosure` are set by the owner only.
`llm_disclosure` is `included` when the description discloses LLM use, or `not-needed` when no disclosure applies.

## Problem

What is wrong or missing, as facts.
What Runebender does about it today, with the file and its size.

## Upstream context

Maintained by the agent.
Every claim has a link, or a file path at a named commit.
Mark anything that was not checked as "not checked".

- Upstream commit read:
- Code:
- History of this code (`git log`, `git blame`):
- Related issues and pull requests, open and closed:
- Earlier discussion, including Zulip:
- Statements by maintainers about the direction of this area:
- Checks upstream runs on a pull request:
- Changelog or documentation that upstream expects in the same pull request:

## Options

Maintained by the agent.
Each option: what changes, what it costs, what a maintainer could object to.

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
