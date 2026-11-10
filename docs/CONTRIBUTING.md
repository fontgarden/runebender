# Contributing to Runebender

Start with a focused [issue](https://github.com/fontgarden/runebender/issues) or pull request that explains the user-visible problem and the intended change.
Runebender is experimental; work on copies of important fonts and preserve unrelated changes in your checkout.
The [main README](../README.md) covers downloads and local builds, and [AGENTS.md](../AGENTS.md) contains the repository's detailed engineering rules.

## Set up a checkout

Install [Rust through rustup](https://rustup.rs/) and your platform's native build tools.
Cargo uses `rust-toolchain.toml`; do not substitute a newer compiler when checking the minimum supported version.
On macOS, install the Xcode Command Line Tools with `xcode-select --install`.
On Debian or Ubuntu, install the native dependencies:

```sh
sudo apt-get install build-essential pkg-config \
  libfontconfig-dev libwayland-dev libxkbcommon-dev libx11-dev libxcb1-dev \
  libvulkan1 libssl-dev libglib2.0-dev libgtk-3-dev libxdo-dev libasound2-dev

git clone https://github.com/fontgarden/runebender.git
cd runebender
cargo build --profile fast --locked
```

The clone and Cargo commands apply on macOS too; only the package installation is Linux-specific.
Use the [local native build instructions](../README.md#local-native-builds) to launch the editor.
See the [installation guide](https://runebender.org/docs/install.html) for Windows prerequisites and support limits; Windows desktop support is not a release guarantee.
The browser has a separate workspace and lockfile: follow [web/README.md](../web/README.md) for its tools, preparation, build, and Playwright checks.
Local models are optional for ordinary development; follow the [local AI setup guide](https://runebender.org/docs/local-ai-setup.html), [model package reference](LOCAL-MODELS.md), and [worker contract](WORKER-CONTRACT.md) rather than installing model runtimes for every test.

## Native test prerequisites

Proof and graph integration tests require `designbot` on `PATH`.
Install the same pinned renderer as [.github/workflows/ci.yml](../.github/workflows/ci.yml):

```sh
cargo install --locked --git https://github.com/eliheuer/designbot \
  --rev d723b6b891c9a9c7e5d79790738134bc2d0ce765 \
  --bin designbot designbot-cli
```

Ordinary unit tests use repository-owned synthetic fixtures and do not require a font checkout.
Tests behind `--features integration-fonts` additionally use the pinned Virtua Grotesk fixture checkout used by CI.
Keep it outside this repository, or use an existing checkout at the same revision:

```sh
git clone https://github.com/eliheuer/virtua-grotesk.git ../virtua-grotesk
git -C ../virtua-grotesk checkout 018488ed9499b7f2c18a249f5406797fc6a20e1d
export RUNEBENDER_TEST_FONTS="$(pwd)/../virtua-grotesk/sources"
```

Do not commit personal fixture paths or use font sources without their license notices.
Keep ordinary unit tests self-contained where possible, and use the real canonical model rather than a test-only reimplementation.
Ignored tests that require models, external runtimes, or interactive resources are separate checks, not passing coverage from an ordinary test run.
Read the relevant test's prerequisite before explicitly running it with `--ignored`.

## Choose checks for the change

Run focused tests during development and the relevant broader checks before submitting.
These commands cover native formatting, production and test code, the engine without the application, and documentation:

```sh
cargo fmt --all --check
bash .github/scripts/copyright.sh
cargo clippy --workspace --lib --bins --locked -- -D warnings
cargo clippy --workspace --tests --examples --benches --locked -- -D warnings
cargo clippy --workspace --tests --examples --benches --features integration-fonts --locked -- -D warnings
cargo clippy --lib --no-default-features --locked -- -D warnings
env -u RUNEBENDER_TEST_FONTS cargo test --lib --no-default-features --locked -- --test-threads=1
cargo test --workspace --features integration-fonts --locked -- --test-threads=1
cargo doc --workspace --no-deps --locked
```

CI also checks TOML formatting with `taplo fmt --check` and spelling with `typos`; tool versions and all platform-specific commands live in the [CI workflow](../.github/workflows/ci.yml).
For dependency changes, run the root and browser advisory checks described there and review both lockfiles when both workspaces are affected.
On machines with less than 24 GiB RAM, set `CARGO_BUILD_JOBS=2` and run Cargo and browser checks sequentially, not in parallel.
Shared editor changes also need the [browser build and smoke checks](../web/README.md#browser-quality-checks).
Use [headless screenshots](UI-DESIGN.md#headless-screenshots) for visual work, but do not treat screenshots as proof of native input, accessibility, or GPU behavior.
See [engine benchmarks](BENCHMARKS.md) for repeatable performance measurements rather than timing a debug build.
Report the commands actually run, their results, and any missing prerequisites; never silently skip a meaningful assertion to make a check pass.

## Architecture and source ownership

Read the [canonical architecture guide](https://runebender.org/docs/architecture.html) and the relevant module header before moving code or adding a subsystem.
These are responsibility boundaries, not a list of exclusive human maintainers:

| Area | Owns |
| --- | --- |
| `src/font` | Canonical document and source identities, layer mutations, interpolation, history, proposals, and persistence. |
| `src/outline`, `src/text`, `src/analysis` | Geometry, shaping and text behavior, and read-only measurements respectively. |
| `src/formats` | Import/export adapters and format-specific metadata interpretation. |
| `src/automation`, `src/workflows` | Guarded automation contracts and saved/live workflow execution. |
| `src/ui` | Toolkit-independent view state and layout data. |
| `src/application` | Commands, Xilem/Masonry views, platform services, and native/browser hosts. |
| `web` | Browser packaging and compatibility adapter, not a second editor implementation. |

`font::project::Project` owns the in-memory document.
Use its layer/source operations for edits and its save path for open documents; keep file dialogs, widget state, and platform processes outside the font engine.
Keep Norad, Babelfont, and fontdrasil conversions at their existing adapter boundaries rather than spreading alternate document models through callers.
Views read workspace state, commands express intent, and the engine owns reusable font behavior.
Interface changes follow the [design principles](https://runebender.org/docs/design-principles.html) and [panel language](UI-DESIGN.md), including existing design tokens and controls.

## Review and policy

Keep a pull request limited to its stated behavior, with regression coverage for the shipped implementation and updated documentation when behavior changes.
Follow the [Linebender formatting scheme](https://linebender.org/wiki/formatting-scheme/), including one prose sentence per source line and the repository's Rust formatting and lint configuration.
Preserve existing copyright, SPDX, bundled-font, and third-party notices; adding a contributor to [AUTHORS](../AUTHORS) does not replace those notices or change licensing.

Review API changes narrowly:

- Prefer private or `pub(crate)` helpers unless a real cross-crate caller needs a public API.
- Keep public items under their domain module instead of adding broad root re-exports or parallel facades.
- Document public behavior, errors, and mutation effects; an in-place edit reports whether or how much it changed.
- Review source/layer identity, preservation, undo, and error propagation when changing document operations.
- Migrate affected callers and tests together; do not leave unused compatibility wrappers or duplicate implementations behind.
- Do not broaden visibility, dependencies, or architecture merely to accommodate a test.

Pin Git dependencies to exact revisions, commit lockfiles, and review dependency changes explicitly.
Keep warnings visible rather than hiding failures with blanket allowances or removing platform checks.
Disclose material AI assistance in the pull request and review its output yourself; do not add agent co-author trailers.
Coding agents must follow [AGENTS.md](../AGENTS.md), and human review remains responsible for correctness and font preservation.

The [audit record](AUDIT-2026-11-09.md) is a dated review with status corrections, not proof that every listed finding remains open.
Use the [documentation index](README.md) for release, testing, and design guides.
