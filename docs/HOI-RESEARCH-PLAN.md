# Higher-order interpolation: research and planning

Last updated: 2026-10-29.
Status: Phases 1–3 completion gates met with HOI Shapes; the previously recorded general browser splitter-cursor failure remains open.
This implementation made no writes to Virtua Grotesk.
Current stage: Phase 3 direct trajectory editing is implemented and ready for hands-on testing with HOI Shapes.

This is the working record for adding higher-order interpolation to Runebender.
Keep this document updated as each question is discussed, recording accepted decisions, their reasons, remaining questions, and any new evidence.
The five questions were discussed in order and their decisions are recorded below.
Do not treat a recommendation in this document as an accepted design or permission to implement it.

## Confirmed constraints

- Phase 1, Phase 2, and Phase 3 application coding and tests using HOI Shapes are authorized; Virtua Grotesk and other production fonts must remain unchanged.
- Virtua Grotesk's Regular `a` is complete and approved by Eli and must remain unchanged.
- Virtua's Bold and alternate drawings are unfinished; do not treat them as approved target artwork or infer design intent from their current state.
- Preserve unrelated working-tree changes in both repositories.
- Numerical compatibility and compiler checks do not substitute for Eli's visual approval of a transition.
- Keep this research and the evolving plan under `runebender/docs/`.
- Prefer industry-compatible representations and conversion paths, but do not require perfect cross-editor HOI interchange in the first version.
- Preserve imported information where feasible and report unsupported behavior; known compatibility edge cases should inform staged work rather than block all initial progress.
- Preserve the accepted decisions from the five separate discussions and keep implementation progress recorded here.

The protected source is `../virtua-grotesk/sources/VirtuaGrotesk-Regular.ufo/glyphs/a.glif`, relative to the Runebender repository root.
Its SHA-256 on 2026-10-29 was `d4fab5ff2eaff9ac300e1c7cb46d8094048876cb79b11d25b7054413809e9c6e`.
This records the approved source for later comparison; it does not authorize a source rewrite, even one intended to preserve the visible outline.
Future scratch targets, trajectory metadata, or derived compatibility representations need a plan that respects this constraint.
The Phase 1 storage choice is documented below under the accepted question 4 compatibility constraints.

## Phase 1 implementation and validation

Eli authorized Phase 1 on 2026-10-29, using HOI Shapes and leaving Virtua Grotesk unchanged.
Implementation started from checkout `7112a5a`, with the research document and synthetic fixture already present as uncommitted work.
The pinned `1.96` toolchain resolves to Rust 1.96.1 on this host.
No dependency changes are planned.

Engineering decision: use the experimental version-1 Designspace-lib key `com.runebender.hoiTrajectories` for scoped authoring records.
This is a documented Runebender extension, not a claim of a shared industry standard.
Its codec retains unknown or malformed payloads and refuses to overwrite them through the editing API.
Records capture the axis coordinates and map, the two source filenames and layers, design coordinates, existing point identifiers, and an ordered outline fingerprint.
Version 1 conservatively suspends a record after an endpoint outline, topology, source-location, or axis change; it never silently reassigns an old point index.
Missing source identifiers use the same fingerprint guard without inserting metadata into source GLIFs.
Real glyph-local sparse sources participate in neighboring-interval validation.

The original `com.runebender.hoiIntermediate` payload and its evaluator remain unchanged.
New scoped records are storage-only in Phase 1, with explicit preview/compilation errors until Phase 2 supplies the shared evaluator.
No interpolation controls are being exposed in the UI in this phase.

Preservation work retains original GLIF bytes only when the whole decoded glyph still matches, in both filesystem and browser package saves.
Changed glyphs continue through the normal serialization path.
Initial Runebender tests found that FontTools had emitted unsupported legacy `source/lib`, `source/features`, and `source/info` copy directives in HOI Shapes.
Those redundant default-master flags were removed from the synthetic Designspace without changing any source drawings or axis locations.
### Implemented contract

The [scoped metadata model](../src/font/model/hoi_trajectories.rs) has one extension key, reader, and writer.
The canonical Designspace owns its source payload, and [Project HOI operations](../src/font/hoi.rs) validate new or edited records before installation.
Installing or removing supported records changes the document revision and Designspace dirty state without modifying endpoint layers.
Unchanged suspended records can be retained while another record is removed.
Malformed, unknown-version, and unknown-field payloads remain opaque and cannot be overwritten by the new editing API.

| Field | Version 1 meaning |
| --- | --- |
| `version` | Integer `1`, in a dictionary with a `trajectories` array |
| `glyph` | Glyph name |
| `axis` | Tag, name, user-space bounds/default, and ordered user-to-design mapping |
| `lower`, `upper` | Source filename as recorded in the Designspace, layer name, mapped coordinate, guarded contour/point indices, existing optional identifiers, and `outline_sha256` |
| `controls` | Two absolute cubic control positions in font units; the endpoint drawings supply the curve endpoints |
| `easing` | `linear`, `ease-in`, `ease-out`, or `ease-in-out` |

The fingerprint is SHA-256 over a deterministic JSON encoding of ordered contour identifiers, closed/hyper flags, and each point's identifier, segment role, smoothness, and coordinates.
It excludes session-local numeric identities and does not require adding identifiers to UFOs.
Contour or point reordering conservatively suspends a record; source display-order changes can rebind successfully because records identify source filenames and layers.
Moving or removing a source, changing its endpoint drawing, or changing the axis coordinates/mapping also suspends the record until it is deliberately recreated or its original context is restored.
Renaming source files during a later workflow may require reauthoring; no general rename-migration or foreign-format interchange is claimed here.

The timing contract for future evaluation is `s = t` for linear, `s = t²` for ease-in, `s = 1 - (1 - t)²` for ease-out, and `s = t²(3 - 2t)` for ease-in-out.
Here `t` is progress within the selected interval in mapped design coordinates.
Phase 1 stores these choices; Phase 2 must implement the composed evaluation and connected-handle displacement before enabling preview or compilation of scoped records.

New-authoring validation currently accepts closed cubic/line contours in single-axis Designspaces with compatible endpoint order and explicit on-curve nodes.
It rejects independent off-curve records, quadratic/shared-control paths, hyperbeziers, open contours, duplicate source locations, non-neighboring intervals, invalid controls, and multi-axis authoring.
Existing legacy midpoint records require an explicit future conversion before new scoped authoring on their endpoint layers; they are not normalized automatically.
Normal interpolation remains available for fonts without scoped records and for unaffected glyphs.

### Validation results

| Check | Result |
| --- | --- |
| `cargo test --lib --locked font::hoi::tests -- --test-threads=1` | 14 passed: filesystem/browser-codec round trips, exact GLIF retention, changed-glyph persistence, guarded identities, stale source/topology/axis detection, sparse boundaries, opaque data, and legacy behavior |
| `cargo test --lib --locked font::persistence:: -- --test-threads=1` | 19 passed |
| `cargo test --lib --locked font::model:: -- --test-threads=1` | 23 passed |
| `cargo test --test canonical_source_history --test canonical_history --locked -- --test-threads=1` | 17 passed |
| `cargo test --lib --locked font::project:: -- --test-threads=1` | 46 passed, including existing legacy HOI preview and Save As regressions |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo doc --lib --no-deps --locked` | Passed |
| `cargo fmt --all --check`, `git diff --check`, copyright headers | Passed; new untracked Rust headers checked separately |
| `./web/build.sh` | Passed using the pinned browser dependencies and wasm-bindgen 0.2.127 |
| `node web/hoi-preservation.cjs` | Passed in headless Chrome: HOI Shapes opened with six glyphs and all 18 original GLIFs were retained byte-for-byte |
| Synthetic fontmake validation and variable compilation | Passed again after removing the redundant source-copy flags |
| `RUNEBENDER_DPRS=1 node web/quality.cjs` | Failed twice at line 115: active splitter cursor was `default`, expected `ew-resize` |

These are 119 passing selected Rust tests, not a claim that the complete workspace runtime suite was run.
The broader browser check reached outline editing, compilation, undo/redo, and zoom before failing at the splitter assertion.
No application UI or browser-host source code was changed by this phase, but a separate baseline build was not used to establish the cause of that failure.
The failing check remains intact, and the canonical known-limitations guide records the unresolved browser verification limit.
The website documentation build passed; nothing was deployed.
Cargo also reports a future-compatibility warning for the existing `block` 0.1.6 dependency; dependency pins were not changed.

All mutating font tests use disposable synthetic sources or temporary fixture copies.
The baseline HOI Shapes GLIFs were not edited.
No Virtua source was written by this task, and the approved Regular `a` still matches its recorded SHA-256.
A source inventory comparison detected concurrent changes to Regular and Bold `seen-ar.glif`; those unrelated changes were left intact rather than reverted.

Phase 1's scoped storage/preservation completion gate is met.
Interactive undo for trajectory edits, actual curved evaluation, connected-handle motion, compiled HOI output, target fitting, and the editing mode remain later-phase work.
The broader browser interaction failure remains open and prevents claiming a complete browser quality pass.

## Phase 2 implementation and validation

Eli authorized the evaluator and export phase on 2026-10-29, using HOI Shapes exclusively for writable tests.
The shared evaluator now composes a cubic trajectory with the four specified easing presets in mapped design-coordinate intervals.
Connected incoming and outgoing cubic controls receive their own on-curve node's displacement; source endpoints and other intervals remain exact.
Canonical authoring transactions support guarded undo/redo, revision invalidation, and deterministic save/reopen evaluation.

The pinned Babelfont/fontc path accepts disposable glyph-local intermediate layers, so no full-source fallback is needed for the tested cases.
`Project::babelfont_snapshot()` now includes the bounded bake, which feeds existing variable compilation and export callers.
`Project::static_babelfont_snapshot()` and `compile_static()` evaluate directly, without sampling, before ordinary OpenType conversion.
These are engine APIs; static export controls and trajectory editing UI are not part of this phase.

The initial sampler uses power-of-two subdivisions and a conservative second-derivative bound for each affected glyph interval.
If the bound is `M` and the segment count is `n`, the positional error of source interpolation is at most `M / (8 n²)` before compiler conversion.
The default source tolerance is 0.25 font units, with a hard ceiling of 4096 generated glyph layers per snapshot and an explicit error if the requested tolerance exceeds the budget.
This bound is not a final-font guarantee: cubic-to-quadratic conversion, integer outline rounding, and axis quantization remain separate sources of error.

The first reopened end-to-end fixture produced 125 glyph-local samples with a 0.151724-unit source bound.
Bidirectional outline sampling at independent interior locations and endpoints measured a maximum 1.177430-unit compiled error across the static and variable proofs, within the declared 2-font-unit test tolerance.
The comparison checks all glyphs, advances, and contour counts, including the ring's counter.
The generated-source comparison independently linearly interpolates baked nodes at unsampled positions and checks the analytical bound, including linked handles.
Existing authored sparse boundaries and both unequal neighboring intervals compile correctly without changing their shared source drawing.
Extreme finite controls that overflow the curvature calculation fail explicitly rather than disappearing into a linear export.

### Phase 2 validation results

- 97 selected Rust tests pass: 27 scoped HOI tests, 3 interpolation tests, 2 history tests, 19 persistence tests, and 46 Project tests.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo doc --lib --no-deps --locked`, `cargo fmt --all --check`, copyright headers, and `git diff --check` pass.
- `./web/build.sh` succeeds using the pinned browser workspace and wasm-bindgen 0.2.127.
- The actual headless Chromium host opens both baseline and authored HOI Shapes packages and retains all 18 original GLIF byte sequences on save.
  Scoped records remain in the existing source package through the browser's incremental save contract; an unchanged Designspace is omitted from the write set.
  Metadata edits and their serialization/reopen are covered by the native memory-package and filesystem tests.
- The broader `web/quality.cjs` check at 1× again reaches the existing `web/quality.cjs:115` failure: the active splitter reports `default` instead of `ew-resize`.
  It reaches outline editing, compiled export, undo/redo, and zoom first, but this is not a complete browser interaction pass.
- A rendered comparison of actual variable-font outlines and the exact evaluator was inspected headlessly.
  Source endpoints agree, the authored movement is visible at interior weights, and the ring retains both contours.
  This is technical proof of the synthetic fixture, not Eli's approval of a production transition.
- All 3,135 Virtua source files and all 36 baseline HOI Shapes files match the SHA-256 manifests captured at the start of Phase 2.
  The protected Regular `a` remains `d4fab5ff2eaff9ac300e1c7cb46d8094048876cb79b11d25b7054413809e9c6e`.
- No dependency pins changed, no foreground application was opened, and no commit, push, or deployment was made.

The existing `block 0.1.6` future-compatibility warning is unchanged.
The broader compiler and editor suites were not represented as a full release pass.

### Reproduce the Phase 2 proof

Run from the `runebender` checkout:

```sh
RUNEBENDER_HOI_PROOF_DIR="$PWD/target/hoi-phase2-proof" \
  cargo test --lib font::hoi::evaluation_tests::glyph_local_bake --locked -- --test-threads=1
```

This generates a disposable authored source package in `target/hoi-phase2-proof/source/`, `HOIShapes-VF.ttf`, static instances at weights 289 and 585, `comparison.svg`, and `measurements.json`.
The source package contains the saved trajectory records and unchanged copied GLIFs, not the compiler's sampled layers.
The SVG overlays ordinary interpolation in gray, exact evaluated outlines in blue, and the compiled variable outlines in orange at weights 100, 289, 400, 585, and 900.
A locally rendered `comparison.png` accompanies this run; the SVG is the reproducible vector artifact.
The authored package can now be opened in the Phase 3 tool for direct trajectory editing.

After building and serving `web/`, run `node web/hoi-preservation.cjs` with the usual Playwright/Chrome variables.
Set `RUNEBENDER_HOI_SOURCE="$PWD/target/hoi-phase2-proof/source"` to repeat the same check on the authored package.

Static snapshots currently retain default-source naming, interpolate explicit full-source metrics and kerning, and reject Designspace rules, cross-axis mappings, differing custom OpenType master values, and mismatched explicit metric sets.
A glyph containing both legacy midpoint HOI and new scoped trajectories is rejected, with its original data retained.
Existing legacy-only behavior is unchanged; no implicit conversion is claimed.
Adaptive sampling, export settings UI, general final-font error certification, and portable baked Designspace/UFO packaging remain Phase 5 work.

## Phase 3 implementation and validation

Eli authorized direct interpolation editing on 2026-10-29 so HOI Shapes can be tested interactively.
The editor now has an **Edit Interpolation** tool (`I`) and a panel built from the shared row vocabulary.
The first implementation supports a single continuous axis and one neighboring interval at a time, including an existing glyph-local source boundary.
The panel shows the axis and endpoint source names; unsupported inputs display the engine's diagnostic.

The canvas shows both locked endpoint outlines, the exact intermediate outline, and a connection for each on-curve point.
Click a connection to select it; drag it to bend its cubic trajectory.
Selected paths expose square trajectory controls that can be dragged independently.
Shift-click adds or removes paths from the selection; selected paths move together.
Arrow keys nudge the selected controls or paths, with Shift for a larger step.
The side panel provides Linear, Ease In, Ease Out, and Ease In/Out, Reset Selected, and a progress slider.
Compare Export requests the existing Phase 2 compiler preview and shows pending, ready, or failure explicitly.
While dragging, export comparison waits for release rather than displaying a previous compiled outline as current.

Pointer drafts use the same engine evaluator without changing the document revision, dirty state, source outlines, or history.
Release commits one canonical metadata transaction; Escape discards the draft.
Returning a drag to its initial geometry makes no history entry.
Undo/redo joins the existing glyph/metadata ordering, and save/reopen uses the Phase 1 Designspace-lib representation.
Outline-changing menu commands are disabled in this tool; finishing the mode restores ordinary source editing.

Native validation passes: 118 selected tests, comprising 28 HOI engine tests, 56 canvas tests, four interpolation-tool tests, eight affected inspector/history tests, six design-system/panel tests, seven action tests, and nine command tests.
One pre-existing canvas benchmark remains ignored and is not runtime coverage.
Strict all-target Clippy, native build, documentation build, formatting, copyright, and whitespace checks pass.
A Gray headless capture was inspected; endpoint labels use the UI font's supported characters, and both small proof previews now follow the exact HOI outline.
The first browser workflow exposed a missing keyboard-focus fallback after panel clicks.
The browser host now refreshes the actual menu widget as the fallback after rebuild, matching the native Xilem driver's behavior through the browser's extra widget wrappers.
The final browser build and strict browser Clippy pass.
`web/hoi-editing.cjs` passes with real pointer and keyboard input: direct bending, independent cubic-control editing, Escape, Shift multi-selection, group nudging, easing, scrubbing, compiled comparison, reset, neighboring-interval scope, Undo/Redo after panel actions, and save/reopen.
The saved package retains all 18 GLIFs byte-for-byte.
`web/hoi-preservation.cjs` also passes on the baseline fixture.
The final Gray browser screenshot was inspected at `target/hoi-phase3-proof/editor.png`; it shows selected trajectory controls, the exact outline, the current compiled overlay, and matching proof previews.
A native headless capture is retained at `target/hoi-phase3-proof/native-editor.png`.
These are automated technical checks; native foreground pointer/IME/accessibility behavior and Eli's design judgment remain separate from this evidence.

The general `web/quality.cjs` run at DPR 1 still stops at its pre-existing line-115 splitter-cursor assertion: `default` instead of `ew-resize` during a resize drag.
This was reproduced before Phase 3 and was not suppressed or reported as passing.
No full release suite or deployment is claimed.
All 3,135 Virtua source files and all 36 baseline HOI Shapes files still match the Phase 3 starting SHA-256 manifests.
The approved Regular `a` is unchanged, and no dependencies, installed application, or production sources were modified.
The Phase 3 completion gate is met for the supported single-axis cubic/line workflow.
No Phase 4 join smoothing or intermediate-target fitting is included in this phase.

### Preview UI follow-up: real letters and a steady canvas

Eli's first hands-on test exposed a misleading fixture and a cluttered editing view.
The box centers are actually at x = 220, 460, and 760 in the saved sources; the UI does not place the masters side by side artificially.
That test is useful for trajectory math, but is a poor introduction to how a letter normally changes weight.

The [Glyphs HOI handbook](https://handbook.glyphsapp.com/higher-order-interpolation/) shows current and neighboring-master node positions in the same glyph coordinate space, with dotted connections and draggable intermediate points or handles.
It omits more distant masters to reduce clutter.
Glyphs also separates editing from its [Preview Area and Preview Panel](https://handbook.glyphsapp.com/previewing-and-testing/#previewing-interpolated-instances), which support instance selection and centering the active glyph.
The documented [FontLab Sigma tool](https://help.fontlab.com/fontlab/9/articles/the-sigma-tool/index.html) similarly edits point trajectories for a chosen axis/master pair.
FontLab's [variation controls](https://help.fontlab.com/fontlab/8/whats-new/whats-new-07-families-variation/) and [Preview panel](https://help.fontlab.com/fontlab/8/whats-new/whats-new-08-test-adjust/) provide instance scrubbing and clean text previews.
These are documentation findings, not hands-on validation of those applications.
[Fontra's roadmap](https://github.com/fontra/fontra#roadmap) still lists built-in nonlinear interpolation without a completion check, so it is not evidence for a shipped native HOI trajectory UI.

Runebender now separates **Edit Paths** and **Preview** in the interpolation panel.
Edit Paths remains the default and shows the locked endpoint drawings and editable trajectories in their real coordinates.
Preview fills the current interpolated outline and hides all endpoint outlines, point markers, trajectories, and controls; an explicitly enabled compiled comparison remains available.
Both views share the same canvas transform, and changing weight does not pan, rescale, or recenter the outline.
The proof panes use a reference frame derived from the interval endpoints and trajectories, independent of the current progress.
This preserves real spacing and positional changes; it does not conceal the box's deliberately translated source drawings.
Preview rejects trajectory edits from pointer input and editing commands; switching modes preserves selection and discards any unfinished pointer draft without committing it.
The Preview controls stay above the editing controls so their position does not jump when changing mode.

[HOI Shapes Letters](../assets/font-sources/variable-fonts/HOIShapesLetters/HOIShapesLetters.designspace) adds Virtua's real `a` to a separate demo beside the other font sources.
The live playground had unsaved edits and the current live protocol cannot create glyphs, so it was left untouched.
The copied Regular and Bold GLIFs are byte-identical to Virtua's originals, including advance widths and anchors; Bold remains an unfinished source drawing.
Light repeats Regular, and the demo's Regular/Bold interval is 400–900 rather than Virtua's 400–700.
The new demo uses 1024 units per em to preserve the original coordinates, with matching two-contour, 51-point topology and no bracket substitution or pre-authored HOI correction.
The accompanying OFL preserves the copied outlines' attribution and license.

Validation passes for this follow-up: six HOI UI tests, four proof-layout tests, and six design-system/panel gates.
The HOI UI test now uses the copied `a` to check a constant reference frame across weights, endpoint preservation, draft cancellation, selection retention, and no revision/history changes from preview actions.
The native pointer harness rejects clicks, drags, arrow keys, and Delete in Preview.
Strict native all-target Clippy, the documented browser Clippy command, native and browser builds, formatting, copyright, and whitespace checks pass.
A broader browser Clippy attempt without the documented `--no-deps` flag also linted the generated upstream Xilem adapter and failed on its existing warnings; that adapter was not modified.
The actual browser interaction test passes through editing, filled preview, scrubbing, blocked edits in Preview, selection restoration, Undo/Redo, compiled comparison, and save/reopen with all 18 baseline GLIFs unchanged.
The separate browser source-preservation check also passes.
The new letter demo opens in both hosts, exposes 27 on-curve trajectories for `a`, and compiles through Runebender's own compiler.
Default Gray native and browser captures were inspected, including the filled `a` at two weights with a steady view; evidence is retained in `target/hoi-phase3-preview/`.
All 3,207 protected files across Virtua sources, the original HOI Shapes baseline, and the existing playground match their starting hashes.
The three new `a` copies match their respective Virtua GLIFs byte-for-byte.
The existing native playground session remains open and unsaved; no live mutation, save, restart, installation, commit, or deployment was performed.
The earlier general browser splitter-cursor failure remains outside this follow-up's checks and is not claimed fixed.

### Try Phase 3 locally

For a real glyph, open [HOI Shapes Letters](../assets/font-sources/variable-fonts/HOIShapesLetters/HOIShapesLetters.designspace), open `a`, and press `I`.
Use **Regular to Bold**, bend a connection in **Edit Paths**, then choose **Preview** and move the Progress slider.
The running application needs the updated build to display the new Preview choice.


Open [HOI Shapes Playground](../assets/font-sources/variable-fonts/HOIShapesPlayground/HOIShapes.designspace) in Runebender, open `box`, and press `I`.
Drag a connection to bend its path; use the easing buttons and Progress slider to try the transition.
Save your experiments in this font.

The playground is kept in `assets/font-sources/variable-fonts/HOIShapesPlayground/`, beside the other fonts.
It starts with the same three masters and ordinary interpolation as HOI Shapes.
The separate HOIShapes baseline remains available for automated tests.
This replaces the earlier instructions to use a temporary copy under `target/` or launch with environment variables.

For automated browser checks, use `web/hoi-editing.cjs` with the Playwright variables in `web/README.md`.
Set `RUNEBENDER_HOI_PROOF_DIR` to retain its screenshot.


## Research conclusion

Curved point trajectories are technically feasible and have established font-editor precedents.
They can preserve endpoint drawings while changing the intermediate shapes.
HOI controls how corresponding points travel; it does not determine point correspondence, repair incompatible outlines, or automatically produce attractive intermediate letters.

For Virtua, the useful question is how to design a continuous transition from the fixed Regular toward an eventual approved Bold.
Replacing the existing shape substitution is a possible future outcome, not an accepted change.

Separate three concerns throughout planning:

1. Correspondence: which point and contour in one drawing corresponds to which in another.
2. Authoring and evaluation: how those points move through the designspace.
3. Export: how the authored motion is represented in a compiled font and reproduced by consumers.

## Terminology and search vocabulary

| Term | Meaning and research use |
| --- | --- |
| Higher-order interpolation, high-order interpolation, HOI | Names associated with quadratic or cubic point motion, notably Underware's work |
| Nonlinear interpolation, non-linear interpolation, NLI | Common synonyms, but also used for axis remapping; qualify searches with point trajectories |
| Bézier interpolation, quadratic or cubic trajectories, curved point paths | Precise language for the desired geometric behavior |
| Control masters, off-curve masters | Control geometry defining trajectories; not necessarily desired intermediate glyph drawings |
| Linked axes, tensor-product interpolation, bilinear or trilinear interpolation | Mathematical and encoding mechanisms that can produce nonlinear motion |
| Gemowespace | Underware's historical term for a construction with duplicated control masters |
| Intermediate or corrective masters, brace layers, sparse sources | Drawings that constrain ordinary piecewise-linear interpolation |
| Axis mapping, easing, designspace warping, virtual axes, avar2 | Coordinate transformations and axis relationships; not automatically per-point trajectory editing |
| Anisotropic interpolation, split or ambivalent locations | Different interpolation factors for horizontal and vertical coordinates |
| Glyph morphing, point correspondence, outline compatibility | Related research into establishing valid relationships between drawings |
| Piecewise Bézier, Hermite interpolation, spline interpolation, tangent continuity | Mathematical options for constructing and joining trajectories; not established HOI interchange standards |
| Parametric, skeleton-based, stroke-based fonts, metapolation | Adjacent authoring models that may inform constraints and semantic editing |

Underware presented HOI in 2018.
See the [ATypI presentation](https://atypi.org/presentation/hi-hoi/) and its [original case study](https://www.underware.nl/case-studies/hoi/).
The case-study URL was identified through primary references, but direct retrieval was blocked during this research.
[Peter Constable's explanation](https://github.com/PeterConstable/OT_Drafts/blob/master/NLI/UnderstandingNLI.md) connects HOI to the variable-font table machinery.
[MutatorMath](https://github.com/LettError/MutatorMath) documents the distinct piecewise-linear and anisotropic approaches.

## Distinctions that affect the design

### A trajectory curve is separate from an outline curve

An outline curve describes the letter at one designspace location.
A trajectory describes how one of that outline's nodes or handles changes position between locations.
A cubic outline does not automatically have cubic interpolation across weight.

### Axis easing is different from bending a trajectory

Between two masters on one axis, applying a common scalar easing function changes progress along the same straight connection.
It does not move a point sideways off that connection.
Multidimensional mapping, including avar2, can support curved motion when combined with appropriately constructed glyph variation data.

### Control positions are different from target positions

A quadratic Bézier control generally does not lie on the resulting curve.
An authoring control that means "pass through this position at the midpoint" must be converted into the appropriate mathematical control.
Runebender's existing helper uses that midpoint interpretation.
The UI should eventually make the meaning clear without requiring designers to work with unintuitive control masters.

### Continuous motion is different from a good transition

Moving points continuously does not guarantee smooth outline joins, open counters, consistent stroke thickness, or desirable weight progression.
On-curve nodes and their off-curve outline handles need coordinated consideration.
Continuity of a trajectory across master intervals is also distinct from tangency of the glyph outline at one weight.
Neither should be inferred from the other.

## How other editors and projects approach it

| Editor or project | Verified research finding | Limits of the evidence |
| --- | --- | --- |
| Glyphs 4 | Native HOI tool with intermediate-point, two-handle, and rotation-center modes; works with an axis and adjacent master layers | Handbook covers static and variable export; a recent support discussion reports multi-axis export problems |
| FontLab 9 documentation | Sigma describes linear, quadratic, and cubic point trajectories, associated with an axis and master pair; neighboring-handle follow modes | Documentation inspected, not a released binary; public product homepage still advertised FontLab 8.4; Sigma's exact export encoding was not established |
| Nonlinear-A for Glyphs 3 | Third-party plugin generating UFO/designspace sources and building TTF with fontmake | Author labels it beta; separate from native Glyphs 4 HOI |
| FontForge-HOI | Third-party Python tooling for trajectory authoring and generated control masters | Historical export rewrites multiple axis tags; not stock FontForge functionality |
| RoboFont | Scriptable interpolation and external Bézier-interpolation examples | No native trajectory-editing tool was established by this research |
| Fontra | Interpolation and avar2 mapping, plus external quadratic Bézier scripts | Current roadmap still leaves built-in NLI unchecked |

The [Glyphs HOI handbook](https://handbook.glyphsapp.com/higher-order-interpolation/) is the strongest current direct-authoring reference.
Its [Glyphs 4 support discussion](https://forum.glyphsapp.com/t/glyphs-4-higher-order-interpolation-tool/36808) confirms the version context.
An [October 2026 export discussion](https://forum.glyphsapp.com/t/hoi-glyphs-4-multiple-axis-export-issue/37461) records problems with multiple axes and staff suggestions involving newer beta builds, CFF2, and changing control modes.
That report is a reason to investigate behavior, not proof of universal failure or a confirmed fix.

The [FontLab Sigma manual](https://help.fontlab.com/fontlab/9/articles/the-sigma-tool/index.html) provides a useful axis-and-master-span interaction model.
The [FontLab product site](https://www.fontlab.com/) should be checked again before making claims about current release availability.
Use the manual as design evidence without assuming a particular encoding or interoperability result.

Additional implementation references are [Nonlinear-A](https://pkolchanov.ru/nla/), [FontForge-HOI](https://github.com/ctrlcctrlv/FontForge-Higher-Order-Interpolation), [RoboFont interpolation](https://robofont.com/documentation/topics/interpolation/), and the [Fontra roadmap](https://github.com/fontra/fontra#roadmap).

### An alternative: draw the intended middle shape

[Jérémie Hornus's Bézier-interpolation scripts](https://github.com/JeremieHornus/bezier-interpolation) demonstrate deriving trajectory controls from start, end, and intermediate target drawings.
This gives designers a way to express a counter or join as a complete shape, then derive curved movement through it.
The Fontra example is documented as quadratic; RoboFont examples include cubic interpolation.
This is different from simply adding a source to a piecewise-linear variation model.

[ArrowType's HOI exploration](https://github.com/arrowtype/HOI-test) includes source fonts and educational examples involving rotation and opening or closing a letter.
It is useful prior art and possible future fixture material, not proof of Runebender support.

## Variable-font export research

Two approaches deserve evaluation before choosing an implementation.

### Polynomial motion through linked axes

Variation contributions multiply across axes.
Synchronizing axes can therefore produce quadratic and cubic terms rather than only a linear term.
Appropriate delta vectors and variation regions can describe polynomial trajectories.
See [Constable's explanation](https://github.com/PeterConstable/OT_Drafts/blob/master/NLI/UnderstandingNLI.md) for the connection to the actual gvar mechanism.

Historical HOI demos sometimes used repeated axis tags.
The modern avar2 route coordinates distinct internal axes through a user-facing control.
The [Google Fonts HOI guide](https://googlefonts.github.io/how2avar2/docs/higher-order-interpolation/) describes the construction and links a quadratic-rotation example and build resources.
The [avar2 technical explainer](https://github.com/harfbuzz/boring-expansion-spec/blob/main/avar2.md) describes multidimensional changes to axis coordinates.

Exact polynomial evaluation remains subject to coordinate quantization, rounding, outline conversion, and renderer behavior.
A polynomial approximation of a circular arc is not mathematically exact circular rotation.
Do not equate cubic CFF2 outline segments with cubic motion through weight; outline representation and variation behavior are separate.
The [CFF2 specification](https://learn.microsoft.com/en-us/typography/opentype/spec/cff2) is relevant to a later format-specific investigation.

### Sampled intermediate sources

An exporter could evaluate authored trajectories at selected locations and generate ordinary intermediate sources.
The compiled motion would approximate the trajectory with piecewise-linear segments.
A plan for this approach needs an error tolerance, sampling strategy, file-size assessment, and comparison against the editor's evaluated geometry.
Sampling is a possible compatibility route, not the only way to encode curved motion.

### Support evidence and remaining checks

[Chrome 148 release notes](https://developer.chrome.com/release-notes/148?hl=en) list avar2 rendering in the May 5, 2026 stable release.
The [Google Fonts support matrix](https://googlefonts.github.io/how2avar2/docs/support/) is dated January 1, 2026 and should not be treated as a current account of Chrome support.
Browser success does not establish correctness in every desktop application, PDF pipeline, subsetter, or partial-instancing tool.
Those paths remain untested here.

## Virtua Grotesk source findings

The active designspace has a Weight axis from 400 to 700 and a rule replacing `a` with `a.bold` from 500 through 700.
Within each currently saved master, `a` and `a.bold` have identical outline geometry, widths, and anchors.
Regular and Bold differ from one another, but the substitution currently has no geometric effect.
Eli confirmed that the alternate and Bold work is incomplete; this observation is not a claim that the intended design change is unnecessary.

The four glyphs have two closed contours with 39 and 12 points, and matching ordered point types.
Regular's width is 608 and Bold's is 632.
The top anchors are `(312, 576)` and `(328, 576)`; the bottom anchors are `(312, 0)` and `(328, 0)`.
No HOI metadata was found in the active UFOs during the initial inspection.

These structures provide a possible correspondence for later design work, not visual acceptance.
If a later Bold design needs different connectivity or segmentation, handle that explicitly without modifying the protected Regular source.
The original Virtua research made no font edits or candidates and ran no shaping or compiled-font checks.
The separately authorized synthetic fixture and its validation are recorded in the consolidated plan below.

## Existing Runebender foundation

The original research inspected the `runebender-xilem` checkout at `b338d2a6`.
When saving this document, the destination `runebender` checkout was at `7112a5a`; its HOI model, quadratic evaluator, project integration, and mutation API were rechecked.
The destination had an unrelated modification to `src/main.rs`, which this task must preserve.
Treat these observations as source inspection rather than runtime validation.

| Area | Existing behavior and evidence |
| --- | --- |
| Metadata | [Typed HOI model](../src/font/model/hoi.rs) stores per-point intermediate positions under `com.runebender.hoiIntermediate` |
| Evaluation | [hoi_quad_at](../src/formats/metadata/lib_keys.rs) evaluates a quadratic passing through the specified position at the midpoint |
| Integration | [Project interpolation](../src/font/project.rs) applies a per-node quadratic override after ordinary interpolation and provides trajectory sampling |
| Mutation | [Canonical draft API](../src/font/babelfont/edit_components.rs) provides `set_hoi_intermediates` |
| Compilation | [Compiler snapshot](../src/font/compiler/mod.rs) consumes canonical geometry and registered sources; automatic HOI baking was not established by the inspection |

The existing approach is restricted to the first axis, global endpoint sources, and one quadratic midpoint per marked point.
It addresses points by contour and point indices, which require careful treatment when topology or ordering changes.
It overrides ordinary interpolation for marked nodes, so precedence relative to intermediate sources needs explicit semantics.
It does not curve widths, anchors, or component transforms.
No application UI or automation callers exposing the HOI mutation operation were found.

Comments mention baked brace layers for compilers, but comments do not establish an implemented bake path.
Do not assume geometric preview, compiled text preview, and exported-font output currently agree for HOI.
Existing tests were inspected, not run.

The current storage on the axis-minimum glyph also conflicts with preserving the approved Regular file byte-for-byte if used directly on Virtua.
Resolve that under question 4 before applying any future experimental metadata.

The [architecture guide](https://runebender.org/docs/architecture.html) assigns reusable interpolation to the font engine, persistence to format and persistence adapters, and user intent to application commands.
Keep those boundaries in the eventual plan.

## Five planning questions

| Order | Question | Status |
| --- | --- | --- |
| 1 | Authoring: how should a designer create and refine a transition? | Combined workflow accepted; interaction requirements recorded |
| 2 | Scope: which points, axes, master spans, and intermediate sources does a trajectory affect? | Local scope accepted; detailed source and multi-axis policies remain open |
| 3 | Outline behavior: how do nodes and handles move together, and how do trajectories join? | Connected following and explicit smooth joins accepted; mathematical details remain open |
| 4 | Persistence: how are identities, source data, and round trips preserved? | Pragmatic compatibility direction accepted; storage and full round trips deferred |
| 5 | Export: how do editor evaluation and compiled output agree across consumers? | Accepted: sampled intermediate sources for variable fonts, direct static evaluation, and avar2 deferred |

### Question 1: authoring

The immediate decision is the main design workflow, not the complete UI, data model, or exporter.

| Option | Designer interaction | Benefit | Tradeoff |
| --- | --- | --- | --- |
| Direct trajectories | Select corresponding points or a group and bend their movement between fixed endpoints | Closest to Eli's original description; precise local correction | Coordinating many nodes and handles can become tedious |
| Draw intermediate targets | Draw a desired intermediate shape at a chosen location, then derive trajectories through it | Expresses the counter, join, and whole letter together | Does not by itself specify desirable behavior throughout the rest of the interval |
| Combined workflow | Make direct trajectory editing the primary interaction, with intermediate target drawings as another entry point | Supports both local motion control and whole-shape design intent | Requires clear rules when a target drawing and manually edited trajectories disagree |

Decision accepted by Eli: support both direct trajectory editing and designer-drawn intermediate targets, with direct trajectory editing as the default.
This establishes the product direction; implementation staging remains to be planned.

#### Requested interpolation editing mode

- Show both endpoint masters and connections between corresponding points.
- Make the initial straight interpolation connections easy to select directly.
- Allow a selected connection to become a curved trajectory and be edited with familiar font-curve interactions.
- Include ease-in and ease-out controls within the interpolation editing mode.
- Keep endpoint artwork fixed while editing its transition; the approved Regular `a` remains unchanged.

These are user-requested interaction requirements, not an implemented interface.
Exact gestures, selection behavior, trajectory control types, and easing presets remain to be specified.

Planning interpretation: expose geometric trajectory shape and progression along that trajectory as distinct concepts.
Easing describes how progress along the weight interval maps to progress along the trajectory, even when no animation is playing.
A separate easing function may change the polynomial degree or export requirements; Phase 1 must settle its representation within the accepted sampled-export approach.
This interpretation is a proposal, not a user-approved mathematical model.

The relationship between intermediate target drawings and manually edited trajectories remains open.
In particular, decide later whether target drawings remain persistent constraints or initialize trajectory controls once.
Do not assume lossless bidirectional editing or silently discard an authored target when changing easing.

Rationale: direct manipulation matches Eli's requested workflow, while intermediate drawings provide another way to express a whole counter, join, or letter shape.
Question 1's main workflow decision is complete; the detailed behaviors above remain tracked for the later specification.

### Question 2: scope

Decision accepted by Eli: edits belong to selected point trajectories in the current glyph, on one explicit axis, between neighboring masters.
The endpoint drawings remain fixed.
Unaffected points and intervals retain their existing interpolation, including any previously authored corrections.
Connected outline handles follow the selected node under the accepted question 3 rule.

For Virtua's current two-source setup, the interval would be Weight 400 to 700.
With Light, Regular, and Bold sources, editing Regular to Bold would leave the Light-to-Regular interval unchanged.
This avoids silently applying a local correction across the entire family.

Easing follows the accepted local scope: selected trajectories in that interval, rather than remapping the weight axis for every glyph.
A group selection could share an easing adjustment.
Whether a whole-glyph or whole-interval convenience command is needed remains open.

Intermediate source drawings require an explicit policy.
The recommendation is to preserve real authored intermediate sources as boundaries between intervals rather than override them with an endpoint-only curve.
Derived target drawings from the combined authoring workflow must be distinguished from those real source masters.
Their relationship is not yet decided.

For multiple axes, define how a trajectory applies when other axis coordinates change before promising full support.
An axis and source pair alone may not fully describe a multidimensional influence region.
No multi-axis composition model or first-release limitation is accepted yet.

Decision: local scope accepted.
Rationale: predictable, targeted editing without changing other glyphs, axes, or master intervals.
The policies for intermediate targets and multidimensional influence remain open.
Question 3 now establishes that connected off-curve points participate in their on-curve node's local editing group.

### Question 3: outline behavior

Decision accepted by Eli: off-curve outline points always follow the on-curve points to which they are connected by handles when editing interpolation trajectories or easing.
There is no independent off-curve trajectory override in the planned workflow.
This replaces the earlier recommendation of following by default with an optional override.
Eli's preference is a consistent connection rule without an additional mode whose purpose is unclear.

An outline handle controls the glyph's shape; a trajectory control controls the on-curve node's movement through weight.
Keep these roles visually distinguishable in the eventual interface.
Show the on-curve node and its connected off-curves as a local editing group so that the affected points are visible.
Changes should be inspectable and undoable together, and must not affect unrelated points, glyphs, axes, or intervals.

Planning interpretation: apply an on-curve node's added displacement to its connected off-curves at each position in the interval.
This preserves their existing relative vectors while adding the trajectory correction.
Those vectors may still vary across weight according to the source drawings; following does not freeze handle lengths or angles to one master.
The precise evaluation of geometry and easing remains to be specified; do not treat this interpretation as an accepted mathematical representation.

An independent off-curve trajectory could in principle change the intermediate outline's tangent or curvature without moving its on-curve node.
That specialized behavior is not a requirement and does not justify adding the rejected override.
Future requests for such control would need a separate design discussion that respects connected following.

The rule concerns interpolation editing, not a restriction on ordinary outline drawing.
Do not infer authorization to alter master drawings from it.
Both endpoint drawings stay fixed, including the approved Regular `a`.
Linked motion preserves existing local relationships; it does not repair existing kinks or guarantee good stroke thickness throughout the interval.
Visual inspection and Eli's approval remain necessary.

Implementation research must account for overlapping group selections and avoid applying a correction twice.
Quadratic segments with shared controls or implied on-curve points require an explicit interpretation of connectivity rather than assuming every source uses cubic handles.
These are unresolved representation details, not exceptions already accepted by Eli.

#### Accepted continuity interaction between intervals

With Light, Regular, and Bold masters, a point can arrive at Regular along one trajectory and leave along another.
The drawings can meet exactly while the movement changes direction or speed abruptly at Regular.
This is distinct from the outline-handle connection rule above.

Any continuity aid must respect the accepted local scope and must not silently modify an unselected neighboring interval.
Decision accepted by Eli: smooth the join when both adjoining intervals are selected, while keeping single-interval edits local.
The operation must preserve the shared master drawing and adjust only the selected trajectories in the two selected intervals.
Single-interval editing would continue to leave the other interval unchanged.
The required relationship between incoming and outgoing trajectory directions, weight-relative speeds, and easing remains open.
Do not automatically force a stop at every master or force smoothness across an intentionally abrupt trajectory change.

Decision: connected off-curve following and the explicit smooth-join interaction are accepted.
The exact continuity order, handling of unequal axis spans, and interaction with easing remain implementation-planning details; do not assume those mathematical choices were decided.

### Question 4: persistence and interoperability

Eli clarified that this decision must start with compatibility with other editors and Babelfont, including Glyphs HOI import and designspace/UFO export.
The earlier recommendation to choose between a Runebender designspace key and a companion file is withdrawn as premature.
No storage location or private HOI schema is accepted.
A standard extension container is not itself an industry-standard interpretation of the stored data.

#### Three separate interoperability claims

1. Editable authoring round trip: preserve trajectory type, controls, timing meaning, axis/span association, and point identity so the design remains editable.
2. Evaluated-shape fidelity: reproduce the intended outlines across the designspace, even if the destination exposes different controls.
3. Opaque preservation: retain uninterpreted source data for possible later write-back without claiming that the editor evaluates it.

Each conversion path must state which of these it supports.
A successful parse or a valid UFO is insufficient proof of any stronger claim.
Generated sampled masters are an approximation, not lossless editable HOI interchange.

#### Glyphs 4 has a documented authoring representation

The [Glyphs 4 file-format specification](https://github.com/schriftgestalt/GlyphsSDK/blob/Glyphs3/GlyphsFileFormat/GlyphsFileFormatv4.md#spec-glyphs-4-nodeAttr) defines `nodeAttr.hoi`, keyed by axis tag and stored on the lower node of the span.
It describes an intermediate position (`ip`), rotation center/direction (`rc`, `rd`), or Bézier time/amplitude controls (`tl`, `tu`, `al`, `au`).
Those Bézier values are expressed relative to the endpoint connection, rather than simply being absolute handle coordinates.
This is concrete prior art for an adapter and semantic model, not a cross-editor standard by itself.

The spec and actual saved fixtures must be reconciled before deciding details such as control signs, coincident endpoints, span selection, or rotation approximation.
Independent easing proposed for Runebender must also be checked against the documented timing representation; do not promise an exact mapping for every combination.

#### Runebender import support does not establish HOI support

The current [foreign-source load path](../src/font/project.rs) calls Runebender's own Glyphs or Fontra adapter to create a working UFO/designspace representation.
It does not simply ask Babelfont to convert every input format losslessly.

The [Glyphs importer](../src/formats/glyphs_import.rs) targets Glyphs 2/3 through Rust `glyphslib` 0.2.7.
Its point conversion creates UFO points from coordinates, type, and smoothness, without transferring node attributes or identifiers.
The dependency's node type has user data but no explicit Glyphs 4 HOI attribute model.
No Glyphs 4 HOI import mapping was found.
The [write-back adapter](../src/formats/glyphs_export.rs) documents loss of node user data and path attributes on rebuilt edited layers.
Keeping an unedited foreign-source payload is different from making its HOI usable in the UFO working copy.
Whether particular Glyphs 4 files parse at all has not been tested here.

The [Fontra adapter](../src/formats/fontra.rs) reads basic outlines and full font sources, but explicitly omits cross-axis mappings, sparse and intermediate glyph sources, local glyph axes, and variable-component locations.
It bypasses the pinned Babelfont Fontra loader because that loader is incomplete, as documented in the module header.
These gaps can affect both ordinary variations and a future linked-axis HOI conversion.
Opening `.fontra` is therefore not a claim of complete variation interchange.

#### What Babelfont currently contributes

Runebender pins `babelfont-rs` revision `29bdedbbfa7d3150b651dbd7c94fce6b79677ca4`, with `types`, `glyphs`, and `fontir` features.
The inspected model has nodes, layers, sources, and `format_specific` data, but no dedicated HOI trajectory model or evaluator was found in that revision.
Its Glyphs node conversion handles selected user data through the Glyphs 3 representation; generic format-specific storage is not evidence of Glyphs 4 HOI conversion.
Runebender's existing quadratic HOI lives in its own typed preservation records and interpolation override.

See the [pinned Babelfont node implementation](https://github.com/simoncozens/babelfont-rs/blob/29bdedbbfa7d3150b651dbd7c94fce6b79677ca4/babelfont/src/common/node.rs).
This finding concerns the actual pinned dependency, not every release or future branch of Babelfont.
Rust `glyphslib` used here and Python `glyphsLib` are separate libraries; their capabilities must not be conflated.

#### Fontra representation and conventions

Fontra's [core model](https://github.com/fontra/fontra/blob/main/src/fontra/core/classes.py) includes sources, layers, axes, cross-axis mappings, and custom data.
No dedicated HOI trajectory field was found in the inspected model, and its [roadmap](https://github.com/fontra/fontra#roadmap) still lists built-in NLI without a completion mark.
Its [designspace backend](https://github.com/fontra/fontra/blob/main/src/fontra/backends/designspace.py) provides useful real interchange precedent: standard designspace data plus documented-by-code keys such as `com.black-foundry.glyph-designspace` and `xyz.fontra.customData`.
Those keys are not an HOI standard.
External Bézier scripts demonstrate generated source/control layers, but do not establish a native shared trajectory schema.

#### UFO and designspace: standard containers versus shared semantics

The [UFO extension guidance](https://unifiedfontobject.org/extensions/) permits namespaced lib data and object identifiers.
The [GLIF specification](https://unifiedfontobject.org/versions/ufo3/glyphs/glif/) defines outlines, point identifiers, and glyph lib storage.
The [designspace specification](https://fonttools.readthedocs.io/en/latest/designspaceLib/xml.html) provides sources, axis mappings, multidimensional mappings, and extension libs.
These are established mechanisms to reuse, but the inspected specifications do not establish a common editable HOI-control schema.
The [UFO 4 page](https://unifiedfontobject.org/versions/ufo4/) describes ongoing development, not a published solution to assume.

An appropriately constructed multidimensional designspace/UFO source set can be a basis for polynomial HOI output using linked axes and avar2.
That possibility does not imply that another editor reconstructs the original trajectory controls or that Runebender's current adapters support all required data.
A sampled-source fallback must be labeled approximate and cannot replace the editable source information silently.
No universal lossless Glyphs HOI to UFO to Fontra round trip has been demonstrated by this research.

#### Revised planning direction

- Start from published representations, especially Glyphs 4's documented node HOI semantics, and existing Fontra/UFO/designspace conventions.
- Define a format-neutral semantic model capable of retaining supported source behavior, with adapters at format boundaries.
- Assess whether common types and conversions belong upstream in Babelfont or its parser dependencies rather than building parallel Runebender-only interpretations.
- Reuse existing conventions where applicable; if a shared HOI convention is missing, prepare a documented proposal for upstream discussion before calling a new extension interoperable.
- Treat our connected-handle editing rule as a rule for new edits; do not silently normalize imported trajectories or destroy source behavior to fit the UI.
- Preserve unsupported data where possible and report unsupported semantics explicitly; do not silently flatten HOI to linear interpolation.
- Leave the approved Regular `a` untouched, including identifiers and metadata; use separate disposable fixtures for any later authorized conversion experiments.

These are planning recommendations under the accepted compatibility-first constraint, not an approved schema or authorization to code or contact upstream maintainers.

#### Compatibility verification backlog

| Path | Required evidence | Current status |
| --- | --- | --- |
| Glyphs 4 HOI to Runebender | Parse and evaluate intermediate, Bézier, rotation, multiple spans, and timing from actual saved sources | Schema identified; adapter missing or unproven |
| Glyphs to designspace/UFO to Glyphs | Preserve editable semantics and evaluated geometry, including after a scoped edit | Not demonstrated |
| Fontra to Runebender and back | Preserve supported sources, mappings, custom data, and any adopted HOI representation | Basic adapter exists with variation gaps |
| Designspace/UFO in other editors | Verify opaque retention separately from evaluation and editable control recovery | No common HOI authoring schema established |
| Babelfont conversion boundaries | Show which fields are typed, preserved opaquely, transformed, or unsupported at each boundary | Pinned model inspected; no typed HOI support found |

Later fixtures should cover unchanged and edited round trips, unknown attributes, coincident endpoints, contour reordering, off-curve behavior, intermediate sources, and multiple axes.
Compare authoring data and intermediate geometry separately; retain exact endpoint checks and explicit approximation tolerances.
Fixtures must be separate from Virtua's approved sources.
No foreign-format conversion experiments, dependency changes, or application code changes were performed for this interoperability research.
The later authorized HOI Shapes fixture uses newly drawn synthetic UFO sources, not converted production artwork.

Clarification accepted from Eli: the first pass does not need perfect interoperability.
The compatibility cases above describe goals and risks to track, not prerequisites that block an initial useful feature.
Use existing conventions, preserve imported information where feasible, and report unsupported behavior explicitly.
Keep the storage decision provisional until focused implementation research identifies a practical fit.
Do not require a complete field-by-field mapping or universal round-trip proof before beginning a separately authorized first implementation.
Question 4's planning direction is settled at this level; the detailed schema and compatibility backlog remain open.

### Question 5: first-version export

Decision accepted by Eli: use sampled intermediate sources for variable-font export in the first version and evaluate HOI directly for static instances.
Preserve the editable authoring instructions separately from generated export data.
Defer linked-axis polynomial output with avar2 to a later stage rather than making it a prerequisite for initial authoring and export.
This accepted direction fits the existing interpolation/compiler architecture; it does not claim that sampling is the only or mathematically exact representation.

#### Accepted export behavior

- Keep authored trajectories and easing as the editable source of truth in Runebender.
- For a static instance, evaluate the authored interpolation at the requested location before the normal outline conversion and compilation steps.
- For a variable font, sample the affected glyph intervals into compatible intermediate sources and compile them through the normal variation pipeline.
- Add more samples where the composed trajectory and easing need them, using an explicit approximation tolerance and a bounded export size/cost.
- For designspace/UFO export, offer a separately generated source set containing those intermediate drawings; label it as a baked approximation rather than editable HOI interchange.
- Never overwrite original masters or replace authored HOI instructions with the generated approximation.
- Keep the protected Regular `a` source byte-for-byte unchanged.
- Let the designer compare the authored interpolation with the compiled export preview.

The first variable-font target should be the project's existing TrueType compilation path.
Do not promise a new CFF2 export path as part of this decision.
Generated sources should be glyph-local where the compiler supports that correctly; verify sparse-source behavior rather than assuming it.
The consolidated plan starts with a synthetic single-axis font covering both two-source intervals and a three-master join.
General multi-axis guarantees remain separate work.

#### Approximation and limits

The resulting variable font follows piecewise-linear motion between sampled locations.
With sufficiently dense sampling this can look smooth, but it does not preserve an exact curved trajectory or exact continuous derivatives at sample boundaries.
An explicit smooth join in the authoring model remains design intent that the exported approximation must reproduce within its declared limits.
Do not describe sampled output as mathematically identical to the authoring model.

Choose a geometric error budget in font units during implementation planning, considering point motion, outline conversion, rounding, file size, and intended display sizes.
Measure error against the full composed trajectory and easing, not only the geometric trajectory before easing.
A midpoint-only test can miss deviations elsewhere; use conservative bounds where feasible and verification locations beyond the samples used to construct the approximation.
Finite sampled comparisons alone do not prove a global bound.
If the exporter cannot meet its declared tolerance within the sample budget, report the limitation instead of silently reducing fidelity.
Do not assign an arbitrary universal tolerance before representative examples have been measured and visually reviewed.

#### Required validation for v1

1. Verify source preservation and fixed master endpoints, distinguishing authored coordinates from normal compiled coordinate rounding.
2. Compare the evaluated authoring model, generated intermediate-source interpolation, and compiled-font outlines at endpoints and unsampled interior locations.
3. Include linear controls, curved paths, ease-in/out, and a shared-master join with unequal interval lengths.
4. Inspect complete glyph outlines and text, not just selected point coordinates; check counters, connected handles, anchors, widths, and composites where affected.
5. Check exported fonts in Runebender's compiled preview and representative external consumers before claiming external fidelity.
6. Confirm that unsupported imported HOI cases and approximation limits are visible rather than silently dropped.

These are future checks, not tests already performed or authorization to generate font candidates.
Eli's visual approval remains separate from numerical and compiler checks.

#### Deferred export work

Linked-axis polynomial output with avar2 remains a valuable later route for compact and mathematically faithful polynomial trajectories.
Revisit it after the authoring model and supported easing semantics are stable, with separate compatibility and round-trip tests.
Keep the compiler boundary modular enough to add this route without redesigning authoring.
Full editable HOI interchange with other editors remains the question 4 backlog and is distinct from delivering a font that renders the intended shapes.

Decision: accepted.
The consolidated plan below defines delivery stages and validation; this decision does not itself authorize coding.

## Consolidated implementation plan

The five product decisions established the roadmap below, and Eli subsequently authorized Phase 1.
The implementation and validation section above records current progress against that roadmap.
The sections below are the recommended engineering plan, not a claim that implementation or validation has happened.
Technical defaults marked as recommendations may be adjusted when fixtures expose a problem, provided the accepted behavior above is preserved and this document records the reason.
Reopen a product decision only if the required behavior must change.

### First-version contract

| Area | First-version behavior |
| --- | --- |
| Primary authoring | Select a connection between corresponding on-curve nodes and edit its trajectory while both endpoint drawings remain visible and fixed |
| Alternate authoring | Draw a temporary intermediate target and derive trajectory controls for the selected nodes; direct editing remains primary |
| Locality | One glyph, selected node groups, one explicit continuous axis, one neighboring-source interval at a time; explicitly select two intervals for a join operation |
| Outline handles | Connected cubic off-curves receive their on-curve node's correction; no independent off-curve trajectory tool |
| Timing | Linear, ease-in, ease-out, and ease-in-out progression, applied to the selected trajectories |
| Joins | A deliberate smooth-join operation on two selected adjacent intervals; no automatic change to an unselected interval |
| Editable save | Versioned authoring metadata with validated identities, undo/redo, and save/reopen support; storage settled in Phase 1 without modifying the protected Regular GLIF |
| Static export | Evaluate the authored geometry at the requested location, then use the existing compilation pipeline |
| Variable export | Bake adaptive intermediate sources into a temporary compilation snapshot for the existing TrueType path |
| Source export | Write a separate baked designspace/UFO package with an approximation report; retain the editable original |
| Interoperability | Preserve supported source data and report unsupported HOI; full editable Glyphs/Fontra round trips are follow-up work |

Recommended first supported authoring case: compatible cubic outlines with explicit on-curve nodes in a single-axis designspace, including more than two masters and unequal neighboring intervals.
This supports the Virtua use case and exercises the accepted join behavior without claiming a general multidimensional interpolation system.
An isolated UFO without a variable source arrangement is not sufficient to author an interval.
Existing non-HOI behavior must remain available for other fonts.
For HOI authoring or export outside the supported set, show a specific limitation instead of guessing another axis, flattening the correction, or altering the font's existing variation structure.

Quadratic outlines with shared off-curves or implied on-curves require a separate connectivity design before new HOI edits can be enabled on them.
Do not silently convert or resegment original sources to fit the initial cubic workflow.
Widths, anchors, kerning, and component transforms retain ordinary interpolation; references to an affected base glyph must still render its corrected outline correctly.
Existing bracket substitutions remain active unless a separately authorized font-design change removes them.
HOI alone cannot smooth a substitution that still switches to a different glyph.

### Engineering defaults and later behavior

#### Canonical identity, scope, and storage

Represent a trajectory as a canonical document record identifying the glyph, axis, lower and upper sources/layers, corresponding node identities, curve controls, easing, and format version.
Use existing canonical identities during the session, but do not assume that session-local numeric IDs survive save/reopen or that equal IDs in different layers establish point correspondence.
Store enough persistent source and point correspondence information to rebind safely after reopening, using existing file identifiers where available and a guarded mapping where absent.
A stale or ambiguous mapping must suspend the affected correction with a diagnostic; never fall back to whichever point occupies the old index.
Source insertion, deletion, relocation, axis edits, and contour topology changes must update a demonstrably valid mapping or invalidate it explicitly.

Resolve neighboring intervals from the actual contributing sources for the glyph, including supported sparse intermediate sources.
Recommended policy: real authored intermediate drawings are fixed boundaries, not targets that an endpoint curve may override.
Reject duplicate or ambiguous endpoint locations until a clear source selection exists.
Keep generated bake samples separate from authored sources so that saving or compiling does not create new authoring intervals.

Phase 1 selected the versioned Designspace-lib representation described above after validating filesystem and browser saves.
The following storage-selection notes retain the rationale and alternatives considered during planning.
Evaluate a versioned namespaced designspace-lib record first because the current model preserves that container and it can avoid writing the Regular GLIF.
This is a candidate to verify, not a revived claim that the earlier withdrawn storage proposal is an industry standard.
If it fails the preservation requirements, document and implement the smallest companion-data alternative before enabling editable save.
Keep the semantic model independent of that codec and leave adapters able to translate supported Glyphs or future shared conventions.
Define one key constant, one reader, and one writer for each adopted extension, and retain unknown versions opaquely with an explicit unsupported status.

#### Trajectory, easing, and connected handles

Recommended new-authoring model: a cubic Bézier trajectory with its endpoints anchored to the two source nodes, plus a separate monotone easing function.
Linear and quadratic trajectories can be represented within the cubic model, so a straight connection does not require a different editing mode.
Define interval progress in mapped design coordinates and convert slider values through the project's existing axis mapping.
Reject zero-length intervals and non-finite data.
Keep easing endpoints at zero and one, and restrict initial presets to monotone progress without time reversal.
The exported sampler must evaluate the composition of trajectory and easing.

For a supported interval, let `B(t)` be a node's ordinary interpolated position and `P(t)` its authored trajectory position after easing.
Apply the added displacement `D(t) = P(t) - B(t)` to that node and each of its connected outline off-curves.
An off-curve with ordinary position `H(t)` therefore becomes `H(t) + D(t)`.
This lets source-defined handle vectors change naturally with weight while following the corrected node.
Apply each correction once, even when a node and its handles are both selected.
At both endpoint masters, `D(t)` must be zero; outside the interval it must be inactive.
Unselected groups retain their previous evaluation, including any existing supported corrections.
Imported or legacy independent off-curve behavior must stay separate from this new-editing rule until explicitly converted with equivalent geometry or reported as unsupported.

Recommended smooth-join target: equal incoming and outgoing first derivatives with respect to the mapped design-axis coordinate.
Account for each interval's length and the derivative of its easing function; matching the lengths of visible trajectory handles alone is insufficient.
Modify only the two selected intervals in one undoable operation and preserve the shared master position.
Choose a deterministic least-change adjustment, preserve an already smooth join, and do not create a zero-speed stop merely to satisfy the equation.
If fixed easing endpoints prevent the intended join, report the constraint or offer an explicit adjustment of easing within the selected pair.
This is a C1 authoring target; the sampled variable font will reproduce its shape only within the export approximation budget.

#### Intermediate target semantics

Recommended first implementation: a drawn target initializes trajectory controls once, with a preview before applying the fit.
Keep the target as a separate scratch reference that can be revisited; it is not a permanent constraint that secretly changes later trajectory edits.
Expose the target location and selected groups explicitly, and leave the source drawings untouched.
Start with one interior target per selected group and fit the simplest trajectory through the endpoints and that target at the current eased progress.
A quadratic through one target can be represented exactly as a cubic for subsequent direct editing; endpoints and a target alone do not uniquely determine a general cubic.
Report degenerate timing or unattainable constraints instead of choosing unstable controls.
The linked-handle rule still applies: arbitrary independently redrawn handle vectors cannot be promised as an exact fit.
Show the target-versus-result difference and explain unsupported handle-only changes before applying the fit.
Persistent target constraints, fitting many targets at once, and exact bidirectional target/trajectory editing are later extensions.

### Delivery sequence and completion gates

#### Phase 1: executable contract and preservation baseline — implemented

1. Recheck the implementation checkout, module headers, toolchain, and current dirty files before editing anything.
2. Add small synthetic fixtures for straight and curved motion, coincident spatial endpoints, three masters with unequal intervals, mapped axis coordinates, connected cubic handles, and an authored sparse intermediate source.
3. Establish stable correspondence and the storage codec described above, including save/reopen and filesystem/browser preservation tests.
4. Specify legacy `com.runebender.hoiIntermediate` handling: retain the original payload and behavior until an explicit supported conversion can preserve its evaluated geometry.
5. Record the supported source arrangements and unsupported-case diagnostics in this document.

Completion gate: a trajectory can be represented, serialized, rebound, and removed without changing either endpoint outline or silently reassigning a point.
The existing font without HOI evaluates and saves as before.
No Virtua source is used as a writable test fixture.
Following Eli’s direction to use HOI Shapes first, Phase 1 checks exact GLIF retention on all 18 synthetic glyph files and checks the protected Regular source hash read-only.
A later Virtua-specific save proof must use a separately authorized disposable copy; do not relax the protection of the actual source.

#### Phase 2: shared evaluator and a minimal complete export path

Status: completion gate met on 2026-10-29 using disposable HOI Shapes projects.
Exact evaluation, static snapshots, and glyph-local sampled variable compilation are validated as recorded in the Phase 2 evidence above.
Virtua Grotesk and the baseline HOI Shapes drawings remain unchanged.


1. Extend the engine to evaluate one explicit scoped trajectory through the canonical project operations, replacing global-first-axis assumptions for the new model.
2. Add connected-handle displacement, easing evaluation, fixed endpoints, and locality checks.
3. Add canonical history and revision invalidation so edits, undo, redo, and save/reopen all produce the same geometry.
4. Route static snapshot generation through the same evaluator and prototype the generated intermediate-source path on a small synthetic variable font.
5. Compare authored geometry, generated-source interpolation, and compiled output at locations not used to construct the samples.

Completion gate: one saved curved trajectory survives reopen and appears in both an exported static instance and a variable font within a declared test tolerance.
This is the first end-to-end milestone; a curved canvas preview alone is not completion.
Prove whether the pinned Babelfont/fontc path handles glyph-local intermediate sources correctly.
If it does not, assess temporary full sources at the required locations, populated from ordinary evaluation for unaffected glyphs, and verify that they preserve unaffected interpolation.
Use that fallback only when its fidelity and cost are demonstrated; otherwise report the compiler limitation and resolve it before claiming variable export.

#### Phase 3: direct interpolation editing

Status: completion gate met on 2026-10-29 with HOI Shapes, the native interaction tests, and the actual browser workflow recorded above.
Master outlines stay fixed; pointer drafts enter canonical history only on release.

1. Add an interpolation mode using the existing tool, command, selection, and canvas infrastructure.
2. Show the selected axis and interval, ghost both endpoint drawings, and draw selectable point connections with screen-space hit testing that remains usable while zooming.
3. Let dragging a selected straight connection create or adjust its curve; show trajectory controls distinctly from outline handles and keep master points fixed.
4. Support multiple selected node groups, easing presets, reset of the selected correction, cancel, and one undo step per completed gesture.
5. Scrub the interval with a live evaluated outline, and expose a compiled-preview comparison with clear pending/error state.

Completion gate: the designer can select a connection directly, bend it, change easing, scrub, undo, save, and reopen without moving a master or changing an unselected interval.
A reset restores the relevant ordinary interpolation rather than replacing the source drawings or deleting unrelated corrections.
Use the repository's design tokens, controls, and panel rows; headless screenshots verify presentation, while actual interaction checks verify selection, dragging, and keyboard behavior.

#### Phase 4: adjacent joins and intermediate targets

1. Add the deliberate two-interval selection and smooth-join command using the derivative contract above.
2. Verify unequal spans, nonlinear axis maps, zero easing derivatives, coincident positions, existing smooth joins, and intentionally unsmoothed single-interval edits.
3. Add a temporary intermediate drawing workflow with source endpoints locked and a clear target location.
4. Fit selected node trajectories using the one-time initialization semantics, preview the result, and apply it as one transaction.
5. Preserve the scratch reference and make any mismatch from linked-handle constraints visible.

Completion gate: both accepted authoring workflows operate on the same canonical data, and a join operation cannot modify an unselected span.
The target workflow does not introduce a second hidden source of truth or require a change to the approved Regular drawing.

#### Phase 5: production export and compatibility diagnostics

1. Replace the minimal bake with adaptive sampling of the complete composed evaluation, with bounded depth, sample count, and export cost.
2. Select and document a default geometric tolerance using representative fixtures and rendered inspection; keep sampling, outline-conversion, coordinate-rounding, and compiled-measurement errors distinguishable.
3. Use conservative error bounds where supported and independent verification locations elsewhere, and label measured fidelity separately from a proven bound.
4. Produce a separate baked designspace/UFO package and TrueType variable output through the same bake implementation, with deterministic sample locations and a report of tolerance, sample counts, and unsupported cases.
5. Add freshness and cache rules for edits, source/axis changes, undo/redo, and export-quality settings so a stale compiled font cannot masquerade as the current result.
6. Detect known unsupported imported HOI before a lossy boundary and retain original data where feasible; extend parsers only as needed for that bounded contract, without promising full Glyphs 4 support.

Completion gate: exports either meet the stated fidelity contract or fail with an actionable diagnostic.
No silent linear fallback is allowed when authored HOI cannot be exported.
Generated output must not contain active authoring metadata that would apply the correction again when reopened.
Keep provenance and the editable source reference separate from any active evaluation instructions.
Do not create generated samples in the live document, grow sample sets on every save, or overwrite the editable source package.
Static export must evaluate the chosen location's supported metrics, anchors, components, and substitutions consistently before normal compilation, rather than merely freezing an unrelated sampled preview.

#### Phase 6: integration, proof, and documented release limits

1. Complete focused engine, persistence, history, and export checks, then run the relevant native suite from `AGENTS.md` with the pinned toolchain.
2. Build the browser workspace and run its documented quality and folder-save checks, adding coverage for the shared HOI interactions and metadata.
3. Inspect the default-theme headless UI and perform targeted native/browser interaction checks before claiming either host works.
4. Compare final compiled outlines and rendered text in Runebender and representative external consumers; record the tested applications and versions.
5. Update user-facing interpolation, export, format-preservation, and known-limitations documentation where the delivered behavior changes their claims.

Completion gate: the first-version contract above is demonstrated, supported and unsupported cases are documented, and outstanding failures remain visible.
Run broad checks once the relevant implementation stabilizes, then repeat them only for new changes or unresolved failures.
Follow the repository's memory-dependent Cargo concurrency rules and keep browser checks separate from Cargo builds on lower-memory hosts.
Do not claim deployment, public-release readiness, complete interoperability, or Eli's approval of a font design from local tests alone.

### Code routing

These are inspected existing entry points, not a requirement to put every new concern in the current large files.
New small modules may be added within the owning area when they keep evaluation, codecs, and sampling easier to reason about.

| Concern | Existing entry points and planned responsibility |
| --- | --- |
| Canonical records and axes | [scoped HOI model](../src/font/model/hoi_trajectories.rs), [legacy HOI model](../src/font/model/hoi.rs), [designspace model](../src/font/model/designspace.rs), [variable identities](../src/font/variable.rs): scoped records, persistent rebinding, source arrangement validation |
| Evaluation and sampling | [HOI scope and binding](../src/font/hoi.rs), [Project](../src/font/project.rs), [interpolation](../src/font/interpolation.rs), [legacy evaluator](../src/formats/metadata/lib_keys.rs): one engine evaluation path and bounded bake preparation |
| Mutations and history | [canonical draft operations](../src/font/babelfont/edit_components.rs), [point edits](../src/font/babelfont/edit_points.rs), [history](../src/font/history.rs): transactions, topology invalidation, undo/redo |
| Native and browser persistence | [persistence](../src/font/persistence/mod.rs), [memory packages](../src/font/persistence/memory.rs), [UFO projection](../src/font/babelfont/ufo_projection.rs): versioned codec, exact preservation, no writes to protected GLIFs |
| Foreign adapters | [Glyphs import](../src/formats/glyphs_import.rs), [Glyphs export](../src/formats/glyphs_export.rs), [Fontra](../src/formats/fontra.rs): capability diagnostics and opaque retention, later semantic conversions |
| Static and variable compilation | [compiler](../src/font/compiler/mod.rs): immutable evaluated/baked snapshots, cache invalidation, shared preview/export behavior |
| User intent | [commands](../src/application/editor/commands.rs), [tools](../src/application/editor/tools/mod.rs), [actions](../src/application/actions.rs): mode, gestures, selection, easing, joins, and target fitting |
| Presentation | [canvas](../src/application/view/canvas/), [panels](../src/application/view/panels/), [design system](../src/application/design/mod.rs): endpoint ghosts, controls, progress, and export comparison |
| Host integration | [browser guide](../web/README.md): shared behavior, package saves, build and interaction checks |

Keep Babelfont/fontdrasil/Norad conversion behind existing adapters and keep UI state out of the engine.
Use Project mutations and save paths rather than writing metadata directly from a canvas tool.
No new dependency or wholesale font-model replacement is assumed by this plan.
If a parser or compiler upgrade becomes necessary, review its exact scope and pin separately.

### Validation matrix

| Property | Required evidence |
| --- | --- |
| Fixed artwork | Exact endpoint coordinates and source-byte preservation for the protected Regular fixture, including after undo, save, reopen, and export |
| Local edits | Selected groups change; unrelated nodes, handles, glyphs, sources, and intervals do not; mixed selections do not double-apply corrections |
| Timing | Linear and eased paths agree with their specified composed evaluation, including mapped axes and zero endpoint derivatives |
| Continuity | Derivatives agree across a deliberately smoothed unequal-span join; a single-span edit leaves its neighbor unchanged |
| Identity | Reordering, deletion, insertion, and source replacement either retain verified correspondence or produce an explicit invalidation; no index-based retargeting |
| Persistence | Known data round trips, unknown/malformed data is retained where possible, browser and native saves agree, and legacy geometry is not silently normalized |
| Compilation | Static, generated-source, and final TrueType outlines compared at endpoints and independent interior locations, including after cubic-to-quadratic conversion |
| Unaffected behavior | No-HOI fonts and unaffected glyphs retain ordinary interpolation; components, metrics, and existing substitution behavior remain consistent |
| Export cost | Sample limits, failure paths, repeated exports, deterministic output, and stale-cache prevention are exercised |
| Actual usability | Direct line selection, dragging, easing, target fitting, join selection, keyboard/cancel, and undo work in supported hosts |
| Design quality | Rendered full-glyph and text inspection followed by Eli's review; numerical agreement alone is not approval |

Use synthetic fixtures before involving Virtua.
Any later Virtua proof needs separately authorized scratch Bold/alternate artwork and a defined bracket-rule policy; neither is a prerequisite for coding the general feature.
Never promote an unfinished Bold drawing or a generated transition to approved artwork automatically.
Record source paths, hashes, font locations, export settings, and inspection results with each proof so the result can be reproduced.

### Deferred work and readiness

Defer linked-axis avar2 export, CFF2-specific work, general multi-axis composition, shared-control quadratic authoring, independent anchor/metric/component trajectories, persistent multi-target fitting, and complete editable HOI interchange.
Keep actual Glyphs 4 fixtures, Fontra sparse-source and mapping coverage, upstream Babelfont types, and cross-editor round-trip comparisons in the compatibility backlog above.
These do not block the supported single-axis implementation; explicit limitations and source preservation do belong in its first version.

Readiness assessment: Phases 1–3 have met their scoped completion gates; the direct editor is ready for hands-on testing before Phase 4 join smoothing and intermediate targets.
The remaining choices are bounded engineering tasks with tests and completion gates, rather than unanswered product questions requiring another research cycle.
Storage/rebinding, linked-handle evaluation, and sparse-source compilation must be demonstrated early; do not postpone them until the UI appears complete.
The original planning work did not change application code, production fonts, or dependency pins.
Phase 1, Phase 2, and Phase 3 application changes and runtime checks are recorded above, while production font sources and dependency pins remain outside this implementation’s writes.


### Prepared synthetic fixture

At Eli's request, [HOI Shapes](../assets/font-sources/variable-fonts/HOIShapes/README.md) is the first development fixture.
Open [HOIShapes.designspace](../assets/font-sources/variable-fonts/HOIShapes/HOIShapes.designspace) to load Light 100, Regular 400, and Bold 900.
The four visible glyphs are a moving square, a moving cubic circle, a two-contour ring, and a fixed square, mapped to `ABCD`.
The unequal intervals exercise neighboring-span joins, while the fixed square provides an unaffected-glyph control and a coincident-endpoint case.
Sources have explicit point identifiers and matching topology across masters.

This is a baseline source font with ordinary interpolation, not a completed HOI demonstration.
It contains no new trajectory schema, baked samples, bracket substitutions, or production artwork.
Later implementation work should derive temporary test copies and add expected HOI behavior there.
The fixture does not yet cover sparse sources, mapped axes, components, foreign-format payloads, or topology invalidation; add those focused cases in the relevant phases.

UFO validation, compatibility checking, and a scratch variable-font compilation passed through fontmake/ufo2ft/FontTools.
Independent checks verified point identities and topology, the unchanged fixed square, and compiled glyph bounds and widths at ten weights including unsampled interior locations.
The maximum measured bounds error against expected ordinary interpolation was zero font units; this bounds check does not establish full-outline equivalence or future HOI fidelity.
The saved-source proof was inspected; neither Runebender's native nor browser editor was launched for this fixture.
These results establish a usable baseline, not runtime proof of the planned HOI feature.
The original Virtua Regular `a` remains protected and unchanged.

## Decision and update record

| Date | Record | Status |
| --- | --- | --- |
| 2026-10-29 | Preserve approved Regular `a`; Bold/alternate drawings remain unfinished | Confirmed by Eli |
| 2026-10-29 | Save research under `runebender/docs/`, keep it updated, and discuss five questions sequentially | Confirmed by Eli |
| 2026-10-29 | No implementation or font changes during planning | Confirmed by Eli |
| 2026-10-29 | Combined authoring with direct trajectory editing primary | Accepted by Eli |
| 2026-10-29 | Interpolation mode showing both masters and selectable connections, familiar curve editing, and easing controls | Requested by Eli |
| 2026-10-29 | Local scope for selected trajectories, glyph, axis, and neighboring-master interval | Accepted by Eli |
| 2026-10-29 | Connected off-curve points always follow their on-curve nodes; omit independent trajectory override | Accepted by Eli; replaces earlier optional-override proposal |
| 2026-10-29 | Explicit smooth join when both adjoining intervals are selected; single-interval edits remain local | Accepted by Eli |
| 2026-10-29 | Project-level HOI metadata in the existing designspace | Earlier recommendation withdrawn as premature |
| 2026-10-29 | Prioritize compatibility with Glyphs, Fontra, Babelfont, and designspace/UFO before choosing storage | Required by Eli; refined by the pragmatic first-pass clarification below |
| 2026-10-29 | Interoperability edge cases are staged goals, not complete first-version prerequisites | Clarified by Eli |
| 2026-10-29 | Sampled intermediate sources for variable export, direct evaluation for statics, avar2 deferred | Accepted by Eli |
| 2026-10-29 | Consolidate all five decisions into a detailed implementation plan and assess readiness | Requested by Eli; plan prepared, coding not started |
| 2026-10-29 | Add a small synthetic font under `assets/font-sources` before editing Virtua | Authorized by Eli; HOI Shapes created and baseline validated |
| 2026-10-29 | Start Phase 1 using HOI Shapes and leave Virtua unchanged | Authorized by Eli; scoped storage and preservation implemented with 119 selected Rust tests and the browser preservation check passing |
| 2026-10-29 | Experimental Designspace-lib schema with guarded correspondence and exact unchanged-GLIF retention | Engineering choice verified for Phase 1; not a cross-editor standard |
| 2026-10-29 | Broader browser smoke assertion for splitter cursor | Reproduced twice and documented; full browser interaction pass not claimed |
| 2026-10-29 | Implement Phase 2 with HOI Shapes | Exact evaluator, connected handles, easing, guarded history, static snapshots, and bounded glyph-local variable export implemented; 97 selected tests pass; browser preservation passes; existing splitter-cursor failure retained |

| 2026-10-29 | Implement Phase 3 before further export work | Direct trajectory tool and local panel implemented; 118 selected native tests and real browser editing/preservation checks pass; existing general splitter-cursor failure retained; Virtua and baseline HOI Shapes unchanged |

| 2026-10-29 | Keep the editable test font with the other fonts and simplify testing instructions | HOI Shapes Playground added under `assets/font-sources/variable-fonts/`; open it normally, open `box`, and press `I` |

Keep this document updated as engineering choices, implementation milestones, fixtures, and validation results change.
Distinguish accepted behavior, recommended implementation details, completed work, and unverified claims.
Phases 1–3 were explicitly authorized and are recorded above; Phase 4 and later work remain planned until Eli asks to proceed.
