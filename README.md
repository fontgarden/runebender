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

![Runebender Graph workflow](https://runebender.org/images/runebender-xilem-nodes.png)

![Runebender Arabic calligraphy editing with a glyph browser](https://runebender.org/images/runebender-xilem-nufo-glyphs.png)

![Runebender Arabic calligraphy editing with a rendered preview](https://runebender.org/images/runebender-xilem-nufo-editing.png)

## Documentation

Start with the [user documentation](https://runebender.org/docs/) for editing and installation, or the [repository documentation index](docs/README.md) for release, testing, interface, and local-model guides.
For setup, tests, source ownership, and review policy, read the [contributor guide](docs/CONTRIBUTING.md).
Coding agents must also follow [AGENTS.md](AGENTS.md).

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

Runebender can use a language model running on your own computer.
You review its proposals, decide what to apply, and save the font yourself.
A model's assessment is feedback, not design approval.

- [Set up local AI](https://runebender.org/docs/local-ai-setup.html): install llama.cpp, download a model, and use Chat and Proof.
- [Local AI reference](docs/LOCAL-AI.md): features, tools, runtime requirements, and limitations.
- [Model packages and shared servers](docs/LOCAL-MODELS.md): model folders, per-model settings, vision, voice, and server sharing.
- [Proof reference fonts](docs/PROOF-REFERENCES.md): project-local reference fonts for review.
- [MCP setup](https://runebender.org/docs/mcp.html): connect an agent to the live editor.
  MCP supplies editor tools, not a model; the agent's own setup decides whether its model is local or remote.

## External workflow worker contract

See the [worker contract](docs/WORKER-CONTRACT.md) for task discovery, Rows arguments, cache policy, trust boundaries, and the native Local AI candidate lifecycle.

## Minimum supported Rust version (MSRV)

This version of Runebender compiles with Rust 1.96 and later.
`rust-toolchain.toml` pins that version for local builds and CI.

## Community and contribution

Report reproducible bugs and discuss changes through [GitHub issues](https://github.com/fontgarden/runebender/issues).
Contributions are welcome by pull request; start with the [contributor guide](docs/CONTRIBUTING.md).
The [AUTHORS](AUTHORS) file credits contributors recorded in this repository's Git history.
Preserve existing copyright and license notices when contributing.

## License

Runebender uses a dual license.
You can use it under one of these licenses:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

The embedded Virtua Grotesk font uses the SIL Open Font License 1.1 in [assets/fonts/VirtuaGrotesk-OFL.txt](assets/fonts/VirtuaGrotesk-OFL.txt).
