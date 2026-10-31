# Local AI reference

This is the reference for every local AI feature in Runebender: the in-editor
features, the agent surface, the runtimes, and where models live.
[Local model packages](LOCAL-MODELS.md) has the setup details for model folders,
Chat, voice and the shared model server.

`tests/local_ai_reference.rs` checks this file. The tool and prompt lists must
match the registries in `src/automation/`, and every repository path in
backticks must exist. A change that adds, removes or renames a local AI feature,
model location, environment variable or runtime updates this file in the same commit.

## 1. Runtimes

Each feature runs on one of five local runtimes. No runtime is a Cargo feature;
the default `application` feature includes `neuraltype-core` for the trained-font
preview and `cpal` for the microphone.

| Runtime | Used by | Found at |
| --- | --- | --- |
| `llama-server` (llama.cpp) | Chat | app bundle, `RUNEBENDER_LLAMA_SERVER`, PATH, Homebrew |
| `whisper-server` (whisper.cpp) | Talk in Chat | app bundle, `RUNEBENDER_WHISPER_SERVER`, PATH, Homebrew |
| `font-ml` | Local AI rail, Bolden, Local AI Shapes, Weight Debt, `font-ml.*` graph nodes, Chat fallback | `RUNEBENDER_FONT_ML`, PATH, `~/.cargo/bin/font-ml` |
| FontGarden `glyphlab.sketch2glyph` (Python) | Brush, Draft with Virtua | `runtime.json` or `RUNEBENDER_SKETCH_REPOSITORY` |
| post-opentype `train-nufo.sh` and `neuraltype-core` | Neural training, Model preview | `[neural]` in `~/.config/runebender/config.toml` |

The MCP server (`runebender mcp`) runs no model. The connected client supplies it.

All features follow the same rules. A model result is a proposal. Only the
designer installs it, only the designer grades a glyph green, and no tool saves the font.

## 2. In-editor features

### 2.1 Chat

Rail tab Chat. Source: `src/application/view/panels/chat.rs`,
`src/application/editor/tools/chat.rs`, `src/application/editor/tools/chat_endpoint.rs`,
`src/workflows/llama_server.rs`, `src/workflows/local_chat.rs`.

- Controls: the field "Ask about the open font", the model picker, and three icon
  buttons: a cross that clears the conversation, a microphone for voice input, and
  an arrow that sends, which is a stop square while the model answers.
  Each answer has a Detail toggle.
- Backend, chosen for each turn:
  1. `RUNEBENDER_CHAT_ENDPOINT` and `RUNEBENDER_CHAT_MODEL`.
  2. A shared `llama-server` on `127.0.0.1:8080`, when `GET /v1/models` lists the selected folder name.
  3. A supervised `llama-server` child for the folder's GGUF.
  4. `font-ml chat`.
- The child server listens on loopback only, with context 32768, thinking off
  and `-ngl 99`. It stops when the model changes, the panel closes or the editor quits.
- A `model.json` beside the GGUF sets `context`, `thinking`, `temperature`, `top_p`,
  `top_k`, `min_p`, `presence_penalty` and `template_kwargs`.
- Tools the model can call: `design_context`, `project_info`, `font_info`,
  `editor_context`, `glyph_inventory`, `analyze_glyph`, `analyze_font`, `read_glyph`,
  `read_kerning`, `proof`, `specimen`, `proposal_list`, `propose_edits`, and
  `proof_image`, which the chat answers itself. A turn has at most 8 tool calls.
  When the model leaves out `source`, the source open in the editor is filled in.
- `proof_image` draws a glyph beside its proposal, A and B in their advance boxes
  at one scale, and a difference image, red where only A has ink, blue where only
  B has, grey where both do. The pictures go to the model as images when the
  model folder holds an `mmproj*.gguf` vision projector; the server passes it with
  `--mmproj`. Each picture is also written to `runebender-proof-images/` in the
  temporary folder. Source: `src/application/editor/tools/proof_image.rs`.
- The prompt includes the design context, up to 20,000 characters.
- A second request, constrained to a schema, restates each reply in one or two plain sentences.
- `/review-glyph H` and the other workflow names send the workflow text.
- Each tool call, its arguments and its result are printed to the terminal.
- The default model is the first folder whose name contains `4b`.
- Desktop only.

### 2.2 Voice input

Microphone button in Chat. Source: `src/application/editor/tools/voice.rs`,
`src/workflows/whisper_server.rs`.

- Records up to 90 s and sends 16 kHz mono WAV to `POST /inference`, with a
  vocabulary prompt and the family name.
- The microphone icon becomes sound bars while it listens; pressing them stops it.
- Puts the text in the field, after any text already there. The designer checks it and sends it.
- A bracketed transcription such as `(audience mumbling)` counts as nothing heard.
- Model: `RUNEBENDER_WHISPER_MODEL`, or `model.bin` in a `whisper*` folder under a model root.
- The button is hidden until the binary and a model are found.

### 2.3 Proof view

The existing toolbar Brush remains in the edit view. Agent Proof provides its
own Red pencil, Undo ink and Clear ink buttons, without adding an edit-toolbar
icon or group. These annotations are session-only and disappear on quit;
font saving never includes them. Ink is stored in font coordinates and keyed by
document/source/glyph and drawing geometry, so resize preserves placement and
changed outlines do not inherit old marks. Undo reverses a stroke or a whole Clear.
Drawing is available in Compare/Toggle, including reference cards; Text/Waterfall
remain selectable text. Use Compare for form sketches and corrections.

`review_proof` includes red ink in the returned MCP PNG by default and returns
stroke coordinates/widths with drawing keys. `include_annotations=false` produces
a clean PNG. Capture rejects an unfinished pencil stroke. The existing sketch
brush remains its existing trace/model workflow; annotation ink does not trigger
inference or change outlines. Review annotations before quitting if they must be
retained externally. Bounds:128 strokes per drawing,2048 points per stroke,
256 retained drawing keys,16 undo entries. No new font edits or grading.


Editor footer, the fourth button.
Sources: `src/application/view/canvas/proof.rs`, `src/application/editor/tools/review.rs`, and `src/application/design/recipes/proof.rs`.

- Replaces the canvas with a native review page for the open glyph; the same footer button returns to editing.
- The outlined Dark toggle changes the entire center proof area above the editor footer, including its heading, controls, fields, and unused space.
  It derives the softer paper and ink from the active theme and keeps the shared button geometry and interaction styling.
  Mode, revision, and action buttons use the same outlined controls as other editor sections.
- Compare shows the two drawings side by side with captions and no revision-switching controls.
  In Toggle mode, Before and Working select the visible revision, and the separate Toggle action on the right switches between them in place.
  The selected Before or Working button shows what is visible; only the mode tabs change modes.
  Both drawings contribute to the shared fit, including overhangs and sidebearings, so switching revisions keeps their scale and baseline fixed.
- Opening review pins the first Before drawing in memory.
  Before survives returning to the canvas and editing Working; toggling revisions never replaces it.
  If you switch glyphs, return to editing and reopen Proof to capture a Before for that glyph.
  A baseline belongs to its document, source, and glyph, and is not saved to the font or restored after quitting.
- When proposals are available, Comparison source lists Before / Working and the waiting proposals as persistent choices.
  Choosing a source leaves the revision buttons available for repeated switching.
  It also follows a proposal explicitly selected for comparison in Local AI, rather than automatically choosing the newest task.
- Apply glyph changes only the displayed glyph and retains the pre-apply drawing as Before.
  Atomic multi-glyph model candidates must be reviewed and applied together in Local AI.
  Discard proposal names its batch size when it contains more than one glyph.
- Compare and Toggle support the active editable master.
  Other axis locations show an explanation instead of comparing a master drawing with an interpolated instance.
- Waterfall and Text show compiled font specimens with native shaping, wrapping, selection, and Cmd+C (Ctrl+C on Linux).
  Waterfall uses 72/48/36/24/18/12 px rows; Text has a size field and slider.
  Choose Glyph, Spacing, Alphabet, or Custom in Sample; Custom accepts multiline Unicode text.
  The spacing preset is a Latin context starter; use Custom for script-specific review strings.
- Capturing Before also keeps an immutable full-font snapshot for reading proofs.
  That full-font snapshot remains available across glyph selections in the same document; the outline comparison still requires a matching glyph and source.
  Working recompiles as the document changes; selecting a proposal compares Working with a private compiled replacement, without applying it.
  Compilation failures and missing character coverage are reported beside the affected specimen.
  A specimen with missing character coverage is withheld rather than displaying system-font substitutes.
- Variable fonts expose proof-only axis controls in Waterfall and Text; these do not move the editor's location or edit the font.
  References come from the project's `reference-fonts/` directory and appear in all four modes; References hides or shows them.
  The folder is at the nearest Git root, or next to the opened font outside Git.
  Files are grouped by family, width and slope, with real weight selection and no synthetic bold.
  Samples and proof settings are session-only; reference choices persist as the files in that folder.
  See [Proof reference fonts](PROOF-REFERENCES.md) for the shared native/live-web convention.
- Agents and local Chat can call `review_proof` after reading `editor_context`.
  It captures the same Before/Working or selected proposal and reference drawings, with a shared fit and explicit source/glyph/revision guards.
  Designbot renders the captured scene outside the editor thread; the model receives an image attachment, not base64 text.
  Both revisions remain visible in the exported comparison even when the page is in Toggle.
  References are paged four at a time with immutable font hashes; missing coverage remains explicit.
  This glyph comparison requires Compare/Toggle at an editable master; shaped text images use the existing `proof_start` workflow.
  Graph keeps its existing retained compiled proof images; neither image route applies or saves a font.
  The separate Text tool and editor preview strip remain available.
- `RUNEBENDER_PROOF_VIEW=1` opens Compare; `toggle` opens Toggle on the right-hand revision, and `before` opens Toggle on the left-hand revision.
  `waterfall` and `text` open the reading modes.
  `RUNEBENDER_PROOF_TEXT` sets a custom screenshot sample (up to 4096 characters).
  Headless screenshots use the same project reference directory as the interactive editor.
  `RUNEBENDER_PROOF_DARK=1` opens the proof page in dark mode.
  With `RUNEBENDER_GLYPH=<name>`, `RUNEBENDER_PROOF_SAMPLE=1` creates and selects an in-memory sample proposal for headless screenshots.

To test the review loop, open a glyph, open Proof view to pin Before, return to the canvas and edit it, then reopen Proof view and switch between Compare and Toggle.
Test a different glyph or source as well: it must not inherit another glyph's baseline.
Review layout and fitting tests run with `cargo test --bin runebender --locked view::canvas::proof -- --test-threads=1`; state and Apply tests run with `cargo test --bin runebender --locked review -- --test-threads=1`.

### 2.4 Local AI rail

Rail tab Local AI. Source: `src/application/view/panels/local_ai.rs`,
`src/application/view/panels/weight_debt.rs`, `src/application/editor/tools/local_ai.rs`.

| Section | Contents |
| --- | --- |
| Model | Model picker, Strength slider from 0 to 3 |
| Tasks | "This Glyph" and "Every Glyph" buttons for each task in `font-ml tasks --json`, Cancel |
| Weight Debt | Draft by offset, Draft with the model, Mark installed drafts orange, Measure |
| Proposals | Compare, Install, Discard, Undo Install |
| Graph | Graph file picker |

- Runs `font-ml run <task> --model <dir> --source <exported UFO> --write --json`.
- Keeps at most 16 candidates for the session. Install is one undo group.
- Glyph > Bolden With Model… runs the `bolden` task on the open glyph.

### 2.5 Local AI Shapes

Toolbar Shapes group, Local AI Shapes. Source: `src/application/editor/tools/ai_shapes.rs`,
`src/application/view/panels/ai_shapes.rs`.

- Drag a box and choose a word. The model draws a ghost in the box. Enter keeps
  it, Tab draws another and Esc removes it. Resizing the box draws it again.
- Draws with `font-ml run draw --prompt <word> --box x,y,w,h`.
- Trains with `font-ml run train-draw`: Quick is 6000 steps (about 12 minutes),
  Thorough is 15000 steps (about 30 minutes). Add This Font's Shapes continues
  from the current model with 1000 or 3000 steps.
- Buttons: Train From This Font, Add This Font's Shapes, Train From the Sample Shapes, Stop.
- Writes to `~/Runebender/models/shapes/` for the sample font, or to
  `<Font>.models/shapes/` for the open font.

### 2.6 Brush and Draft with Virtua

Toolbar Brush tile. Source: `src/application/view/panels/sketch.rs`,
`src/application/editor/tools/sketch.rs`, `src/workflows/local_sketch.rs`,
`src/application/local_models.rs`. Unix only.

- Trace to draft traces the ink with `img2bez`. It uses no model.
- Draft with Virtua runs `.venv/bin/python -m glyphlab.sketch2glyph` from the
  FontGarden repository with the selected checkpoint. It records hashes of the
  interpreter, modules and checkpoint, and checks the result against the ink.
- The editor sends 1 candidate, temperature 0.0, seed 0 and an identity strength from 0 to 1.5.
- Review: Check status, Retry current ink, Use draft in glyph, Open proof comparison.

### 2.7 Neural fonts

Source: `crates/nufo`, `src/application/editor/tools/label.rs`,
`src/application/view/panels/neural.rs`, `src/application/platform/train.rs`,
`src/application/platform/model.rs`, `src/application/pieces.rs`.

- A `.nufo` is a UFO whose glyphs are canvases of samples. Labels are in the glyph
  lib key `com.runebender.neuralItem`.
- The Label tool shows the Samples section in the inspector.
- The Neural section has Train, Train this sample, Cancel, Reset preview, and the
  trained versions with score and epochs.
- Training runs `train-nufo.sh` from post-opentype. When `train_host` answers over
  ssh it trains there, otherwise on this machine. Versions go to `<Name>.models/<NNN>/`.
- `[neural]` keys: `post_opentype`, `train_host`, `epochs` (800), `sample_epochs` (200).
- The preview strip has Pieces, which assembles labeled pieces with no model, and
  Model, which draws with a trained version through `neuraltype-core`.

### 2.8 Graph

Source: `src/workflows/graph.rs`, `src/workflows/graph_run.rs`,
`src/workflows/graph_live.rs`, `src/workflows/graph_session.rs`.

- File graphs are `<name>.graph.json`. Each task in `font-ml tasks --json` is a
  node `font-ml.<task>`. Unchanged runs are cached by input hash.
- Live graphs use `live.font`, `live.fork`, `live.python`, `live.proof` and `live.apply`.
- `graph_trace` with `local_sketch` runs the local sketch model and puts the
  result in a `live.python` recipe node.
- Graph runs from MCP on saved files are always proposal-only.

### 2.9 Design context

Source: `src/automation/context.rs`. A later layer wins.

1. Default: `assets/context/type-design.context.md` and `assets/context/font-engineering.context.md`.
2. Personal: `*.context.md` in `~/.config/runebender/context/` and `RUNEBENDER_CONTEXT_PATH`.
3. Font: `*.context.md` beside the font, up to the repository root.

The `design_context` tool and the Chat prompt read these files.

## 3. Agent surface

### 3.1 Commands

| Command | Purpose |
| --- | --- |
| `runebender mcp --install [clients] [--dry-run]` | Registers `runebender mcp --live` by absolute path in Claude Code, Codex, Claude Desktop and Cursor |
| `runebender mcp --live` | Live server for open editors |
| `runebender mcp --font <path>` | Server for saved files |
| `runebender agent serve --font <path>` | Headless unsaved session (Unix) |
| `runebender agent tools [--contracts]` | Prompt and tools as JSON; the live surface when `RUNEBENDER_LIVE_SESSION` is set |
| `runebender agent call <tool>` | One tool call |
| `runebender sessions` | Lists editor sockets |
| `runebender graph check\|types\|run\|schema` | File graph commands |
| `runebender propose`, `runebender proposal`, `runebender bolden` | font-ml tasks from the shell |

`runebender agent tools` does not list the three host tools `editor_sessions`,
`editor_connect` and `export_proof`. MCP `tools/list` lists all of them.

### 3.2 Live tools

<!-- check: live-tools -->
The live server publishes 50 tools.

| Group | Tools |
| --- | --- |
| Connection and state | `editor_sessions`, `editor_connect`, `project_info`, `editor_context`, `editor_open_glyph`, `editor_review`, `editor_select_points`, `editor_set_text` |
| Read and analysis | `font_info`, `read_glyph`, `glyph_inventory`, `design_context`, `analyze_glyph`, `analyze_font`, `read_kerning`, `proof`, `specimen`, `review_proof`, `export_proof` |
| Proposals and marks | `propose_edits`, `proposal_list`, `proposal_discard`, `proposal_install`, `flag_glyph` |
| Experiments | `experiment_fork`, `experiment_list`, `experiment_kern`, `experiment_apply`, `experiment_undo_apply` |
| Guarded apply | `agent_apply`, `agent_receipt`, `agent_cancel`, `agent_history` |
| Compiled proofs | `proof_start`, `proof_status`, `proof_cancel`, `proof_release` |
| Graph | `graph_discover`, `graph_snapshot`, `graph_mutate`, `graph_trace`, `graph_trace_status`, `graph_trace_cancel`, `graph_trace_release`, `graph_run`, `graph_status`, `graph_cancel`, `graph_release`, `graph_apply`, `graph_image` |
<!-- end -->

Registries: `src/automation/live.rs`, `src/automation/agent_edit.rs`,
`src/automation/agent_proof.rs`, `src/automation/agent_graph.rs`,
`src/automation/tool_contracts.rs`.

### 3.3 Disk tools

<!-- check: disk-tools -->
The saved-file server publishes 12 tools: `project_info`, `font_info`,
`read_glyph`, `design_context`, `analyze_glyph`, `proof`, `propose`, `graph_run`,
`proposal_list`, `proposal_discard`, `docs`, `propose_edits`.
<!-- end -->

There is no install tool for saved files. Registry: `src/automation/agent.rs`.

### 3.4 Prompts

<!-- check: prompts -->
The live server serves 7 prompts: `runebender`, `review-glyph`, `triage-font`,
`compatibility-check`, `spacing-pass`, `kerning-pass`, `draft-weight`.
<!-- end -->

Each prompt is `skills/runebender-<name>/SKILL.md` (the hub is `skills/runebender/SKILL.md`),
compiled into the binary by `src/automation/prompts.rs`. Each declares
`surface: runebender-live-v1`. There are no MCP resources.

### 3.5 Safety model

- Each open document has one epoch. A request with a stale epoch is refused.
- `proposal_install`, `experiment_apply`, `experiment_undo_apply`, `agent_history`
  and `graph_apply` require `authorization: "user-approved"`.
- Red marks permit outline replacement. Green and purple glyphs are frozen.
  `flag_glyph` sets pink only. Source: `src/automation/glyph_grading.rs`.
- Receipts are keyed by actor, operation key and payload hash. An exact retry
  returns the first result. Receipts are in memory and end with the document.
- Every live result carries `saved: false`.

### 3.6 Python recipes

Source: `src/automation/script_recipe.rs`. Schema version 2. The recipe reads one
JSON object on stdin and writes one on stdout, with `job_id` and `input_hash`
unchanged. Operations: `set_width`, `set_anchor`, `set_point`, `append_contours`,
`replace_contours`. Chat, MCP and `live.python` graph nodes use the same contract.

## 4. Models on disk

| Kind | Location | Files |
| --- | --- | --- |
| Shapes | `~/Runebender/models/shapes/`, `<Font>.models/shapes/` | `config.json`, `weights.safetensors`, `vocab.txt`, `manifest.json` |
| Bolden | `~/Runebender/models/bolden/`, `<Font>.models/` | `config.json`, `weights.safetensors`, `vocab.txt` |
| Chat | a folder under a model root that holds a `.gguf` | `.gguf`, optional `model.json`, optional `mmproj*.gguf` for images |
| Whisper | a `whisper*` folder under a model root | `model.bin` |
| Virtua sketch | `~/runebender/models/<id>/` | `manifest.json`, `config.json`, `vocab.txt`, `weights.safetensors`, and `runtime.json` at the root |
| Neural | `<Name>.models/<NNN>/` | output of `train-nufo.sh` |

Model roots: `RUNEBENDER_MODELS` or `~/Runebender/models`, the older
`~/.runebender/models`, and extra roots from `~/.runebender/model_paths` and
`RUNEBENDER_MODEL_PATHS`. Virtua sketch models use `RUNEBENDER_MODELS_DIR` or
`~/runebender/models`. That is the same folder on a case-insensitive macOS
volume and a different folder on Linux.

A `presets.ini` at the model root holds each chat model's settings for a shared
`llama-server` in router mode, one section per folder name; see
[the shared model server](LOCAL-MODELS.md#one-server-for-every-local-agent).

The editor downloads no models. `scripts/bundle_llama.py` copies a pinned
`llama-server` build into the macOS app bundle.

## 5. Status

| Feature | Status |
| --- | --- |
| Chat, voice input, Proof view | Working |
| Local AI rail, Bolden, Weight Debt | Working; the models score only slightly better than a mean shift |
| Local AI Shapes | Working |
| Live MCP, experiments, compiled proofs | Working on macOS and Linux |
| Graph file runs, live Python nodes | Working; live model nodes are not finished |
| Draft with Virtua | Runs; the Arabic quality gate is open |
| Neural training, Model preview | Working |
| Finetune button in the editor | Planned |
| Chat model catalog with download | Planned |
| Sketch redraw with a drawing model | Planned |
| Spacing and kerning models | Planned; named in font-ml, not built |
| Draft ranking model, one model for several families | Planned |

The planning records are [the AI architecture checklist](AI-ARCHITECTURE-CHECKLIST.md)
and [the Virtua Regular AI checklist](VIRTUA-REGULAR-AI-CHECKLIST.md).

`editor_select_points` replaces the active native point selection using canonical IDs from `read_glyph` and a fresh `editor_context` revision. It activates Select, rejects unknown IDs atomically and active gestures, and never changes or saves font data. Pass an empty list to clear selection. Select first, then use guarded `agent_apply` for movement.

`editor_review` controls the native Agent Proof view with a fresh context revision. Use `pin_before` explicitly before a demo edit, `edit` for yellow point selection and movement, then `compare` or `before`/`after` for review. `review_proof` captures the same comparison/reference geometry; no web proof is required. The tool never applies proposals, saves, or grades drawings.

For human/agent handoff, stop issuing mutations when the designer takes over. Read the live unsaved geometry again after their edits; never replay or silently rebase a stale proposal. Capture correction pairs separately and require explicit human approval before adding a training target. Saving is not required for MCP to observe their committed editor changes.
