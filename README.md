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

## Local AI

Runebender can talk to a language model that runs on your own computer.
The model reads the open font, measures glyphs and proposes edits.
You review every proposal in Proof and decide what to apply and save.
Nothing is sent anywhere, and no account or key is needed.

Setup is four steps on a Mac with Apple Silicon and 8 GB of memory or more.

### 1. Install the model server

The model runs inside `llama-server`, a program from the llama.cpp project.
Runebender does not include it yet.
Install it with [Homebrew](https://brew.sh):

```sh
brew install llama.cpp
```

### 2. Download a model

A model is one file.
Put it in its own folder under `~/Runebender/models`, named `model.gguf`.
This downloads a 3 GB model that runs well on 8 GB of memory:

```sh
mkdir -p ~/Runebender/models/qwen3.5-4b
curl -L -o ~/Runebender/models/qwen3.5-4b/model.gguf \
  https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-UD-Q4_K_XL.gguf
```

The folder name is the name you will see in the editor.
Any GGUF chat model from Hugging Face works the same way; with 32 GB of memory or more, [the local AI setup guide](https://runebender.org/docs/local-ai-setup.html) names stronger ones.

### When the server runs

You do not start `llama-server` yourself.
Runebender starts it the first time you send a question, loads the model you picked, and keeps it running until you pick another model, close the Chat tab or quit.
It listens on this computer only.

To share one model between Runebender and other programs, such as Pi, start the server yourself before opening Runebender:

```sh
llama-server --models-dir ~/Runebender/models --port 8080
```

It lists every model folder and loads a model when a program asks for it.
While it runs on port 8080, Runebender uses it instead of starting its own.
Stop it with Ctrl-C.

### 3. Ask the model about your font

Open a font in Runebender and open the **Chat** tab in the left rail.
Pick your model with the button under the text field.
Type a question and press Return, for example: "Inspect the selected glyph and explain its spacing."
The first answer takes about fifteen seconds while the model loads.

### 4. Review a proposal

Ask for a change, for example: "Make the zero 64 units wider."
The model proposes an edit; it does not change your font.
Press the **Proof** button at the bottom left of the editor to see the proposal beside the current drawing.
Apply it there if you agree, or go back to the canvas and draw it your way.
Save the font yourself when you want to keep the result.

### If something is wrong

- **"Neither llama-server nor font-ml found"** in the status line: step 1 did not run, or Homebrew is not installed.
- **No model in the Chat tab:** the file must be inside its own folder under `~/Runebender/models`, with the `.gguf` extension.
- **The first answer is slow:** the model is loading. The second is fast.
- **A small model gets an edit wrong:** ask again in plainer words, or use a larger model.

### Going further

- **Pictures:** a model can look at the proof when its folder also holds the matching `mmproj*.gguf` file from the same download page.
- **Voice:** `brew install whisper.cpp` and a whisper model as `~/Runebender/models/whisper-small/model.bin` add a microphone button to Chat.
- **Reference fonts:** put TTF or OTF files in the project's `reference-fonts/` folder to see them in Proof; see [reference font setup](docs/PROOF-REFERENCES.md).
- **Agents:** Pi, Codex and other agents connect to the live editor through `runebender mcp --live`; see [MCP setup](https://runebender.org/docs/mcp.html).
  MCP supplies editor tools, not a model; the agent's own setup decides whether its model is local or remote.
- **Per-model settings** for a shared server go in a `presets.ini` beside the models; see [model packages and shared servers](docs/LOCAL-MODELS.md).
- **Linux:** unpack a [llama.cpp release](https://github.com/ggml-org/llama.cpp/releases) and set `RUNEBENDER_LLAMA_SERVER` to its `llama-server`.

The [local AI reference](docs/LOCAL-AI.md) lists every local AI feature, tool and limitation.
A model's assessment is feedback, not design approval.

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
