# Arabic text layout: research and plan

Date: 2026-11-23.
Status: plan, awaiting owner sign-off.
This document records why the text layout of the editor is being rebuilt, what Fontra, Counterpunch and Glyphs do, what was measured, the design, and the work checklist.

## 1. Goal

Runebender must be a first-class editor for Arabic type design, including Quranic typography.
The text line in the editor must show the glyph being edited in real words, with correct joining, marks, cursive attachment and kerning.
The line must not move unless the designer changed something that moves it.
The line must match the exported font.
The line must keep working while the font is unfinished.

The target is to match every Arabic design feature of Counterpunch, and to exceed it in feedback speed, stability and Arabic tools.

## 2. The problem that started this work

On 2026-11-23 a point nudge in `reh-ar.fina` made the text line move and then return.
The font had one glyph, `tet-hb`, whose contours did not interpolate.

The cause was a design fault, not one defect.

- Text layout came from one complete variable font compiled by fontc, outlines included.
- Any point edit made that font stale and started a new compile.
- One glyph that did not interpolate made the compile fail for the whole font.
- The text code had three shaping paths with different results: the compiled font, a font made from the feature file alone, and the built-in Arabic joining rules.
- The line moved each time the editor changed from one path to another.

Commit `3f69c3c` kept the last compiled font after a failure.
That was a workaround.
Commit `6ca726a` added a layout font: the same variable compile without outlines, rebuilt only after an edit that changes layout.
That removed the dependence on outlines, but section 4 shows that it still fails on real Arabic sources.
This plan replaces both.

## 3. Research

### 3.1 Fontra

Source read: `fontra/fontra` at `71ec125` (2026-11-23).
The older `googlefonts/fontra` repository is stale and has no shaper.

- Fontra compiles a shaper font with `fea-rs` from the feature code, glyph order, axes and glyph classes.
  The font has GSUB, GPOS and GDEF, and no outlines, advances or character map.
- HarfBuzz gets advances and the character map from the live glyphs through callbacks.
- Substitutions and hand-written positioning are real lookups that HarfBuzz applies.
- The generated `curs`, `kern`, `mark` and `mkmk` features are not compiled.
  Fontra applies them in JavaScript from the live kerning and anchors.
- `fea-rs` reports where each `# Automatic Code` comment is in the lookup order.
  Fontra uses a HarfBuzz message callback to stop at that lookup and change glyph positions.
- A width, anchor, kerning or point edit shows with no compile.
- A glyph that does not interpolate uses its nearest source; other glyphs are not affected.
- When the feature code does not compile, Fontra keeps the previous shaper.
- Fontra has a shaping debugger, feature switches, and script, language and direction controls.

Limits that matter for Arabic:

- One line is one HarfBuzz run with one direction and one script.
  There is no bidirectional layout and no script segmentation ([issue 2432](https://github.com/fontra/fontra/issues/2432)).
- The emulated features have fixed behavior: no lookup flags, no mark filtering sets, no script or language scope, no split of kerning by script.
- Exact emulation needed many corrections, most found by Khaled Hosny with the Arabic font Raqq ([2437](https://github.com/fontra/fontra/issues/2437), [2521](https://github.com/fontra/fontra/issues/2521), [2507](https://github.com/fontra/fontra/issues/2507)).

Reason for the design: Fontra loads glyphs on demand from a server.
Just van Rossum: "Compiling a full font in real-time is just not a viable option" ([blog](https://blog.fontra.xyz/blog/opentype-harfbuzz/), [issue 2381](https://github.com/fontra/fontra/issues/2381)).

### 3.2 Counterpunch

Source read: `counterpunchspace/editor` at `384f95d` (2026-11-20).
Counterpunch uses Babelfont, fontc compiled to WebAssembly, and HarfBuzz, which is the stack closest to Runebender.

- Counterpunch compiles a real variable font for the glyphs in the text plus their GSUB closure, with outlines.
- HarfBuzz shapes the text and also draws the glyphs from the compiled font.
  Only the selected glyph is drawn from the source model.
- Text is split into bidirectional runs before shaping.
- fontc generates the kerning and mark features, so they are the same as in the export.
- The Babelfont filter `DropIncompatiblePaths` removes paths that do not interpolate, so one glyph cannot stop the compile.
- When a compile fails, the previous font stays.
- Each edit has a type, and each type has a compile mode: `outline-only`, `anchor-only`, `kerning-only` or `full`.
- During an outline or sidebearing drag the text is not shaped again; advances are patched from the JavaScript model.
- Arabic tools: a generator for `isol`, `init`, `medi`, `fina` and `rlig`; composition of letters from a skeleton and dot marks through `ccmp`; a plug-in point for `mkmk` code; separate right-to-left kerning; feature switches.
- Not found: cursive attachment tools, script or language selection, a shaping debugger, collision detection, kashida tools.

Reason for the design: output correctness.
"If the font behaves correctly in Counterpunch, it's a correct font!" ([documentation](https://counterpunch.space/documentation/getting-started/01-what-is-counterpunch)).
Yanone: "I don't emulate anything but compile the full font" ([fontc issue 1897](https://github.com/googlefonts/fontc/issues/1897)).
In the same issue he reports 5 ms for normal fonts and 50 ms for very large fonts, and calls that "not good enough".

Costs recorded by the project itself:

- The compile policy document has 34 numbered rules.
- A comment in the canvas code says that shaping again with a preview font "would jump the canvas".
- A post-mortem describes a jump on the second sidebearing drag.
  The cause was a session flag cleared before the last asynchronous preview compile arrived.
- The changelog has entries for stale compiled text and for `.notdef` glyphs.

The jump defects of Counterpunch and of Runebender have the same cause.
Both editors show values from two sources, a live model and an asynchronous compile, and the picture changes when one replaces the other.

### 3.3 Glyphs

Glyphs is the tool most Arabic designers use.
Its Edit View previews substitution, plain kerning and cursive attachment.
It does not preview mark positioning, contextual kerning or hand-written positioning ([handbook](https://handbook.glyphsapp.com/previewing-and-testing/), [contextual kerning](https://glyphsapp.com/learn/contextual-kerning)).
Designers export the font and test it in other applications.

### 3.4 What Arabic fonts need from layout

- Lookup order.
  Amiri puts `# Automatic Code` in `curs`, `kern`, `mark` and `mkmk`, with hand-written lookups before and after.
  Virtua Grotesk has `feature mark { # Automatic Code ... }` followed by manual mark lookups.
- Lookup flags and filters: `IgnoreMarks`, `RightToLeft`, mark filtering sets.
- Kerning that depends on marks.
  Amiri has `pos @Raa_1' <98 0 98 0> fatha-ar alefabove-ar.isol [...]`.
- Contextual positioning after the generated marks, to repair collisions of dots and vowel marks ([TypeDrawers](https://typedrawers.com/discussion/5227/need-help-with-arabic-contextual-positioning)).
- Marks on ligatures, with one anchor for each component.
- Cursive attachment for Nastaliq and Ruqah styles, where one anchor edit moves a whole word.
- Kerning rules for right-to-left text: fontc and ufo2ft split pairs by script and direction, and a right-to-left pair changes both placement and advance.
- Tolerance for unfinished masters.
  Amiri has the anchor `HamzaBelow` on `aBaa.medi` in Bold and not in Regular.

## 4. Measurements

Measured on 2026-11-23 with a temporary probe, test profile, one thread.
Virtua Grotesk has 913 glyphs and two masters.
Amiri has 6,202 glyphs and two masters; its masters were loaded without the hand-written feature files.

| Route | Virtua Grotesk | Amiri |
|---|---|---|
| Complete variable compile with fontc | 68 ms | fails |
| Layout font of `6ca726a`: variable, no outlines | 27 ms | fails |
| fontc, one master, no outlines | fails | 383 ms |
| fontc, variable, 120 glyphs, no feature file | 2 ms | fails |
| Generate mark features from live anchors | 0.13 ms | 9 ms |
| `fea-rs` compile of the whole feature text | 1.2 ms | 32 ms |
| Shape one line | 0.02 ms | 0.04 ms |

Findings:

- Both variable routes fail on Amiri because of the one anchor that is missing in Regular.
  The layout font of `6ca726a` is therefore not good enough.
- The one-master fontc route fails on Virtua Grotesk with a feature validation error when the axes are removed.
- The `fea-rs` route is 10 to 20 times faster than fontc and has no dependence on other masters.
- The `fea-rs` route gives the same glyphs as fontc for Arabic, Hebrew and Latin samples.
- The `fea-rs` route in Runebender today does not position marks for Virtua Grotesk.
  `text::features` adds no generated lookups when the feature file defines `mark`, and it does not read `# Automatic Code`.
  That is why the editor needed the complete compile.

Facts about the libraries:

- `harfrust` 0.5.2, the pinned shaper, has no message callback and no advance callbacks.
- `harfrust` 0.15 has a message callback that can read the buffer and skip a lookup but cannot change positions.
  The method of Fontra is therefore not available in Rust.
- `harfrust` 0.13.3 has font functions for custom advances and uses the same `read-fonts` version as the other pinned crates.
- `fea_rs::compile` builds layout tables from a glyph order alone and reports the insert markers.
- Generated lookups enter `fea-rs` through the `FeatureProvider` trait.
  `fontbe`, the back end of fontc, implements it for kerning and marks.
  Runebender passes `NopFeatureProvider` today.
- fontc removed incremental compilation in January 2025.
- The pinned Babelfont has the `DropIncompatiblePaths` filter.

## 5. Design

### 5.1 Principle

Layout is a pure function, computed in the same frame as the edit:

```text
layout = shape(text, options, tables)
tables = compile(feature code, generated lookups(live values at the location))
```

- The tables are real GSUB, GPOS and GDEF, applied by the real shaper.
  Lookup order, flags and filters are those of the font, with no emulation.
- The generated lookups are built from live advances, anchors and kerning at the displayed location.
  They enter the compile at the `# Automatic Code` markers, through the `fea-rs` `FeatureProvider` interface that fontc uses for export.
- No outline, no variation data and no other master takes part.
- The complete fontc compile continues in the background for export and proofs.
  It is a checker: it never supplies positions or outlines to the edit canvas.

This takes the data flow of Fontra (live values, no font compile in the editing loop) and the fidelity of Counterpunch (real tables, real shaper).

### 5.2 Rules that prevent jumps

Each rule answers a defect seen in Runebender or Counterpunch.

1. One layout path.
   No fallback gives different positions for the same font state.
   The compiled preview font, the layout font of `6ca726a`, and the joining-rules fallback are removed from the editing path.
2. No time in the function.
   The same text, feature code, values and options always give the same layout.
   No result depends on whether a background job has finished.
3. Synchronous.
   The tables are rebuilt and the text is shaped before the frame that shows the edit.
   There is no debounce and no "compiling" state for positions.
4. One code path for a drag and for a commit.
   A drag step and the final commit call the same function.
   There are no preview modes and no edit-type classification.
5. One source of outlines.
   The edit canvas draws every glyph from the live source at the displayed location.
   A compiled font never replaces those outlines.
6. Failures stay local.
   A glyph that does not interpolate, or an anchor that is missing in one master, changes only that glyph, and the editor reports it on that glyph.
7. One permitted retained state.
   When the feature code does not compile, the editor keeps the last feature code that compiled, still combines it with live values, and shows the error.
   This is the only case in which layout uses old data.
8. The view is anchored.
   When an edit legitimately changes layout, the edited glyph stays at the same place on the screen, in left-to-right and right-to-left lines.
9. The checker cannot write to the view.
   A difference between the live layout and the fontc compile is shown as a warning, never applied.

### 5.3 Trade-offs

- Two feature generators can differ: the one in the editor and the one in fontc.
  First choice: drive the kerning and mark writers of `fontbe` directly, so there is one implementation.
  Second choice: port them, and compare with fontc in tests and in the background checker.
- Values between masters come from interpolation in the editor, not from the variation data of a compiled font.
  Small rounding differences are possible; the checker reports them.
- The whole-font compile on Amiri takes about 42 ms, which is too slow for each drag frame.
  The generated lookups are therefore limited to the glyphs that the text can produce, and the parsed feature code is cached.
  The budget is 4 ms for one edit of one line at the size of Amiri.
- A shaping debugger with steps needs a newer `harfrust`.
  The upgrade is a separate, reviewed dependency change.
- Variable feature syntax in a feature file is resolved at the displayed location.

### 5.4 Comparison

| | Fontra | Counterpunch | Runebender, this plan |
|---|---|---|---|
| Substitution and manual positioning | Real | Real | Real |
| Generated kerning, marks, cursive | Emulated in application code | Real, compiled by fontc | Real, compiled from live values |
| Lookup order with `# Automatic Code` | Through a HarfBuzz callback | Real | Real |
| Edit feedback | Immediate | After a compile, 5 to 50 ms and more | Same frame |
| Asynchronous state in the edit view | None | Much | None |
| One unfinished glyph | Local | Paths dropped | Local, reported |
| Bidirectional text | No | Yes | Yes |
| Outlines in the edit view | Live | From the compiled font | Live |
| Match with export | Approximate | By construction | Shared generator, plus a checker |

## 6. Arabic design features

Features that Counterpunch has, to match:

- Bidirectional runs and per-run shaping (Runebender has these).
- Generator for `isol`, `init`, `medi`, `fina` and `rlig` from glyph names.
- Letters composed from a skeleton and dot marks through `ccmp`.
- Right-to-left kerning: storage, editing, and the merge rules of fontc.
- A place for generated `mkmk` code for harakat.
- Feature switches (Runebender has these).
- Incompatible paths do not stop the preview.

Features beyond Counterpunch:

- Marks, cursive attachment and kerning that follow the pointer during a drag, in the same frame.
- Cursive attachment from entry and exit anchors, with editing of a whole attached word.
- Marks on ligatures with component anchors.
- Script and language selection for shaping (Runebender has these).
- A collision overlay for dots and vowel marks, which GPOS cannot detect.
- Test strings for one glyph in every joining context and with every mark class.
- A Quranic test set: Quranic annotation marks, small letters, stacked marks, long ligatures.
- Kashida and tatweel preview.
- A shaping debugger that shows the text after each lookup, with the source line.
- A report of the differences between the live layout and the exported font.

## 7. Work checklist

Each item names its verification.
Phases 1 to 4 are the new system; phases 5 to 7 build on it.

### Phase 0: decisions by test

- [ ] 0.1 Find out if the kerning and mark writers of `fontbe` can run from Runebender with values for one location, without the fontc pipeline.
      Result: "reuse" or "port", recorded in this document.
      Source study, 2026-11-23: reuse is possible, and no private item blocks it.
      `fontir` and `fontbe` contexts and the IR types have public constructors; `FeaRsKerns` and `FeaRsMarks` implement `FeatureProvider`.
      Open: the cost of one edit, because the writers rebuild every group and the GSUB closure each time.
      A measured test decides between reuse, port, and reuse for marks with a port for kerning.
      Result, 2026-11-23: reuse.
      `src/text/layout_tables.rs` runs the mark work and the three kerning works of `fontbe` 1.0.0 on the IR of one location with no axes.
      On Virtua Grotesk, glyphs, advances and offsets equal the fontc compile with kerning on, for Arabic, Hebrew, Latin and a mixed line.
      With the first pass and the result of the other writer cached, one anchor edit costs 5 ms and one kerning edit 4 ms on Virtua Grotesk, and an anchor edit costs 32 ms on Amiri.
      With about 150 glyphs, an anchor edit costs 1 to 2 ms on both fonts.
      The cost on Amiri is in the mark writer (16 ms), so item 5.2 is necessary.
- [x] 0.2 Prove marker insertion: a `FeatureProvider` that adds one generated lookup at `# Automatic Code` in `mark`, compiled by `fea-rs` 1.0.0, shaped by `harfrust`.
      Verify with Virtua Grotesk that marks get the same offsets as with fontc.
      Done in `src/text/generated.rs`: glyphs, advances and mark offsets equal the fontc compile for Arabic, Hebrew and Latin samples.
- [x] 0.3 Review a `harfrust` upgrade to the current release: API changes, message callback, font functions.
      Result: upgrade to 0.13.3 before phase 3; it needs one changed line and adds no `read-fonts` copy.
      Move to 0.15 or later, which has the message callback for the debugger, together with a later move to fontc 2.
      `parley` keeps its own `harfrust` 0.5.2 until it is updated.
- [ ] 0.4 Put the timing probe into `benches/` so every later phase can be measured.

### Phase 1: layout tables from live values

- [ ] 1.1 New engine module for layout tables under `src/text/`, with a header that states the rules of section 5.2.
      Input: feature code, glyph order, and for each glyph the advance, code points, anchors, category and components at one location, plus kerning and groups.
      Output: a shaping font or an error with line and column.
- [ ] 1.2 Marker insertion for `curs`, `kern`, `mark`, `mkmk`, and the default order when a feature has no block.
      A feature block without a marker gets no generated lookups, as in fontc.
- [ ] 1.3 Mark-to-base and mark-to-mark with mark filtering sets, as fontc writes them.
- [ ] 1.4 Mark-to-ligature from numbered anchors.
- [ ] 1.5 Cursive attachment from entry and exit anchors with `RightToLeft` and `IgnoreMarks`.
- [ ] 1.6 Kerning as GPOS: groups, split by script and direction, right-to-left value records, pairs with marks, `dist` where fontc uses it.
- [ ] 1.7 GDEF glyph classes from categories, anchors and the feature file.
- [ ] 1.8 Anchor propagation through components, as fontc does it.
- [ ] 1.9 Parity tests: for the core fixture, Virtua Grotesk, and a new Arabic fixture, the shaped output equals that of the fontc compile for a set of strings.

### Phase 2: values at a location

- [x] 2.1 Advances, anchors and kerning at a master come directly from that master.
      Done 2026-11-23: `LayoutInputs::at_location` takes the values of a layer at the location itself, and a test checks that the result equals `LayoutInputs::from_project` at each master.
- [x] 2.2 Between masters, each glyph is interpolated alone.
      A glyph or anchor that cannot be interpolated uses the nearest master and gets a diagnostic.
      Done 2026-11-23: advances and each anchor by name use the weights of the variation model of the outlines, over the layers of that glyph, sparse layers included.
      Kerning pairs are resolved at each master with the UFO lookup that fontc uses, then interpolated.
      Contour compatibility is not read.
- [x] 2.3 Text outlines between masters come from per-glyph interpolation, with the same fallback.
      Done 2026-11-23: the drawn outlines of the live masters are blended point by point for each glyph that the text shows.
      Sparse layers are not used for outlines yet.
- [x] 2.4 Test: Amiri-style fixture with an anchor missing in one master; layout works, one diagnostic names the glyph and anchor.
      Done 2026-11-23 on `tests/fixtures/arabic`: at the midpoint the only diagnostic is the `top` anchor of `dal-ar`.
      At three locations between the masters, the test strings give the same glyphs as the fontc variable compile of the finished glyphs, with advances and offsets within one unit.

### Phase 3: one path in the editor

- [x] 3.1 The Text tool, the proof strip, the specimen and the Proof panel section use the new engine.
      Partly done 2026-11-23: at a master, `FontModel::live_layout` builds the tables on the calling thread and the text inputs use them.
      Done 2026-11-23: `FontModel::live_layout` is keyed on the layout revision, the active source and the location, and `TextInputs` uses it at every location.
- [ ] 3.2 Remove from the editing path: the compiled preview font, `layout_font` and the layout compile cache of `6ca726a`, the `text_location` cache, the joining-rules fallback, and the post-shaping kerning lookup.
      Partly done 2026-11-23: the text inputs no longer read the compiled preview font, and `layout_font`, the layout compile and its cache, `text_location` and the layout status of the status bar and the preview pump are removed.
      The joining-rules fallback and the post-shaping kerning lookup remain.
- [ ] 3.3 Rebuild triggers: feature code, glyph set, classes, and layout values of glyphs that the text can produce.
      A point edit rebuilds nothing.
- [x] 3.4 A drag step and a commit call the same layout function.
      Done 2026-11-23 at a master: `LayoutTables::build_live` takes the committed inputs and `LayoutOverrides`, the draft values of the gesture.
      Point, anchor, sidebearing and component drags keep their edit in a session draft until release; the edit canvas reads the draft advance and anchors, and a Text tool kerning drag gives its pair value, on each step.
      Between masters the kerning drag still moves the pair after shaping, until phase 2.
- [x] 3.5 View anchoring on the edited glyph for left-to-right and right-to-left lines.
      Done 2026-11-23: the edit canvas moves its viewport by the screen distance that new layout moved the active sort, after a drag step and after the app supplies new text inputs.
- [x] 3.6 Feature code errors: keep the last code that compiled, with live values, and show the error with its line.
      Done 2026-11-23: `build_live` builds from the last feature text that compiled, or from no feature text before one has, and the status bar shows `features.fea line N: message`.
- [x] 3.7 Tests: a sequence of drag steps gives the same layout as a fresh computation from the final state; an idle editor requests no frame; no layout change after a point edit; a failed feature compile changes no position.
      Done 2026-11-23 on the Arabic fixture: `src/text/layout_tables.rs`, `src/application/font_model.rs`, `src/application/view/canvas/editor/tests/text_line.rs` and `src/application/view/render.rs`.
      A contour drag and its commit run no step of a table build, counted by `LayoutTables::build_counts`.
- [ ] 3.8 Browser build: the same path, no worker thread needed.

### Phase 4: checker and preview compile

- [ ] 4.1 Apply `DropIncompatiblePaths` to the background preview compile, and list the glyphs it changed.
- [ ] 4.2 After the background compile, shape the visible text with both fonts and compare glyphs, advances and offsets.
- [ ] 4.3 Show differences in the status bar and in the proof view; never apply them to the edit canvas.
- [ ] 4.4 Decide if export uses the generated lookups of the editor or those of fontc, from the result of 0.1.

### Phase 5: speed

- [ ] 5.1 Cache the parsed feature code; rebuild only generated lookups after a value edit.
- [ ] 5.2 Limit generated lookups to the glyphs that the text can produce through GSUB.
- [ ] 5.3 Budgets, measured in `benches/`: 4 ms for one edit of one line at 6,000 glyphs; 1 ms at 1,000 glyphs.
- [ ] 5.4 Frame-budget test for an anchor drag in an Arabic line.

### Phase 6: Arabic design features

- [ ] 6.1 Right-to-left kerning: storage, editing in the Text tool, and compile rules equal to fontc.
- [ ] 6.2 Generator for joining features from glyph names, as a reviewed proposal.
- [ ] 6.3 Letters from a skeleton and dot marks through `ccmp`.
- [ ] 6.4 Cursive attachment editing: entry and exit anchors, whole-word preview.
- [ ] 6.5 Ligature component anchors: display and editing.
- [ ] 6.6 Collision overlay for marks and dots in the text line.
- [ ] 6.7 Test strings for a glyph in all joining contexts and with all mark classes.
- [ ] 6.8 Quranic sample set in the proof samples.
- [ ] 6.9 Kashida and tatweel preview.
- [ ] 6.10 Shaping debugger, after the `harfrust` decision of 0.3.

### Phase 7: fixtures, documents, upstream

- [x] 7.1 A repository-owned Arabic fixture with joining, marks, a ligature with component anchors, cursive attachment, right-to-left kerning, and one deliberately incompatible glyph.
- [ ] 7.2 Optional local tests against Amiri and Raqq when their sources are present.
- [ ] 7.3 Update `docs/ARCHITECTURE.md`, the changelog, the website architecture page and the known-limitations page.
- [ ] 7.4 Record upstream needs in `docs/upstream/`: a `harfrust` hook that can change positions, public kerning and mark writers in `fontbe`, incremental compile in fontc.

## 8. Findings during the work

- 2026-11-23: fontc does not take anchors from components for UFO sources.
  The current live path of the editor does.
  In Virtua Grotesk, `beh-ar.init` and `noon-ar.fina` are composites with no anchors of their own, so the exported font does not attach a kasra to them, while the editor showed it attached.
  Decided 2026-11-23, to keep the behavior designers had: the compiler snapshot gives a composite without anchors the anchors of its components, as Glyphs does, and the live tables use the same rule.
  The export and the editor now both attach the kasra.

## 9. Decisions for the owner

1. Export uses the generated lookups of the editor, or fontc keeps its own and the checker compares (item 4.4).
2. Upgrade `harfrust` now or after phase 3 (item 0.3).
3. The order of phase 6 items.
4. Anchors from components: decided, in both (section 8).

## 10. Sources

- Fontra: [repository](https://github.com/fontra/fontra), [shaper font builder](https://github.com/fontra/build-shaper-font), [design issue 2381](https://github.com/fontra/fontra/issues/2381), [blog](https://blog.fontra.xyz/blog/opentype-harfbuzz/).
- Counterpunch: [repository](https://github.com/counterpunchspace/editor), [site](https://counterpunch.space/), [fontc issue 1897](https://github.com/googlefonts/fontc/issues/1897).
- Glyphs: [previewing and testing](https://handbook.glyphsapp.com/previewing-and-testing/), [contextual kerning](https://glyphsapp.com/learn/contextual-kerning).
- fontc: [kerning writer](https://github.com/googlefonts/fontc/blob/main/fontbe/src/features/kern.rs), [mark writer](https://github.com/googlefonts/fontc/blob/main/fontbe/src/features/marks.rs), [feature provider](https://github.com/googlefonts/fontc/blob/main/fea-rs/src/compile/feature_writer.rs), [removal of incremental compile](https://github.com/googlefonts/fontc/pull/1183).
- ufo2ft: [kerning feature writer](https://github.com/googlefonts/ufo2ft/blob/main/Lib/ufo2ft/featureWriters/kernFeatureWriter.py).
- harfrust: [repository](https://github.com/harfbuzz/harfrust).
- Arabic engineering: [Nastaliq kerning](https://simoncozens.github.io/nastaliq-autokerning), [Nastaliq dot positioning](https://simoncozens.github.io/nastaliq-dot-positioning), [Gulzar](https://github.com/googlefonts/Gulzar), [Sakkal, ATypI 2021](https://atypi.org/presentation/deficiencies-of-handling-arabic-script-in-opentype-and-some-possible-solutions/).
