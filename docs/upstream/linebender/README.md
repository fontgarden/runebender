# Linebender pull requests

This folder holds one file for each change that Runebender proposes to a Linebender project.
[OVERVIEW.md](OVERVIEW.md) lists all planned changes and the suggested order.
[TEMPLATE.md](TEMPLATE.md) is the form for a new file.

The process has one purpose: follow the [Linebender LLM policy](https://linebender.org/wiki/llm-policy/) exactly, with the least manual work for the owner.
Read the policy again before each pull request, because it can change.

## What the policy requires

- A pull request generated end to end by an LLM is not allowed.
- A pull request description written by an LLM is not allowed.
- LLM-written analysis in issues, review comments or Zulip is not allowed, even if a person checked it.
- Agent instruction files are not allowed in the pull request.
- LLM use is disclosed in the pull request description before a reviewer asks. This covers code an agent wrote, code pasted from a chat tool, generated tests, and code written by hand from a plan that an LLM proposed.
- Debugging, exploring code, running tests, and spelling or grammar checks of your own text need no disclosure.
- Do not state that a pull request has no LLM content.
- Review your own pull request with full effort before you ask anyone to read it.

## Who does what

The split below is what keeps the work inside the policy.
It is also what keeps the owner's part small: three short tasks for each pull request.

| Part | Agent | Owner |
| --- | --- | --- |
| Research: upstream code, history, existing issues and pull requests, maintainer statements | Does it, with links | Reads it |
| Design: what to change, and the options | Lists the options with facts | Decides, and writes the decision in one or two sentences |
| Code | May write a draft on a branch of the fork | Reads every line, changes what is not right, and can explain each line |
| Tests and upstream checks | Runs them and records the output | Reads the result |
| Title and description | Does not write or draft them. Corrects spelling and grammar only when asked. | Writes them, by typing or dictation |
| LLM disclosure | Keeps an exact record of what the agent did | Writes the disclosure sentence from that record |
| Opening the pull request | Does not open it | Runs the submit script, which asks for confirmation |
| Review comments and replies | Does not write them. Can explain code and run experiments. | Writes every reply |

The owner's three tasks are: decide the design, review the diff, and write the description.
Dictation counts as the owner's own writing.
An agent that receives dictated text corrects spelling and grammar and changes nothing else.

## The steps for one pull request

1. **Research.** The agent fills the "Upstream context" section of the file: the code at a named upstream commit, related issues and pull requests found with `gh`, earlier discussion, and the checks upstream runs. Each claim has a link or a file path. A claim that was not checked is marked "not checked".
2. **Decision.** The owner reads the research and writes the "Owner's decision" section. If the change adds public API, the owner asks upstream first, on Zulip or in an issue, in the owner's own words.
3. **Code.** The change is made on a branch of the fork that starts from current upstream `main`, one change for each branch. The "Code record" section lists each commit and who wrote it.
4. **Verification.** The agent runs the upstream checks and builds Runebender against the branch. The commands and results go in the "Verification" section.
5. **Owner review.** The owner reads the full diff and records the reviewed commit in the header of the file.
6. **Text.** The owner writes the title, the description and the disclosure in the "Pull request text" section.
7. **Submit.** The owner runs `scripts/upstream/submit_pull_request.sh <file>`. The script checks the approvals, shows the exact text, asks for typed confirmation, and then runs `gh pr create`.
8. **After.** The link goes in the "Record" section. Replies to reviewers are the owner's. When the pull request merges, update the pinned revision in `Cargo.toml` and delete the workaround.

## What every Xilem pull request needs

Checked on 2026-11-17 against `linebender/xilem` `main` at `dc496876`.
Read `.github/workflows/ci.yml` again before running the checks, because it changes.

CI commands, from `.github/workflows/ci.yml`:

- Formatting job: `cargo fmt --all --check`, `taplo fmt --check --diff` (Taplo 0.9.3), `bash .github/copyright.sh`, `bash .github/debug_assertions.sh`, and `cargo rdme --check --workspace-project=<crate>` (cargo-rdme 1.4.8) for `masonry`, `masonry_core`, `masonry_testing`, `masonry_winit`, `tree_arena`, `xilem`, `xilem_core`, `xilem_masonry`, `xilem_web` and `placehero`.
- Clippy on Windows, macOS and Linux: `cargo hack clippy --workspace --locked --profile ci --optional-deps --each-feature -- -D warnings`, and the same with `--tests --benches --examples`.
  Both run again with `CARGO_PROFILE_CI_DEBUG_ASSERTIONS=false`.
- Clippy for wasm32: the same two commands with `--exclude` for the non-wasm crates and `--target wasm32-unknown-unknown`.
- Tests on Windows, macOS and Linux: `cargo nextest run --workspace --locked --cargo-profile ci --all-features --no-fail-fast` with `CHECK_DOC_PATHS=true` (render tests are skipped on Windows with `SKIP_RENDER_TESTS`), `cargo nextest run -p tree_arena --locked --no-fail-fast --no-default-features`, and `cargo test --doc --workspace --locked --profile ci --all-features --no-fail-fast`.
- Other jobs: `cargo miri test -p tree_arena` on nightly, `cargo test --no-run` for wasm32, `cargo apk check` for the Android examples with `RUSTFLAGS=-D warnings`, the MSRV check `cargo hack check --workspace --ignore-private --locked --profile ci --optional-deps --each-feature` on Rust 1.96 (native and wasm32), `cargo doc --workspace --locked --profile ci --all-features --no-deps --document-private-items` on nightly with `RUSTDOCFLAGS='--cfg docsrs -D warnings'`, and `crate-ci/typos` (configuration in `.typos.toml`).
- Toolchains: stable `RUST_STABLE_VER` 1.97.1 and MSRV `RUST_MIN_VER` 1.96 (`ci.yml`); the workspace `rust-version` is 1.96 (`Cargo.toml`), and the README states the same MSRV.
  A change that raises the MSRV updates all three places, per the comment in `ci.yml`.

Conventions:

- Copyright header: every `.rs` file starts with `// Copyright <year> the Xilem Authors` and `// SPDX-License-Identifier: Apache-2.0` (`.github/copyright.sh`).
- Snapshot tests: Masonry render tests compare against PNG files in each package's `screenshots/` folder.
  `MASONRY_TEST_BLESS=1` rewrites the references (`masonry_testing/README.md`).
  Since [#1839](https://github.com/linebender/xilem/pull/1839) (2026-08-25) comparison is exact.
  CI uploads `**/screenshots/*.png` when tests fail.
- Changelog: `linebender/xilem` has no `CHANGELOG.md` at `dc496876`.
  The Linebender [release process](https://linebender.org/wiki/process/release/) asks for changelog updates in projects that have one, such as Vello.
- Lints and formatting: the Linebender [canonical lint set](https://linebender.org/wiki/canonical-lints/) and [formatting scheme](https://linebender.org/wiki/formatting-scheme/); CI denies warnings.
- Merging: commits on `main` are single commits whose subject ends in the pull request number, for example `Text widget tests (#1857)`, so pull requests are squash-merged.
  The [contributor guidelines](https://linebender.org/contributor-guidelines/) allow merge or squash and require approval by someone with write access.
- Titles of recently merged pull requests (#1850 to #1862) are short imperative sentences in sentence case.
  Some carry a crate prefix (`masonry: ...`, `xilem: ...`) or a type prefix (`fix(masonry): ...`, `perf: ...`); most have none.
  Identifiers are in backticks.
- LLM disclosure in recent merged pull requests is one plain sentence in the description, for example in [#1691](https://github.com/linebender/xilem/pull/1691), [#1821](https://github.com/linebender/xilem/pull/1821), [#1853](https://github.com/linebender/xilem/pull/1853) and [#1856](https://github.com/linebender/xilem/pull/1856).
  [#1842](https://github.com/linebender/xilem/pull/1842) states that no AI was used, which the policy asks contributors not to do.
- Agent files: the Xilem `.gitignore` lists `AGENT.md`, `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `research.md`, `plan.md`, `.mcp.json`, `.github/copilot-instructions.md` and `.codex` ([#1711](https://github.com/linebender/xilem/pull/1711)).
  A branch must not add them.
- Templates: `linebender/xilem` has no pull request or issue template at `dc496876`.
- Nontrivial changes: the [contributor guidelines](https://linebender.org/contributor-guidelines/) say to file an issue before a pull request for a nontrivial change, and to use Zulip or office hours when consensus is unclear.
  Design discussion for Xilem and Masonry is in the `#xilem` and `#masonry` Zulip channels (README of `linebender/xilem`).
- Timing: Olivier FAURE has set a deadline for a major Masonry release before 2026-11-27 ([Zulip](https://xi.zulipchat.com/#narrow/channel/317477-masonry/topic/Linebender.202026.20roadmap.20-.20Masonry/near/636301246)).

The Linebender Zulip archive is readable without an account through the web client and through `https://xi.zulipchat.com/json/messages` with a `web-public` narrow.
Its messages are not indexed by search engines (contributor guidelines).

## Rules for agents

- Do not write, draft, outline or reword the title, the description or the disclosure. The only permitted edit is a spelling or grammar correction that the owner asks for.
- Do not run `scripts/upstream/submit_pull_request.sh`, `gh pr create`, `gh pr comment`, `gh issue create` or `gh issue comment` for an upstream repository. Do not push to the fork unless the owner asks in the conversation.
- Do not set the approval fields in a file header. They belong to the owner.
- Keep the "Code record" exact. The disclosure depends on it.
- Do not copy text from the research sections into anything that goes upstream.
- Use `gh` for reading only: `gh search`, `gh issue view`, `gh pr view`, `gh pr diff`.

## How the script enforces this

`scripts/upstream/submit_pull_request.sh` refuses to continue unless all of these are true:

- it runs in an interactive terminal, so an agent's shell cannot run it;
- the header has `owner_wrote_text: yes` and `owner_decided_design: yes`;
- `owner_reviewed_commit` equals the current head commit of the branch, so a commit added after the review blocks the submission;
- the title, the description and the disclosure are present, and no template placeholder remains;
- the person at the terminal types the repository name to confirm.

The script opens a draft pull request by default.
The owner marks it ready on GitHub after one more look at the rendered page.
