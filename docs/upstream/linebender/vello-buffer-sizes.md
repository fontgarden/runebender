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

Vello Classic (the `vello` crate) allocates seven bump buffers at fixed element counts for any scene: `bin_data` `1 << 18`, `tiles`, `lines`, `seg_counts` and `segments` `1 << 21`, `blend_spill` `1 << 20`, `ptcl` `1 << 23` (`BufferSizes::new` in `vello_encoding/src/config.rs`).
In a Runebender window this is 166 MB of GPU memory.
A further 168 MB sits in 42 Metal blocks of 4 MB whose cause is not known.
Runebender renders through Masonry's default `imaging_vello` backend, which uses Vello Classic.

## Upstream context

Maintained by the agent.
Every claim has a link, or a file path at a named commit.
Mark anything that was not checked as "not checked".

- Upstream commit read: `linebender/vello` `main` at `84ffc24a` (2026-11-16), from a bare clone.
- Repository layout on `main`: [vello#1872](https://github.com/linebender/vello/pull/1872) "Move sparse renderers under renderers" (merged 2026-09-07, part of the closed proposal [vello#1871](https://github.com/linebender/vello/issues/1871)) made the sparse strips renderers (`vello_cpu`, and `vello_hybrid` renamed to `vello_gpu`) the default renderers at the root, and moved Vello Classic to `research/`.
  The crate published as `vello` is now `research/vello_research`, and `vello_encoding` is `research/vello_encoding`.
  The research `CHANGELOG.md` lists `vello` 0.12.0 (2026-11-11).
- Does `main` differ from 0.8.0 in `config.rs`: only in `bin_headers`, which [vello#1700](https://github.com/linebender/vello/pull/1700) "[classic] Support more than 256 bins" (2026-06-13) sized by the bin count.
  The seven bump sizes are unchanged, and the comment still says they "should instead get derived from the scene layout using reasonable heuristics".
- Whether Vello exposes a size option on `main`: no.
- Existing pull request on exactly this item: [vello#1861](https://github.com/linebender/vello/pull/1861) "Make Vello Classic bump-buffer reservations configurable" (open, opened 2026-08-31 by Quik2007, last updated 2026-10-30).
  It adds `BumpAllocationSizes`, `RenderConfig::new_with_bump_sizes` and `Renderer::set_bump_allocation_sizes`, keeps the current sizes as the default, and reports 165.0 MiB requested by default against 60.5 MiB for a smaller profile.
  It has no reviews and no comments, it is marked `CONFLICTING`, and it edits the pre-#1872 paths `vello/src` and `vello_encoding/src`.
  It discloses AI assistance.
- Related issues and pull requests on dynamic sizing:
  - [vello#366](https://github.com/linebender/vello/issues/366) "Strategy for robust dynamic memory, readback, and async" (open, 2023-09-28, raphlinus).
    DJMcNab linked meeting notes on 2024-04-02.
  - [vello#606](https://github.com/linebender/vello/pull/606) "Robust memory allocation handling" (open, DJMcNab, 2024-06-07): reads back the bump buffer and reallocates.
  - [vello#541](https://github.com/linebender/vello/issues/541) "Improving bump estimation accuracy and performance" (open, 2024-04-03).
  - [vello#708](https://github.com/linebender/vello/issues/708) "Make `segments` and `seg_counts` use the same buffer size." (open, 2024-09-30).
  - In [xilem#918](https://github.com/linebender/xilem/issues/918), Majora320 added up the same buffers to about 173 MB on 2025-04-09, and DJMcNab's working theory (2025-11-14) is Vello's internal buffer cache; he refers to a vello#1234 that GitHub no longer resolves.
- Masonry side: `masonry_imaging` and `xilem` have features `imaging_vello` (default), `imaging_vello_hybrid`, `imaging_vello_cpu` (Masonry Imaging only) and `imaging_skia` ([xilem#1847](https://github.com/linebender/xilem/pull/1847), merged 2026-09-14, so in the fork base).
  The hybrid backend does not use these bump buffers.
  `masonry_winit/src/vello_util.rs` requests the device with `MemoryHints::default()`.
- The 168 MB in Metal blocks: cause not known, not checked.
  Leads from upstream: Brian Craig measured about 500 MB of VRAM for both `vello` and `vello_hybrid` Masonry apps on Vulkan and about 210 MB for `vello_hybrid` with memory hints, and traced it to the `wgpu-hal` allocator's large blocks ([#vello > memory usage in vello](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/memory.20usage.20in.20vello/near/605993681), 2026-06-23).
  In the same thread Nico Burns wrote that high memory use is a known issue of the `vello` crate and "one of the primary drivers" of the move to `vello_hybrid` ([message](https://xi.zulipchat.com/#narrow/channel/197075-vello/topic/memory.20usage.20in.20vello/near/604849642)).
  Whether the Metal backend of `wgpu` 28 uses 4 MB blocks: not checked.
- Statements by maintainers about the direction of this area: development has moved to the sparse strips renderers (#1871, #1872, the Zulip message above); Vello Classic is maintained as a research renderer.
- Checks upstream runs: `.github/workflows/ci.yml` on Vello `main` runs `cargo fmt --all --check`, `taplo fmt --check --diff`, `bash .github/copyright.sh`, `cargo rdme --check` for `glifo`, `vello_common`, `vello_cpu` and `vello_gpu`, `cargo hack clippy` over each feature for native, wasm32 and no_std targets with `-D warnings`, `cargo hack check` at `RUST_MIN_VER` 1.89, `cargo test` per package with `--profile ci`, `cargo test --doc --workspace --locked --all-features`, `cargo doc --workspace --locked --all-features --no-deps --document-private-items`, `wgslfmt --check`, `cargo apk check`, WebGL and wasm tests, and a wasm size check.
  Stable Rust is 1.99.
- What Vello expects in a pull request (`CONTRIBUTING.md` and `AGENTS.md` on `main`, the latter added in [vello#2044](https://github.com/linebender/vello/pull/2044) on 2026-11-03): run `git lfs pull` first, because snapshot references are in Git LFS; prefer a `#[vello_test]` snapshot for visible output; update references with `REPLACE=1`; add an `[Unreleased]` entry in the crate's `CHANGELOG.md` for user-facing changes; keep mechanical changes, snapshot updates and behavior changes in separate commits; disclose LLM use in the description.
- Difference from the Linebender wiki: Vello's `AGENTS.md` says pull request descriptions "may be drafted with an LLM" if the author edits them, while the wiki [LLM policy](https://linebender.org/wiki/llm-policy/) does not allow AI-generated pull request descriptions.
  The rules in [README.md](README.md) follow the stricter wiki text.

## Options

Maintained by the agent.

1. No Vello pull request.
   Try `imaging_vello_hybrid` in Runebender (one Cargo feature on `xilem`, `masonry` and `masonry_winit`) and measure; this removes the Classic bump buffers entirely, and the hybrid renderer is where upstream development is.
2. Support the existing vello#1861 instead of opening a new pull request: the owner can say on that pull request or on Zulip `#vello`, in the owner's own words, that Runebender needs it and what the measurements are.
   It needs a rebase onto the `research/` layout before review.
3. A Masonry option for `wgpu` memory hints, if measurements show the allocator, not Vello, holds most of the memory.
   This is a Xilem pull request, not a Vello one, and needs measurements first.

Do not open a second pull request for configurable bump sizes while vello#1861 is open.

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
