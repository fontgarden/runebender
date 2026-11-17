# Architecture

This document is the module map of the root `runebender` package.
It states the layers, the rules between them, and the file where each type of change starts.
`tests/architecture_map.rs` checks the document against the source tree.
If a path in backticks does not exist, the test fails.
If a directory under `src` has no row in this document, the test also fails.
When an item name in backticks comes before "in" and a source path, the test also checks that the path declares the item.
If you add, move or remove a file that this document names, update the document in the same commit.

The [architecture guide](https://runebender.org/docs/architecture.html) on the website gives a longer explanation.
When the two disagree, follow this document.
A test checks this document against the code, and no test checks the website.

## Layers

The package has two crate targets.

- The library starts at `src/lib.rs`.
  It holds the font engine and the interface data that is independent of a GUI toolkit.
- The executable starts at `src/main.rs`.
  All of its code is in `src/application`.
  It holds the Xilem editor and the command-line interface.

The browser host in `web/` builds the same sources through a compatibility package.
`src/testing` holds test fixtures and compiles only for `cargo test`.

These rules apply between the layers.

1. The library does not name the application.
   Cargo enforces this rule, because the library is a separate crate.
2. The library does not call Xilem, Masonry, winit or a file dialog.
   The command `cargo clippy --lib --no-default-features --locked -- -D warnings` builds the library without those dependencies.
3. The application can use each library module.
4. The library modules have an order, and the application modules have an order.
   A module can depend only on the modules before it in its order.
   The two tables below give the orders.
5. Norad types stay in the import, export and serialization files that `tests/norad_boundary_allowlist.rs` lists.
6. A view does not set a color, an opacity, a stroke, a radius or a text size.
   `tests/design_system.rs` checks this rule.

The code does not obey rule 4 in each place yet.
`tests/module_layering.rs` counts each reference that goes against an order, and the count for a pair of modules can only go down.
"Back edges" below gives the counts.
Items R3 to R5 of [the cleanup plan](CLEANUP-PLAN.md) remove these references.

## Library modules

The table is in dependency order, lowest layer first.

<!-- check: library-order -->
Library order: `ui`, `outline`, `font`, `text`, `formats`, `analysis`, `workflows`, `automation`.
<!-- end -->

| Order | Module | Responsibility | Can depend on |
|---|---|---|---|
| 1 | `src/ui` | Interface data that each front end shares: the color type, the theme parser, the viewport, the sidebar filter data, icon geometry, node-graph layout, the Vim key model and syntax highlighting. | No library module. |
| 2 | `src/outline` | Geometry: the path types, point and segment edits, the knife, effects, emboldening, metaballs, and the builders that make a `kurbo::BezPath` from a layer. | `ui` |
| 3 | `src/font` | The open document: `Project`, glyph layers, sources, the designspace, interpolation, undo records, proposals, persistence and compilation. | `ui`, `outline` |
| 4 | `src/text` | Shaping with the feature code of the font, mark features from anchors, Arabic joining, and the text buffer of the Text tool. | `ui`, `outline`, `font` |
| 5 | `src/formats` | UFO lib keys and complete file formats: UFO, Designspace, Glyphs, Fontra, Babelfont, OpenType binaries, SVG and traced images. | `ui`, `outline`, `font`, `text` |
| 6 | `src/analysis` | Read-only measurement: curvature, dimensions, thickness, Unicode categories, weight debt and reports. | `ui`, `outline`, `font`, `text`, `formats` |
| 7 | `src/workflows` | Saved and live node graphs, the bounded process supervisor, and the local model servers. | All modules before it. |
| 8 | `src/automation` | The agent tool contracts: disk tools, live tools, typed edit, graph and proof requests, prompts, design context, and the Unix socket mailbox. | All modules before it. |

## Application modules

The table is in dependency order, lowest layer first.
Each module is below `src/application`.
`design` and `design::recipes` are two layers, because the widgets use the first and the second uses the widgets.

<!-- check: application-order -->
Application order: `design`, `widgets`, `design::recipes`, `local_models`, `pieces`, `font_model`, `workspace`, `actions`, `platform`, `editor`, `view`, `cli`, `launch`, `browser`.
<!-- end -->

| Order | Module | Path | Responsibility |
|---|---|---|---|
| 1 | `design` | `src/application/design` | The design system without the recipes: the `Palette`, the scales, the control states, the control widgets, the themed stock widgets and the area exceptions. |
| 2 | `widgets` | `src/application/widgets` | The Masonry widgets that the editor owns, and the shortcut host. |
| 3 | `design::recipes` | `src/application/design/recipes` | Panel sections, rows, fields and the proof page blocks. |
| 4 | `local_models` | `src/application/local_models.rs` | Discovery of local model packages. |
| 5 | `pieces` | `src/application/pieces.rs` | The piece preview of a neural source. |
| 6 | `font_model` | `src/application/font_model.rs` | `FontModel`: the `Project` and the glyph cache that the grid paints from. |
| 7 | `workspace` | `src/application/workspace.rs` | `Workspace` and `AppState`: the state of one open document and of the window. |
| 8 | `actions` | `src/application/actions.rs` | The `ACTIONS` table for the menus and the keymap, and the native menu bar. |
| 9 | `platform` | `src/application/platform` | Files, dialogs, file watching, export, configuration, themes, updates, headless capture, script jobs, and the live editor host. |
| 10 | `editor` | `src/application/editor` | Commands, edit sessions, the inspector fields, history, and the tools. |
| 11 | `view` | `src/application/view` | The canvases, the panels, the window chrome and the render tree. |
| 12 | `cli` | `src/application/cli.rs` | Argument parsing and the headless subcommands. |
| 13 | `launch` | `src/application/launch.rs` | The event loop, the window and the first frame. |
| 14 | `browser` | `src/application/browser.rs` | The browser host. It compiles only for `wasm32`. |

`local_models`, `pieces`, `font_model`, `workspace` and `actions` are the state layer.
`cli`, `launch` and `browser` are the entry layer.

## Back edges

A back edge is a reference from a module to a module after it in its order.
`tests/module_layering.rs` parses the production code with `syn` and counts one reference for each leaf of a `use` declaration and for each qualified path that starts with `crate`, `self`, `super` or `runebender`.
Code under `#[cfg(test)]` is not counted.
`LIBRARY_BACK_EDGES` and `APPLICATION_BACK_EDGES` in the test record the permitted count for each pair.
The test fails when a count rises above its record, and also when a record is higher than the count.
When a move removes references, lower the record in the same commit.
To list the files and paths behind each pair, run `cargo test --locked --test module_layering -- --ignored --nocapture`.

These are the counts at the start, measured at commit `a8d0682`.
The test holds the current counts.

Library: 9 pairs, 90 references.

| From | To | References |
|---|---|---|
| `outline` | `font` | 39 |
| `font` | `formats` | 30 |
| `ui` | `workflows` | 5 |
| `font` | `analysis` | 4 |
| `font` | `text` | 4 |
| `outline` | `formats` | 4 |
| `ui` | `font` | 2 |
| `formats` | `workflows` | 1 |
| `ui` | `formats` | 1 |

Application: 24 pairs, 141 references.

| From | To | References |
|---|---|---|
| `widgets` | `workspace` | 19 |
| `platform` | `editor` | 17 |
| `editor` | `view` | 16 |
| `workspace` | `editor` | 13 |
| `widgets` | `view` | 12 |
| `design::recipes` | `view` | 10 |
| `platform` | `view` | 9 |
| `workspace` | `platform` | 8 |
| `widgets` | `actions` | 7 |
| `actions` | `platform` | 4 |
| `actions` | `editor` | 3 |
| `design::recipes` | `workspace` | 3 |
| `font_model` | `editor` | 3 |
| `widgets` | `editor` | 3 |
| `workspace` | `view` | 3 |
| `design` | `platform` | 2 |
| `design` | `view` | 2 |
| `design` | `widgets` | 1 |
| `editor` | `browser` | 1 |
| `pieces` | `workspace` | 1 |
| `platform` | `browser` | 1 |
| `widgets` | `design::recipes` | 1 |
| `widgets` | `platform` | 1 |
| `workspace` | `browser` | 1 |

## Directories

Each directory under `src` has one row.

| Directory | Contents |
|---|---|
| `src/analysis` | One file for each type of measurement. No file here changes a font. |
| `src/application` | The executable. `src/application/mod.rs` lists the modules. |
| `src/application/cli` | One file for each group of subcommands: agent tools, font commands, graph commands, the MCP host, the MCP installer and themes. |
| `src/application/cli/font_commands` | The font subcommands: features, inspection, outlines and proposals. |
| `src/application/design` | The design system. Read `src/application/design/mod.rs` first. |
| `src/application/design/controls` | The pressable control widget and the icon button. |
| `src/application/design/recipes` | The `Row` vocabulary in `src/application/design/recipes/rows.rs`, the section recipes and the proof page recipes. |
| `src/application/editor` | Commands, sessions, the inspector, the sidebar filters, source commands, in-place component editing and edit-group history. |
| `src/application/editor/tools` | One file for each tool or tool-like workflow: text, label, metaballs, trace, sketch, AI shapes, chat, voice, scripts, review, proof images, local AI, interpolation and weight debt. |
| `src/application/editor/tools/graph` | The node graph in the editor: the open graph, its controls, the live executor, tracing and the workspace state. |
| `src/application/platform` | One file for each service outside the window. |
| `src/application/view` | `src/application/view/render.rs` builds the frame, and `src/application/view/chrome.rs` builds the bars around the canvas. |
| `src/application/view/canvas` | The center canvases: the glyph grid, the glyph list, the glyph editor, the graph, the proof and the specimen. |
| `src/application/view/canvas/editor` | The glyph editor widget: pointer and keyboard input, painting, and the Xilem view. |
| `src/application/view/canvas/editor/tests` | Unit tests of the glyph editor widget. |
| `src/application/view/canvas/proof` | The reading proof and the reference font cards. |
| `src/application/view/panels` | The side panels, one file for each region or feature. |
| `src/application/widgets` | One file for each widget. |
| `src/automation` | One file for each tool surface, and the shared contracts. |
| `src/automation/agent_edit` | The result types of the live edit tools. |
| `src/automation/agent_graph` | The result types of the live graph tools. |
| `src/font` | The document model and its services. `src/font/mod.rs` describes the main child modules. |
| `src/font/babelfont` | The layer adapter: read-only views, the `LayerEditDraft` edit modules, glyph transactions, handle cleanup, curve conversion and the UFO projection. |
| `src/font/compiler` | In-memory OpenType compilation, proof inputs and the proof job queue. |
| `src/font/hoi` | Authoring and evaluation of higher-order interpolation. |
| `src/font/model` | Value types: font info, glyph metadata, designspace data, kerning lookup, entity ids, HOI data and smart components. |
| `src/font/persistence` | UFO and Designspace import, staged save, the in-memory codec, and source-format preservation. |
| `src/font/project` | The parts of `Project` that `src/font/project.rs` mounts with `#[path]`: constructors, glyph transactions, edit transactions, proposal transactions, import errors and Save As. |
| `src/font/proposal` | Detached proposal capture. |
| `src/font/variable` | Glyph-first storage: constructors, glyph transactions, proposal transactions, background layers and the source builder. |
| `src/formats` | One file for each file format. |
| `src/formats/metadata` | Codecs for UFO lib keys: mark colors, metaballs and metrics keys. |
| `src/outline` | One file for each group of geometry operations. |
| `src/outline/metaballs` | The fit of metaball boundaries to cubic contours. |
| `src/outline/path` | The cubic, quadratic and hyperbezier path types, their points and their segments. |
| `src/testing` | Fixture font paths for the unit tests of the library. |
| `src/text` | Shaping, feature code, joining rules and proof samples. |
| `src/text/buffer` | The text buffer: bidirectional runs, kerning, layout and shaping. |
| `src/ui` | One file for each type of interface data. |
| `src/ui/editing` | The viewport. |
| `src/workflows` | The graph model, the disk runner, the live graph session, and the local process and server helpers. |

## Where to start a change

Each row names the first file to read.
The other files in the row are the next ones that the change touches.

| To change | Start in | Then |
|---|---|---|
| The document model | `Project` in `src/font/project.rs` | `src/font/variable.rs` for glyph storage, `src/font/babelfont.rs` for layer geometry, `src/font/model` for value types. |
| An outline edit | The methods of `LayerEditDraft` in `src/font/babelfont` | `src/outline` for the geometry, and `Session` in `src/application/editor/session.rs`, which holds the layer transaction that editor commands and drags change. |
| Open and save | `Project::load` and `Project::save` in `src/font/project.rs` | `ImportPlan` and `ExportPlan` in `src/font/persistence/mod.rs` for the staged file work, `src/font/project/save_as.rs`, and `src/application/platform/host.rs` for the open and save commands of the editor. |
| A file format | The file of the format in `src/formats` | `load_inner` in `src/font/project.rs`, which selects the importer, and `src/formats/foreign.rs` for the write-back to Glyphs and Fontra sources. |
| Compile and export | `src/font/compiler/mod.rs` | `src/application/platform/export.rs` for the Export command, `preview_font` in `src/application/font_model.rs` for the preview compile. |
| Undo | `src/font/history.rs` for the undo stores | `undo_active_edit` in `src/application/editor/session.rs`, which selects the store for one Undo command, `src/font/sources.rs` for source history, `src/application/editor/inspector.rs` for metadata steps, `src/application/editor/agent_history.rs` for edit groups. |
| Application state | `src/application/workspace.rs` | `src/application/editor/session.rs` for one glyph tab, `src/application/font_model.rs` for caches of the font. |
| A menu item | `ACTIONS` in `src/application/actions.rs` | `AppAction` in `src/application/widgets/shortcuts.rs`, `dispatch` in `src/application/editor/commands.rs`, and the command method that it calls. |
| Canvas interaction | `src/application/view/canvas/editor/pointer.rs` | `src/application/view/canvas/editor/keyboard.rs`, `src/application/view/canvas/editor/paint.rs`, and `src/application/editor/session.rs` for the edit. |
| A panel section | `src/application/design/recipes/rows.rs` and `docs/UI-DESIGN.md` | The panel file in `src/application/view/panels`. `src/application/view/panels/info.rs` selects the sections of the info panel. |
| Colors and sizes | The theme files in `assets/themes/default` | `src/ui/theme.rs` for the parser, `src/application/design/theme.rs` for the `Palette`, `src/application/design/tokens.rs` for the scales, `src/application/design/areas.rs` for an exception. |
| A background job | `poll` in `src/application/view/render.rs` | `src/font/compiler/proof_jobs.rs` as the model of a job queue, `src/application/platform/script_jobs.rs`, and `src/workflows/process.rs` for a subprocess. |
| An agent tool | `tools` in `src/automation/agent.rs` for a disk tool, `tools` and `call` in `src/automation/live.rs` for a live tool | `src/automation/tool_contracts.rs` for effects, `handle_live` in `src/application/platform/live.rs` for dispatch, `docs/LOCAL-AI.md` for the reference. |
| The MCP transport | `src/application/cli/mcp.rs` for the stdio host | `src/automation/live_socket.rs` for the socket mailbox, `src/application/platform/live.rs` for the editor side, `src/application/cli/mcp_install.rs` for client registration. |
| A CLI subcommand | The `Command` enum in `src/application/cli.rs` | The file of the command group in `src/application/cli`. |
| The node graph | `src/workflows/graph.rs` for the model | `src/workflows/graph_run.rs` for the disk runner, `src/workflows/graph_session.rs` and `src/application/editor/tools/graph` for the live graph, `src/ui/graph.rs` for layout, `src/application/view/canvas/graph.rs` for the canvas. |
| Text | `src/text/buffer/mod.rs` for the buffer, `src/text/shape.rs` for shaping | `src/application/editor/tools/text.rs` for the Text tool. |
| Headless capture | `src/application/platform/screenshot.rs` | `src/application/launch.rs`, which reads the `RUNEBENDER_*` variables, and the list of variables in `docs/UI-DESIGN.md`. |
