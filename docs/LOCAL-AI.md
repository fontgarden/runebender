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
  `read_kerning`, `proof`, `specimen`, `proposal_list`, `propose_edits`.
  A turn has at most 6 tool calls.
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

Editor footer, the fourth button. Source: `src/application/view/canvas/proof.rs`,
`src/application/view/render.rs`, `src/application/view/chrome.rs`.

- Replaces the canvas with a proof of the open glyph; the button switches back.
- Shows the current drawing and the latest proposal that names the glyph, side by
  side at one scale in their advance boxes, with Install and Discard.
- Below them, current and proposal in running text at 96, 48 and 24 px per em: the
  Text tool's composition, or a sample of the glyph between n, o, H and O.
- Reads the proposal in the active master only.
- `RUNEBENDER_PROOF_VIEW=1` opens it, and `RUNEBENDER_PROOF_SAMPLE=1` adds a sample
  proposal, for headless screenshots.

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
The live server publishes 47 tools.

| Group | Tools |
| --- | --- |
| Connection and state | `editor_sessions`, `editor_connect`, `project_info`, `editor_context`, `editor_open_glyph`, `editor_set_text` |
| Read and analysis | `font_info`, `read_glyph`, `glyph_inventory`, `design_context`, `analyze_glyph`, `analyze_font`, `read_kerning`, `proof`, `specimen`, `export_proof` |
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
| Chat | a folder under a model root that holds a `.gguf` | `.gguf`, optional `model.json` |
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
