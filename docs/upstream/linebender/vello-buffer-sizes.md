---
item: 9
upstream_repository: linebender/vello
base_branch: main
head_repository: eliheuer/vello
head_branch:
state: research
owner_decided_design: no
owner_reviewed_commit:
owner_wrote_text: no
llm_disclosure: undecided
pull_request:
---

# Vello: configurable or dynamic bump buffer sizes

## Problem

Vello allocates seven bump buffers at fixed sizes for any scene.
In a Runebender window this is 166 MB of GPU memory.
A further 168 MB sits in 42 Metal blocks of 4 MB whose cause is not known.

## Upstream context

Maintained by the agent.
Mark anything that was not checked as "not checked".

- Code: `vello_encoding/src/config.rs`, sizes `1 << 21` and `1 << 23` elements, read in version 0.8.0 from the Cargo registry. Current Vello `main`: not checked.
- Whether `imaging_vello` or Vello expose a size option on `main`: not checked.
- Related issues, pull requests and Zulip threads: not checked. Dynamic buffer sizing is a long-standing Vello topic, so earlier design work almost certainly exists. Read it before proposing anything.
- The cause of the 168 MB in Metal blocks: not known. The present path in the fork (`masonry_winit/src/event_loop_runner.rs`) is the first place to look.
- Checks upstream runs: not checked.

## Options

Maintained by the agent.

1. Take part in the existing upstream design, if one is in progress.
2. A size option in the renderer configuration, with the current sizes as the default.
This item is last in the order, and it needs a discussion with the Vello maintainers before any code.

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
