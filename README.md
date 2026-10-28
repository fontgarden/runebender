# Runebender

Runebender is an experimental font editor built on the [Linebender ecosystem](https://linebender.org/) of Rust crates.

For documentation and installation instructions, see [runebender.org](https://runebender.org), or point your AI agent there for the context it needs to help you.

## Download

[Download the macOS DMG — 0.1.0-alpha.1](https://github.com/fontgarden/runebender/releases/download/v0.1.0-alpha.1/Runebender-0.1.0-alpha.1-macos-apple-silicon-notarized.dmg)

The first alpha is a Developer ID signed, Apple-notarized application for Apple Silicon Macs.
See [installation instructions and requirements](https://runebender.org/download.html), [release notes](https://github.com/fontgarden/runebender/releases/tag/v0.1.0-alpha.1), and the [changelog](docs/CHANGELOG.md).
Work on copies of important fonts; the release notes record the remaining testing limits.

For a source installation of the current default branch, with Rust 1.96 or newer and the [platform prerequisites](https://runebender.org/docs/install.html):

```sh
cargo install --git https://github.com/fontgarden/runebender
```

This installs development source rather than the frozen alpha release.
Linux source builds and the [browser editor](https://runebender.org/wasm-editor/) are experimental; this release has no Intel Mac or Windows installer.

## Font formats

Runebender opens and saves UFO and Designspace sources.
Glyphs files, Glyphs packages and Fontra folders open through converted working copies, with limited write-back to the original format.
Python `.babelfont` directories and compiled TTF/OTF fonts are import-only formats.
See [supported formats and preservation limits](https://runebender.org/docs/opening.html) before editing an existing source.

## Screenshots

![Runebender glyph overview](https://runebender.org/images/runebender-xilem-grid.png)

![Runebender outline editing](https://runebender.org/images/runebender-xilem-outline.png)

![Runebender Nodes workflow](https://runebender.org/images/runebender-xilem-nodes.png)

![Runebender Arabic calligraphy editing with a glyph browser](https://runebender.org/images/runebender-xilem-nufo-glyphs.png)

![Runebender Arabic calligraphy editing with a rendered preview](https://runebender.org/images/runebender-xilem-nufo-editing.png)

## Documentation

Start with the [user documentation](https://runebender.org/docs/) for editing and installation, or the [repository documentation index](docs/README.md) for release, testing, interface, and local-model guides.
Contributors and coding agents should read [AGENTS.md](AGENTS.md).

## Local native builds

Run these commands from a checkout of this repository; Cargo uses the pinned Rust toolchain.
For UI development, use an optimized build with incremental compilation:

```sh
cargo run --profile fast --locked -- path/to/Font.designspace
```

Cargo builds when needed, then launches the editor with the supplied font.
To build without launching:

```sh
cargo build --profile fast --locked
```

The executable is `target/fast/runebender`.
This profile keeps release's optimization level and disabled debug assertions, while disabling LTO and using 16 code-generation units to favor shorter rebuilds.
Its first build fills a separate cache; later source changes reuse incremental compilation data.
Use `cargo build --release --locked` for final release builds and performance comparisons.

## macOS application packages

To build a local `.app` and drag-to-Applications DMG on macOS:

```sh
./scripts/build_dmg.sh
```

Each invocation builds the current source with the temporary application icon and writes a fresh DMG under `target/macos/`.
This is a local test package with ad-hoc signing.
Follow [Testing a Runebender DMG](docs/TESTING-MACOS.md) to install and exercise it.
See [Releasing Runebender](docs/RELEASING.md) for the release plan, signing, notarization, Cargo distribution, and website downloads, and the [Changelog](docs/CHANGELOG.md) for changes by version.
Published downloads and version-specific notes are on [GitHub Releases](https://github.com/fontgarden/runebender/releases).

## Local sketch models

The native Brush panel can draft with an installed Virtua model and compare the result before applying it.
Put compatible model packages in `~/runebender/models`; see [Local models](docs/LOCAL-MODELS.md) for the package layout and runtime requirements.
Use the [Virtua testing guide](docs/VIRTUA-REGULAR-TESTING.md) for the first Arabic trial.

## Local chat and agents

The native Chat panel works with local models through `llama-server`, an explicitly configured loopback endpoint, or the `font-ml` fallback.
It can reuse a compatible running llama server or start a supervised server for a selected installed GGUF model.
Models and external tools are installed separately; see [local AI setup](https://runebender.org/docs/local-ai-setup.html) and the [local AI reference](docs/LOCAL-AI.md) for runtime selection and configuration.

External coding agents connect through `runebender mcp --live`.
The repository's `.mcp.json` and `.codex/config.toml` declare that command; see [MCP setup](https://runebender.org/docs/mcp.html) for connecting to the intended open document.
The native proof view supports Before/Working comparison, toggle, waterfall and text views, with [project-local reference fonts](docs/PROOF-REFERENCES.md).
With Designbot installed, `review_proof` can return comparison images to an agent for review.
These live-editor workflows require native macOS or Linux; the browser editor does not provide local AI.

## External workflow worker contract

`runebender nodes types --tool /path/to/font-ml --json` exposes the discovered task ports and execution policy.
External tasks can advertise this optional block in each task returned by `tasks --json`:

```json
{"execution":{"schema_version":1,"cache":"input_fingerprint","rows_input":"json_argument"}}
```

Without this block, external tasks remain executable but run every time and reject Rows inputs.
An explicit block requires version 1; unknown versions, fields, or policy values exclude that task from discovery.
The authoring graph file remains version 1.

`json_argument` sends each Rows input as one compact JSON array argument after its port flag, replacing underscores with hyphens in the flag name.
For example, `report_rows` becomes `--report-rows '[{"glyph":"A","score":0.5}]'`; the runner builds argument vectors directly without a shell.
Empty arrays, nested JSON, and Unicode are preserved.
Each input permits at most 4096 rows, and all Rows arguments together permit at most 65536 UTF-8 bytes.
Unsupported or oversized inputs fail before process launch, and a declared Rows output must be a JSON array in the worker's final report.

`input_fingerprint` is an explicit promise by the trusted worker that declared inputs describe its dependencies.
Cache reuse verifies the node definition, executable, input values and files, and retained output content; older cache files are invalidated.
Model and adapter directories are hashed from their actual bytes, including additional artifacts; manifest digests alone are not trusted.
This conservative check can add disk I/O for large local models.
Workers depending on time, random state, environment variables, network responses, or unlisted files should use `cache: "never"` unless those dependencies are supplied as inputs.
External effects are reported as `trusted_process`; this metadata grants no permissions and provides no process sandbox.
Font-writing builtins and live document nodes are never reused from the disk cache.
External declarations of `write_font` or `live_document` likewise force `cache: "never"` while retaining the `trusted_process` classification.

These are offline worker contracts, not a claim of cloud-provider or model compatibility.
The native Local AI action exports current in-memory data to an owned temporary directory and retains validated output separately until explicit Install.
Install commits the complete candidate as one undo group; Discard leaves the root document unchanged.
This initial adapter accepts ordinary contour point moves, existing anchor moves, and advance changes, with at most 64 captured glyphs and 256 changed values per candidate.
Changed topology, components, hyperbeziers, unsupported metadata, stale results, or out-of-scope glyphs are rejected.
At most 16 candidates are retained, keyed by source and task, for this document session.
An existing saved proposal for the same task must be reviewed or discarded before another model run.
Live DAG model nodes and migration of the older disk-oriented Nodes action remain unfinished; the latter still uses its save-first path.
