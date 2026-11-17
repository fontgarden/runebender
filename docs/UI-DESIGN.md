# Panel language

A panel section is built from `recipes::Row` values grouped under captions.
There is no other way to put content in a section.
The vocabulary, its rules, and every measurement live in [`src/application/design/recipes/rows.rs`](../src/application/design/recipes/rows.rs); this page is the summary.
`tests/panel_vocabulary.rs` fails the build when a panel module reaches past the vocabulary to a bare label, column, or button.

## The design system

Panels are one part of the design system.
Its entry point is [`src/application/design/mod.rs`](../src/application/design/mod.rs).
Styles come from three layers:

1. The theme files in `assets/themes/default/` give every color and shape value.
2. `src/application/design/` gives the scales, the control states, and the themed stock widgets.
3. `src/application/design/areas.rs` gives each exception for an area of the window, with its reason.

A control names a `ControlKind`.
The state table then gives it a look at rest, under the pointer, pressed, selected, disabled, and with keyboard focus.
`tests/design_system.rs` fails the build when a view sets a color, opacity, stroke, radius, or text size by itself.
That test counts the older exceptions, and each count can only go down.

## Four rules

1. A section is one question. Its title is a short noun phrase in title case. It holds groups, and each group answers one part of the question.
2. A group is a caption and its rows. Rows sit 4 pixels apart. Groups are separated by a hairline that stops at the section margin, with 12 pixels above and below. Sections are separated by the full-width rule. Three spacings, two dividers, no others.
3. Prose is state, not instruction. A section never explains itself at the top. `Row::Meta` is one muted line under the thing it describes. Instructions live where the action is.
4. One `Row::Primary` per section. Everything else is `Row::Secondary` or `Row::Quiet`. Facts go behind `Row::Details`.

Captions and titles are title case, never all caps. Nothing in the view uppercases text, and the gate test keeps it that way.

## The rows

| Row | Looks like | Use for |
| --- | --- | --- |
| `Meta` | One muted line | What the row above is: "14 words, 2 spare" |
| `Prose` | One line in body ink | A note or a result |
| `Field` | Caption over an input | A value you type |
| `Picker` | The current choice with a chevron | One of several models or files; never a stack of toggles |
| `Chips` | Small outlined pills over as many lines as needed | A set of words or tags |
| `Dropdown` | A caption and the chosen name in one row; the list opens under it and closes on a pick | One of many, where the list is too long to keep on screen |
| `Choice` | Segmented control | Two or three exclusive options |
| `Primary` | Filled button, full width | The one thing the section is for |
| `Secondary` | Outlined button, full width | The other way to do it |
| `Quiet` | Muted text button | Rare or reversible: Stop, Cancel, Undo |
| `Status` | One sentence and a thin bar | Something in progress |
| `Tiles` | Squares with a caption each | Samples, layers, masters |
| `Specimen` | Shaped font proof lines | Sidebar spacing, waterfall, and script previews |
| `KeyValue` | Name left, value right | Facts: steps, loss, folder |
| `Details` | Disclosure, closed by default | Rows nobody needs every day |
| `Slider` | Caption and a slider | A value on a range |

If a feature needs a row that is not here, add it to `Row` once, with a reason in its doc comment.

## Build a section

```rust
use crate::application::design::recipes::{self, Group, Row, click};
use crate::application::workspace::Workspace;
use xilem::WidgetView;

fn width_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let model = Group::new(
        "Model",
        vec![
            Row::Picker { choices: vec!["shapes-base".into()], chosen: Some(0), on_pick: std::sync::Arc::new(|_, _| {}) },
            Row::Meta("14 words, 2 spare".into()),
        ],
    );
    let train = Group::new(
        "Train",
        vec![Row::Primary { text: "Train From This Font".into(), on_click: click(|app| app.shapes_train(Default::default())) }],
    );
    recipes::section_of(app, "Local AI Shapes", "Local AI Shapes", vec![model, train])
}
```

`recipes::section_of` owns the header, collapse state, inset, group dividers, and the full-width rule.
`recipes::section_with_details` adds a Details strip under the section rule.
`recipes::panel_stack` stacks sections.
The reference section is `src/application/view/panels/ai_shapes.rs`; the Local AI rail in `panels/local_ai.rs` is the second.
Views read workspace state and forward edits to existing workspace intents; font mutations and undo stay in the commands and font engine.

## Older sections

Sections written before the vocabulary use `recipes::panel_section`, `recipes::section`, `recipes::panel_group`, `recipes::field`, and `design::column` and `design::row` with a `Region` directly.
They are listed in `tests/panel_vocabulary.rs` and leave that list as they move.
Until then: `panel_group` owns an 8-pixel inset and the full-width divider; `section` owns the header and a 4-pixel gap; section bodies add no outer padding.
Read colors from `design::theme::Palette` and measurements from `design` instead of assigning local colors, radii, or pixel values.
Let the dock own its width; section contents fit the available width.

## Center proof page

The proof page uses the shared spacing scale through `design::areas::proof`.
One unit is 8 pixels: compact button and label gaps are half a unit, content bands have one unit above and below, and section and specimen-column gaps are two units.
The outer page inset uses the existing three-unit `Space::Xxl` token.
`recipes::proof::band` owns vertical padding; the containing stack has no additional gap.
Rules sit between bands, so a control row has equal clearance above and below.
Optional sections return no widget when absent, instead of leaving empty wrappers that add phantom gaps.
Mode and revision buttons share the same standard control height and compact gap.
The separate Toggle action changes only the visible revision, keeping its controls, specimen scale, and baseline stationary.

## Built-in theme baseline

Theme files begin with `formatVersion`, `id`, and `name`, followed by the Base UI and Rainbow palette definitions.
The built-in Rainbow palette contains the seven mark-picker hues: red, orange, yellow, green, blue, purple, and pink.
The `[drawing]` section follows the palettes and controls how their colors appear on glyph tiles and outline points.
It contains `markStep`, `markStyle`, `markOutline`, `markInk`, `pointStyle`, `pointOutline`, and `pointHalo`.
Glyph tiles, the metrics card header, the mark picker and points all use the same Rainbow colors, so a mark has one color everywhere.
Existing custom themes can keep these settings at the top level, but must not also define the same setting in `[drawing]`.
Dark, Dark Gray, Gray, Light Gray, Light, Strawberry, and Campfire explicitly define panel, tile, and control rounding, slider states, header ink, backdrop tint, and panel shadows.
Gray uses 6-pixel tile corners; the other built-ins use 8-pixel tile corners.
All built-ins use 10-pixel panel corners and 4-pixel control corners while keeping their own palettes and point styles.
Optional `code*` roles color the code editor in script nodes: `codeBackground`, `codeText`, `codeKeyword`, `codeFunction`, `codeType`, `codeString`, `codeNumber`, `codeComment`, `codeConstant`, `codeOperator`, `codeLineNumber`, and `codeSelection`.
A theme without them uses its own text and mark colors on its field surface.
Sidebar tabs are a row of equal icon buttons with control corners, 6-pixel gaps and 6-pixel insets.
The open tab takes the side panel's face, and the other tabs keep the `inactiveTab` face and dimmer ink.
No tab changes size or position when it opens.
Icons that are inset in their frame draw at 14 pixels and icons that fill their frame draw at 12, so all tab icons look the same size.
Optional `text.inactiveTabInk` supplies solid inactive icon ink; older themes retain their previous opacity treatment.
Panel outlines sit outside the content clip, with matching concentric radii and paint bounds that include the full stroke.
Selected glyph and sidebar labels use the theme's yellow mark; other selected controls use `controlSelectedInk`.
Both inks must remain readable against `controlSelected`, including in Dark.
Dark uses charcoal glyph tiles with colored outlines and matching glyph/caption ink; the other built-ins retain filled marks.
The native grid must honor `[drawing] markStyle` rather than assuming a filled mark.
The floating metrics card follows the same mark treatment.
The inspector outline preview uses `surfaces.glyphPreview`, falling back to `surfaces.canvas` in existing custom themes.
Gray uses `baseUi.08` for that preview while its main drawing canvas stays `baseUi.07`.
The preview boundary owns a full-strength section keyline inside the scroll viewport.
Inspector preview markers use 75% of the main editor point radius.
Optional `roles.proofInk` separates proof-strip type from neutral `previewFill` in the editing canvas.
Optional `roles.headerActiveInk` colors active title-bar tools and tabs without changing the document title.
Optional `roles.controlSelectedOutline` colors selected control keylines.
Optional `roles.savedInk` and `roles.unsavedInk` keep save-state text readable without changing glyph mark colors.
Older custom themes retain their previous colors when these roles are absent.
Dark uses green active controls, yellow proof type, and subdued neutral field borders to keep the glyph primary.
Dark Gray uses Gray's filled tiles, neutral proof ink, and yellow selected labels on charcoal panels.
Its colored ring points keep their edges visible on a dark canvas.
Both dark themes use slider knobs one Base UI step darker than their panels and brighten the fill during interaction.
Light pairs pale gray window chrome with near-white panels and a white drawing surface.
Light Gray uses the same light chrome with softer gray panels and canvas; its fields are one Base UI stop above the panel.
Both light themes and Strawberry use a deep green Saved label and dim red Not saved label, with at least 4.5:1 contrast against their panels.
Strawberry applies a rose Base UI ramp across the window, panels, controls, and ink.
Campfire applies an ember-brown and copper Base UI ramp with cream ink.
These showcase themes preserve Gray's Rainbow mark colors and shared geometry.
Native title-bar appearance follows the brightness of the opaque window ground at startup and after theme changes.
Light, Light Gray, and Strawberry request light native appearance; the other built-ins request dark appearance.
The macOS backdrop uses the matching Vibrant Light or Vibrant Dark appearance.
Linux keeps flat application surfaces and requests the same native appearance where the window manager supports it.
Compare the glyph grid and edit mode in all built-ins at the same viewport when adding or changing a built-in theme.
Headless proofs cover solid-mode layout and colors; wallpaper blur still needs native review.

## Window backdrops

All built-ins enable the optional macOS `[window] blurBackground` setting.
Keep it `false` or omit it for the supported solid background; Gray uses `surfaces.app` and `surfaces.header`, both `baseUi.02`.
View > Translucent Window flips the setting while the application runs, for comparing the two; the theme's own value applies again at the next start.
Linux, browser, and headless hosts always use flat colors.
The native experiment places an AppKit behind-window effect below the GPU view and adds a `surfaces.backdropTint` overlay.
That optional color defaults to `surfaces.app` in older themes.
Dark uses the approved `#404040` tint; Dark Gray uses `#505050`; Gray uses `#808080` at 50% opacity to keep the window ground lighter over dark wallpaper.
Dark and Dark Gray use 25% opacity; Light uses a `#F0F0F0` tint at 65% to stay light over dark wallpaper.
Light Gray uses `#E5E5E5` at 65%, Strawberry uses `#F3B8C4` at 75%, and Campfire uses `#774D2F` at 38%.
Unsupported hosts keep their opaque theme colors.
`window.blurTintOpacity` is a diagnostic opacity from 0 to 1, with a default of 0.8 when omitted.
On 2026-09-30, the user's desktop capture `1852-005` confirmed wallpaper blur in the full editor with transparent startup and zero application tint.
`window.shadowPanels = false` suppresses only main-panel shadows when the native backdrop is active.
Solid mode retains `geometry.shadowPanel`; glyph tile shadows and other application shadows are independent.
The independent tint gently lifts dark wallpaper and reduces its color influence.
To use the backdrop, enable `blurBackground`; its color and opacity are independent of the solid background.
Run with `RUNEBENDER_FRAME_STATS=1` to have the window render every frame and report its frame cadence on stderr, which is how the backdrop's cost is measured.
The original zero-overlay test can be reproduced with `blurTintOpacity = 0.0`.
Earlier 0.8 and 0.65 tint settings appeared washed out.
The small Xilem example in `examples/macos_vibrancy.rs` reproduced bright, jagged edges on translucent text and shapes.
An isolated comparison with Vello 0.10.0 and wgpu 30.0.1 still reproduced those artifacts.
Backporting [wgpu #9922](https://github.com/gfx-rs/wgpu/pull/9922) made the demo and editor text smooth, as confirmed by the user on-screen.
The native dependencies now pin a Xilem fork that applies the equivalent Metal premultiplication workaround to the supported renderer versions.
Remove that workaround when the supported dependency graph includes the upstream correction.
[Xilem #1852](https://github.com/linebender/xilem/issues/1852) is closed with these findings.
Floating panels must clip their complete content subtree to the theme radius.
Painting a flat background over their square corners fails when the real backdrop varies, so the panel frame paints only its outline and outside shadow.
The rounded clip brackets child painting with matching pre-paint and post-paint clips; keep panel descendants in the inline paint layer.
The user accepted the rebuilt native result after the rounded clipping change.
Verify the result on the actual desktop, since isolated window captures showed flat gray even when the minimal probe visibly blurred.
The experiment and tested settings are recorded beside `set_backdrop` in `src/application/platform/window.rs`.

## Center-panel footers

Use `recipes::panel_footer(pal, controls)` for the center bottom bar in overview and editor modes.
The middle of the bar is the document readout: the font's path and save state, centred.
The path gives way to the file name when the bar is narrow, and the name is elided below that; a transient note replaces the readout while it shows.
Bare footer icons show hover and press in their ink, never as a tile behind them.
It owns the full-width upper divider, the 28-pixel height, centered controls, and the outer inset.
The 10-pixel side inset gives controls optical clearance from the curved panel corners; the 16-pixel icons remain vertically centered with 6 pixels above and below.
Neutral slider tracks use the shared 2-pixel `SLIDER_TRACK_THICKNESS`, independently of the thumb size and hit area.
Keep icon clusters in unpadded `Region::Inline` rows and use `STATUS_ICON_SIZE` for compact footer buttons.
Do not add a local footer height or horizontal padding; change the shared recipe and geometry when the footer contract changes.

## Components in place

A component opens in place: the session edits its base glyph, but the canvas draws it where the composite puts it.
Double-click a component, or choose Edit Component in Place from its context menu, to open it.
The text line keeps the composite as its active sort, so shaping does not change.
Every glyph that uses the base follows the edit when it is committed.
The open base keeps the component tint, and the rest of the composite stays around it.
The metrics card names the composite, as in `dotbelow-ar in beh-ar`, and the metrics box keeps the composite's advance.
Escape, or a double-click on empty space in the sort, goes back to the composite.

The editor footer's components toggle shows the points of every part of the composite at once.
A press on one of those points moves the session into that part and drags the point.
Undo moves into the part whose edit it undoes, so the history stays in order across parts.
Open a base glyph in its own tab from the context menu when it needs its own text line.

## Check a new section

Check the expanded and collapsed section, including its last control and the next divider, in a headless screenshot using the default Gray theme.
Check its narrowest supported dock width and any other mode that reuses the same contents.
Run focused tests for changed interaction or layout behavior and the repository formatting checks.
Reuse shared recipe tests for common spacing instead of adding identical tests to every section.
Headless screenshots do not verify native pointer, input method, accessibility, or GPU behavior; validate those separately when the change requires it.
See [`AGENTS.md`](../AGENTS.md) for the full validation guidance and the [design principles](https://runebender.org/docs/design-principles.html) for the broader visual rules.

## Headless screenshots

Set `RUNEBENDER_SCREENSHOT` to a PNG path to render one frame of the editor and exit without a window:

```sh
RUNEBENDER_SCREENSHOT=/tmp/shot.png cargo run --release -- path/to/Font.designspace
```

The capture uses the same text and font settings as the window.
The next two sections list every `RUNEBENDER_*` variable that the code reads.

### Launch variables

The editor reads these variables once at launch, in `LaunchOptions::from_env` in `src/application/launch_options.rs`.
The same table is in that file, and a test compares the two.
Most of them exist for captures, but each one also applies to a window unless its row says "in a capture".

| Variable | Effect |
| --- | --- |
| `RUNEBENDER_SCREENSHOT=<path>` | Render one frame to this PNG file and exit without a window. |
| `RUNEBENDER_SIZE=1100x720` | Logical size of the capture; the default is 1100 by 720. |
| `RUNEBENDER_SCALE=2` | Device-pixel scale of the capture at the same logical layout. |
| `RUNEBENDER_GLYPH=<name>` | Open this glyph in the editor. |
| `RUNEBENDER_OPEN=<name>` | Create the workspace with the editor already on this glyph, without the steps of opening it. |
| `RUNEBENDER_SELECTED=<name>` | In a capture, select this glyph in the grid. |
| `RUNEBENDER_SELECTALL=1` | Select all points in the open glyph. |
| `RUNEBENDER_EXPAND=<key>[,<key>]` | In a capture, expand the panel sections with these collapse keys. |
| `RUNEBENDER_COLLAPSED=<key,...>` | Collapse these panel sections. |
| `RUNEBENDER_HIDDEN_DOCKS=<left,right>` | Fold these docks away, as the footer toggles do. |
| `RUNEBENDER_CAT=<Number\|Symbol\|Mark>` | Select this category in the sidebar and expand the Categories section. |
| `RUNEBENDER_SCRIPT=<label>` | Select this script group in the sidebar and open its coverage rows. |
| `RUNEBENDER_VIEW_MODE=list` | Show the overview as a list. |
| `RUNEBENDER_RAIL=<ai\|chat\|scripts\|shapes>` | Open this tool pane. |
| `RUNEBENDER_TOOL=<interpolation\|hand\|metaball\|measure\|label\|text\|sketch\|shapes>` | Start with this tool instead of the selection tool. |
| `RUNEBENDER_VIEW=<option,...>` | Turn on canvas view options: `comb`, `thickness`, `continuity`, `colorize`, `handles`, `segments`, `bearings`, `popcount`, `components`. |
| `RUNEBENDER_COMPONENT=<n>` | Open the open glyph's component at index `n` in place, inside its composite. |
| `RUNEBENDER_AXIS=wght=500,wdth=80` | Set the design-space location. |
| `RUNEBENDER_MASTER=<name>` | Make this master active, by its style name. |
| `RUNEBENDER_SHOW_LAYERS=1` | Turn on the eye of every layer of the open glyph. |
| `RUNEBENDER_LAYER=<name>` | Edit this layer of the open glyph, by its name in the Layers panel. |
| `RUNEBENDER_BACKGROUND=0` | Hide the background layers. |
| `RUNEBENDER_MARK_CLOUD=1` | Show the mark cloud and expand the Background section. |
| `RUNEBENDER_REFERENCE=<name>` | Fill the Reference field with this glyph name. |
| `RUNEBENDER_TEXT=<text>` | Start a text session in the editor with this text. |
| `RUNEBENDER_TEXT_FEATURES_DISABLED=<tag,...>` | Turn off these OpenType features in the text session. |
| `RUNEBENDER_TEXT_SCRIPT=<tag>` | Shape the text session with this OpenType script tag. |
| `RUNEBENDER_TEXT_LANGUAGE=<tag>` | Shape the text session with this OpenType language tag. |
| `RUNEBENDER_PREVIEW_TEXT=<text>` | Set the text of the proof strip. |
| `RUNEBENDER_PREVIEW_BLUR=<radius>` | Blur the proof strip by this radius, from 0 to 8. |
| `RUNEBENDER_PREVIEW_INVERT=1` | Invert the proof strip. |
| `RUNEBENDER_PREVIEW=<pieces\|model>` | In a capture, show this view of the proof strip; `model` waits for the first drawing of the trained font. |
| `RUNEBENDER_NEURAL_STATUS=<text>` | In a capture, show this training status in the Neural section. |
| `RUNEBENDER_PROOF_VIEW=<1\|toggle\|before\|waterfall\|text>` | Open the proof view: `1` opens Compare, `toggle` and `before` open Toggle on the right-hand or left-hand revision, `waterfall` and `text` open those pages. |
| `RUNEBENDER_PROOF_DARK=1` | Open the proof page in dark mode. |
| `RUNEBENDER_PROOF_TEXT=<text>` | Set a custom sample of at most 4096 characters for the reading proof. |
| `RUNEBENDER_PROOF_MENU=1` | Open the sample menu of the reading proof. |
| `RUNEBENDER_PROOF_SAMPLE=1` | With `RUNEBENDER_GLYPH`, create and select an in-memory sample proposal for the proof view. |
| `RUNEBENDER_LABEL_SAMPLE=<n>` | With `RUNEBENDER_GLYPH`, select this sample of the label tool. |
| `RUNEBENDER_SCRIPT_DRAFT=<code>` | Open a script draft with this content in the Scripts pane. |
| `RUNEBENDER_SCRIPT_NAME=<name>` | Name the script draft; the default is `headless-recipe.py`. |
| `RUNEBENDER_SCRIPT_PARAMETERS=<json>` | Set the parameters of the script draft. |
| `RUNEBENDER_CHAT_SAMPLE=<1\|listening\|detail\|busy\|models>` | Fill the Chat pane with a sample exchange; the named values show the open microphone, an open detail, a running tool or the model list. |
| `RUNEBENDER_MODEL=<dir>` | Load the outline model in this folder. |
| `RUNEBENDER_PROPOSAL_PREVIEW=<task>` | Show the review overlay of the proposal with this task name. |
| `RUNEBENDER_NODES=<file>` | Open this graph file. |
| `RUNEBENDER_MODE=graph` | Start in the Graph view. |
| `RUNEBENDER_GRAPH_SCRIPT=<index>` | In a capture of the Graph view, add a code node with this example script. |
| `RUNEBENDER_GRAPH_EXPANDED=1` | In a capture of the Graph view, show the first proof image expanded. |
| `RUNEBENDER_FRAME_STATS=1` | In a window, render every frame and report the frame cadence on stderr. |

### Variables that other code reads

Other code reads these variables where it uses them: editor views and tools, platform code, the command line, the font engine and the tests.
A test checks that this section and the previous one name every `RUNEBENDER_*` variable in `src`.

| Variable | Effect |
| --- | --- |
| `RUNEBENDER_TEXT_SELECTION=<start>:<end>` | Select this range of the text session. |
| `RUNEBENDER_NODES_CONTENT_FIXTURE=1` | Open the fixed code-and-image graph layout that interface reviews use. |
| `RUNEBENDER_IN_WINDOW_MENU=1` | On macOS, show the in-window menu that the other platforms use. |
| `RUNEBENDER_MENU_OPEN=<title>` | Open the in-window menu with this title. |
| `RUNEBENDER_PYTHON=<path>` | The Python executable that runs recipes and graph code nodes. |
| `RUNEBENDER_FONT_ML=<path>` | The `font-ml` executable. |
| `RUNEBENDER_AI_DEVICE=<device>` | The device that graph model nodes request; the default is `auto`. |
| `RUNEBENDER_CORE=<path>` | The executable that Chat runs for its tools; the default is the editor itself. |
| `RUNEBENDER_CHAT_ENDPOINT=<url>` | An OpenAI-compatible chat server to use instead of a local model. |
| `RUNEBENDER_CHAT_MODEL=<name>` | The model name sent to `RUNEBENDER_CHAT_ENDPOINT`; the default is `font-ml`. |
| `RUNEBENDER_MODELS_DIR=<dir>` | The folder of Virtua sketch models; the default is `~/runebender/models`. |
| `RUNEBENDER_SKETCH_REPOSITORY=<dir>` | The sketch runtime checkout, instead of the one in `runtime.json`. |
| `RUNEBENDER_SKETCH_CHECKPOINT=<dir>` | The sketch checkpoint for a request that selects no model package. |
| `RUNEBENDER_SAMPLE_SHAPES=<dir>` | The sample shapes UFO of the Shapes pane. |
| `RUNEBENDER_THEME=<id>` | Theme ID; the default is Gray. |
| `RUNEBENDER_THEME_PATH=<dir>[:<dir>]` | More folders of theme files. |
| `RUNEBENDER_EDITOR_ZOOM=<zoom>` | In a capture, frame the editor canvas at this zoom. |
| `RUNEBENDER_DEBUG_BACKDROP=1` | On macOS, report the state of the window backdrop on stderr. |
| `RUNEBENDER_LIVE_SESSION=<socket>` | The live editor session that the `runebender agent` commands and the Chat runner use. |
| `RUNEBENDER_DOCS=<dir>[:<dir>]` | More folders for the documentation search of the agent commands. |
| `RUNEBENDER_CONTEXT_PATH=<dir>[:<dir>]` | More folders of personal design context files. |
| `RUNEBENDER_MODELS=<dir>` | The model folder; the default is `~/Runebender/models`. |
| `RUNEBENDER_MODEL_PATHS=<dir>[:<dir>]` | More model folders. |
| `RUNEBENDER_LLAMA_SERVER=<path>` | The `llama-server` executable. |
| `RUNEBENDER_WHISPER_SERVER=<path>` | The `whisper-server` executable. |
| `RUNEBENDER_WHISPER_MODEL=<file>` | The speech model for voice input. |
| `RUNEBENDER_TEST_FONTS=<dir>` | Tests: the Virtua Grotesk `sources` folder for the full-font tests. |
| `RUNEBENDER_BENCH_FONT=<path>` | Frame probe: the designspace or UFO to measure. |
| `RUNEBENDER_BENCH_GLYPH=<name>` | Frame probe: the glyph to open; the default is `a`. |
| `RUNEBENDER_BENCH_OVERVIEW=1` | Frame probe: measure the overview grid instead of a point drag. |
| `RUNEBENDER_BENCH_REPEAT=<count>` | Frame probe: repeat the edit and its rebuild for a sampling profiler. |
| `RUNEBENDER_BENCH_GLYPHS=<count>[,<count>]` | Large-font benchmark: the glyph counts of the fixtures. |
| `RUNEBENDER_BENCH_KEEP=<dir>` | Large-font benchmark: keep the fixture of each size in this folder. |
| `RUNEBENDER_REAL_LLAMA=<gguf>` | Ignored test: start `llama-server` with this model and ask one question. |
| `RUNEBENDER_REAL_WHISPER=<wav>` | Ignored test: transcribe this 16 kHz mono file with `whisper-server`. |
| `RUNEBENDER_REAL_MICROPHONE=1` | Ignored test: record two seconds from the default input. |
| `RUNEBENDER_REAL_VOICE=<designspace>` | Ignored test: run voice input from the microphone to the Chat field. |
| `RUNEBENDER_REAL_CHAT_FONT=<designspace>` | Ignored test: run one Chat turn against the shared model server. |
| `RUNEBENDER_REAL_CHAT_MODEL=<name>` | Ignored test: the model for that Chat turn. |
| `RUNEBENDER_RECIPE_EXAMPLES=<dir>` | Tests: the recipe examples folder; the default is `tests/fixtures/recipes`. |
| `RUNEBENDER_SCRIPT_FONT=<ttf>` | Ignored test: the font that the specimen script starters render. |
| `RUNEBENDER_SCRIPT_OUT=<dir>` | Ignored test: keep the specimen script images in this folder. |
| `RUNEBENDER_ANNOTATED_MCP_RENDER=<path>` | Tests: write the annotated proof scene of the live tools to this PNG file. |
| `RUNEBENDER_ANNOTATED_PAGE_RENDER=<path>` | Tests: write the annotated proof page to this PNG file. |
| `RUNEBENDER_HEADER_RENDER=<path>` | Tests: write the proof page header to this PNG file. |
| `RUNEBENDER_PENCIL_RENDER=<path>` | Tests: write the proof pencil ink to this PNG file. |
| `RUNEBENDER_GRID_FIT_PROOFS=<dir>` | Tests: write the grid fit images to this folder. |
| `RUNEBENDER_GRID_LABEL_PROOF=<path>` | Tests: write the grid caption image to this PNG file. |
| `RUNEBENDER_METABALL_PROOF_DIR=<dir>` | Tests: write the metaball panel images to this folder. |
| `RUNEBENDER_NODES_PROOFS=<dir>` | Tests: write the graph comparison images to this folder. |
| `RUNEBENDER_RESIZE_EVIDENCE=<dir>` | Tests: write the splitter images to this folder. |
| `RUNEBENDER_HOI_PROOF_DIR=<dir>` | Tests: write the higher-order interpolation proofs to this folder. |
