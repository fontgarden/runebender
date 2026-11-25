# Runebender engineering review

November 25, 2026 · [Reviewed revision `dfa8ace`](https://github.com/fontgarden/runebender/tree/dfa8ace636c02d7533743ecfa72e6db1b51547dc)

## Implementation follow-up

| Finding | Local implementation and regression check |
| --- | --- |
| 1 · Browser Save As | A copy now includes the designspace and all loaded UFO masters. It retains the original XML when the descriptor is unchanged. The headless Chrome regression uses the normal Save As command and reopens the saved folder. |
| 2 · Native save retry | A successful working-UFO save leaves a failed write to the original pending. Retry tests reopen Glyphs, Glyphs package, and Fontra originals and verify the saved advance. |
| 8 · Static kerning | Static export resolves missing exceptions from their groups and preserves explicit zero values. Compiled-font tests compare shaped static and variable advances in Regular, at the midpoint, and in Bold. |

Finding 3 is partly addressed.
Acknowledging an older browser save now leaves later edits marked dirty, but the browser still drops an additional Save request received while a write is underway.
One kerning discrepancy also remains in live proofing when a nondefault master has a completely empty kerning table.
The variable and static compilers skip that master for kerning interpolation, while the live interpolation can still blend toward zero.
The [browser usability research](#browser-usability-research--november-25-2026) below sets out the next milestones and distinguishes the published editor from the local work.

The local build now implements portable source import and editable ZIP download, including nested masters and project sidecars.
A two-master round trip in Firefox, WebKit, and Chromium retained all 75 source files and reopened the edited outline from the extracted ZIP.
The edited GLIF was the only changed file.
The other 74 matched byte-for-byte, including the annotated fontinfo and explicitly empty layerinfo plists.
Chromium also exercised a denied folder write, downloaded a backup, and successfully retried the write to the original folder.
Local recovery checkpoints store edited font sources and unapplied feature text in IndexedDB.
Chromium recovered the last committed outline after closing the editing tab and preserved a draft typed in Bold until it was explicitly applied.
Conflict detection for an external edit to a connected folder, responsiveness during compilation, and qualification through the deployed iframe remain open. [Browser workflows and regression checks](web/README.md)

## Original review

**I would fix Save As and save retries before the next release.**
In a headless Chrome session, Save As reported “Saved Core.designspace” and produced a folder that Runebender could not reopen as a family.
On the native engine, retrying a failed Glyphs save returned success while the original still contained the old width.
The next priority is export fidelity.
In a two-master fixture, the editor and variable font agreed on kerning at every location we tested, while the static fonts produced different spacing.

| Priority | What I would schedule | Evidence that closes it |
| --- | --- | --- |
| Before relying on Save for production work | Findings 1–7 on publication, outside edits, document replacement, and recovery | Reopen the user’s actual destination after success, failure, retry, and concurrent editing |
| Before promising production-quality proofing and editing | Findings 8–14 on shaping, export, geometry, and history | Compare output glyphs and positions, contour identity, and undo across sources |
| Alongside the next release | Finding 15 on release validation, then 16–20 on large fonts and accessible input | The release check accepts the actual green SHA and the same user workflows work on both hosts |
| As those changes land | Findings 21–24 on background work, measurements, and ownership | Deterministic scheduling tests, comparable timings, and smaller dependency cycles |

**What was checked.**
The review follows public engine APIs, native commands, the browser folder and input bridges, build checks, and the release script in this repository.
“Reproduced” identifies an executed case and names the boundary exercised.
“Source trace” identifies a reachable implementation path with a proposed regression test.
Native engine probes used the existing local debug library and disposable fonts.
The Chrome check used the existing local Wasm build and an origin-private directory in an isolated profile.
The primary CI run on the exact reviewed SHA passed all six jobs, including the browser editor.
The separate Windows diagnostic failed in its CLI, rendering, and native startup smoke step.
Locally, the engine-only suite passed 865 tests with three ignored, and the Python release, updater, and browser-adapter suites passed all 17 tests.
GPU latency, physical input devices, assistive technology, and competitor products were outside the hands-on checks. [Primary CI](https://github.com/fontgarden/runebender/actions/runs/45539409976) · [Windows diagnostic](https://github.com/fontgarden/runebender/actions/runs/45539409904)

## Saving and protecting work

### 1. Make browser Save As produce a complete, reopenable family

**Reproduced in Chrome.**
I opened the two-master Core fixture, invoked Ctrl+Shift+S, and returned an empty browser directory from the picker.
The normal file writer reported success and wrote 72 files, including both UFOs, but omitted `Core.designspace`.
Reopening those files produced “this folder holds several UFOs and no designspace; open one UFO”.
The encoder emits the descriptor only when designspace metadata is dirty, and the host considers the new-folder flag after encoding.
The existing folder smoke test explicitly excludes the descriptor from its expected set and supplies the success acknowledgement itself. [Encoder](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/persistence/memory.rs#L749-L768) · [Browser save](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/browser.rs#L461-L495) · [Smoke assertion](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/web/folder.cjs#L62-L73)

Pass the save intent into the encoder and produce a self-contained file inventory for a copy.
Exercise the real writer on clean, outline-only, and axis-edited families, then reopen the destination and compare source locations, named instances, and a glyph in each master.
Commit the new directory and its UFO path prefix together after writing succeeds.
Today a failed Save As can reset the prefix while the page still holds the original directory. [Directory and prefix handling](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/web/app.js#L110-L139) · [UFO root](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/browser.rs#L479-L494)

### 2. Report a native save as complete only when the requested source is current

**Reproduced with a Glyphs 3 file.**
With the original temporarily unavailable, saving a width of 610 updated the working UFO and correctly reported a write-back error.
After restoring the original, Save returned success without another edit.
The working UFO reopened at 610 and the original at 510.
The project clears its dirty state after publishing the working copy, then a clean retry exits before the shared Glyphs/Fontra write-back step. [Project save](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/project.rs#L2355-L2369) · [Editor acknowledgement](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/platform/host.rs#L431-L452)

Track the working-copy revision and original-source publication separately.
Keep the original pending until write-back succeeds, and tell the designer where the recovered work exists after a failure.
Test a failed first attempt, a retry with no new edits, and a retry after a newer edit for both adapters.

### 3. Acknowledge the exact browser revision that was written

**Source trace.**
The page captures a save bundle and awaits individual file writes while normal input continues.
A successful callback then marks the current project and workspace clean.
If the designer edits during the write, the newer revision can lose its dirty indicator.
A second Save request received while the folder is busy is consumed without queuing another write. [Asynchronous writer](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/web/app.js#L116-L142) · [Request consumption](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/browser.rs#L424-L430) · [Success callback](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/browser.rs#L497-L510)

Return a document identity and edit revision with each bundle, acknowledge that revision, and coalesce a subsequent Save into a write of the newest snapshot.
A deterministic test can pause a file handle’s `close()`, make a second width edit, and deliver the first acknowledgement.
Assert the editor still shows an unsaved change, then complete the second save and reopen the newer width.

### 4. Preserve unrelated Glyphs data and external edits

**Reproduced on disk.**
Changing only an advance from 500 to 510 removed a locked anchor’s lock and custom user data from the Glyphs original.
In a separate two-glyph fixture, Runebender edited A and an outside writer changed B from 400 to 431.
Saving A succeeded and reverted B to 400.
Width participates in a whole-drawing comparison, whose write path reconstructs anchors and shapes from the UFO projection.
Comparing the current original with a stale working copy also mistakes an external change for an editor change. [Drawing comparison](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/formats/foreign.rs#L197-L204) · [Write-back decision](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/formats/glyphs_export.rs#L274-L298) · [Layer reconstruction](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/formats/glyphs_export.rs#L563-L604)

Track the fields changed since import, patch width alone for a spacing edit, and compare overlapping edits against the imported baseline before replacing native records.
Round-trip a fixture with locked anchors, node data, and interleaved paths and components.
Have an independent writer edit B between open and save, then verify both edits in the original.

### 5. Give deletion and external conflicts explicit file semantics

**Reproduced at the JavaScript writer boundary.**
Clearing the last feature omits `features.fea` from the Rust bundle.
With the existing feature file and a stale GLIF in a directory-handle test double, the actual page writer removed the GLIF, reported success, and read the old feature text back.
An empty kerning table follows the same omission path.
Conversely, the browser prunes any unlisted GLIF in a managed glyph directory, including one created externally since open. [Empty metadata encoding](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/persistence/memory.rs#L457-L475) · [Write and prune](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/web/app.js#L63-L77) · [Save sequence](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/web/app.js#L116-L124)

Native Save already detects an outside change and deliberately publishes the editor’s version over it, then reports what happened.
That policy affects users moving between Runebender, a font build script, and another editor. [Native save policy](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/platform/host.rs#L418-L450)

Keep an inventory of files and revisions read at open or last successful save.
Identify intentional deletions and reject overlapping external changes before writing, with choices to keep a copy or explicitly overwrite.
Verify clearing all features and kerning, adding a GLIF externally, and independently editing different files within the same UFO.

### 6. Preserve the current workspace and its drafts until replacement succeeds

**Source trace.**
Native Open clears `workspace.modified` when the designer chooses Discard, before attempting to load the replacement.
If the new file fails to open, the old workspace survives with its dirty flag cleared and Quit can skip its prompt.
Browser drag-and-drop calls the installation path directly even though menu Open checks the dirty flag. [Native Open](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/workspace.rs#L604-L623) · [Discard and Quit](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/workspace.rs#L715-L754) · [Drop handler](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/web/app.js#L145-L155) · [Menu guard](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/browser.rs#L692-L704)

Script edits only mark the script draft dirty.
A clean font can therefore be reloaded or closed while a script draft contains unsaved work.
Graph edits similarly live in graph state, and in Graph mode a successful font save can replace the message from a failed graph save. [Script draft](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/editor/tools/scripts.rs#L235-L253) · [Graph edit and save](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/editor/tools/graph/mod.rs#L438-L474) · [Save dispatch](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/editor/commands.rs#L1035-L1043) · [Reload guard](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/platform/host.rs#L465-L477)

Treat replacement as a transaction shared by Open, drop, reload, and Quit.
Track each editable resource independently and preserve the old workspace until the new one is ready.
Test a failed Open followed by Quit, a drop with a feature draft, a script-only draft during an external reload, and a graph write failure followed by a successful font save.

### 7. Recover an interrupted multi-source save on the next launch

**Source trace.**
The native exporter stages and validates its output before publication.
During publication it renames the destination to a backup and then renames the stage into place, repeating this for each artifact.
The list needed for rollback lives in process memory, and rollback ignores a failed backup rename.
A process exit between renames can leave a missing source or a family containing masters from different revisions. [Publish sequence](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/persistence/mod.rs#L626-L687) · [Rollback](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/persistence/mod.rs#L705-L710)

Persist a small recovery record containing the destinations, stages, and backups before the first rename.
On open, recover one coherent revision and surface any artifact requiring manual recovery.
Use a disposable two-master family and terminate a child process at each publication boundary, then reopen and compare all masters and the descriptor.

## Drawing, proofing, and export

### 8. Make static kerning agree with the live project and variable font

**Reproduced in compiled fonts.**
In a copy of Core, we set class kerning to −80 in both masters and an A/V exception to −120 in Regular alone.
We shaped `AV` with kerning on and off and measured its advance difference in font units.

| Normalized location | Editor lookup | Variable TTF | Static TTF |
| --- | ---: | ---: | ---: |
| Regular · 0 | −120 | −120 | −120 |
| Midpoint · 0.5 | −100 | −100 | −60 |
| Bold · 1 | −80 | −80 | 0 |

The static compiler takes the union of raw pairs and uses zero when a master lacks a pair.
That manufactured exception masks the inherited class value.
Resolve missing exceptions under the same UFO precedence rules used by the project before interpolation.
Gate the public static compiler with the three-way comparison above, including an explicit zero exception and a right-to-left case. [Static snapshot and compilation](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/compiler/hoi_export.rs#L273-L305) · [Project interpolation](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/project.rs#L1042-L1077) · [UFO precedence](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/canonical_metadata.rs#L286-L315)

### 9. Select shaping scripts independently of text direction

**Reproduced in the public TextBuffer.**
In a font whose Hebrew `ccmp` substitutes `alef.alt`, typing א with automatic script produced `alef`.
Setting the script to `hebr` produced the intended alternate.
The buffer splits a bidi run into Arabic and non-Arabic spans, and the lower-level shaper defaults an otherwise unspecified RTL run to Arabic. [Run construction](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/text/buffer/shaping.rs#L128-L166) · [Shaper setup](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/text/shape.rs#L348-L379)

Itemize by Unicode script within each bidi run, assign Common and Inherited characters using surrounding context, and preserve explicit user overrides.
Check glyph IDs, advances, offsets, and cluster mapping for Hebrew and Arabic on the same line, Latin with combining marks, and an Indic syllable.
Retain the existing Arabic joining and mark-attachment fixtures while fixing the defaults.

### 10. Keep an explicitly inserted glyph in shaping order

**Reproduced in the public TextBuffer.**
With a substitution from `A A` to `A_A`, inserting `A`, the unencoded glyph `X.alt`, and another `A` formed a ligature across `X.alt`.
The named glyph stays visible but drops out of the Unicode shaping input, so its neighbors become adjacent there.
End the shaped run at an explicitly named glyph, or represent it directly in a glyph-aware run.
Assert the displayed order, cursor stops, and hit targets for `A X.alt A`. [Input filtering](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/text/buffer/shaping.rs#L132-L154) · [Cluster application](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/text/buffer/shaping.rs#L207-L238)

### 11. Preserve an editable hyperbézier when the knife makes no split

**Reproduced through the geometry API.**
A closed four-point hyperbézier cut from outside to inside with one boundary crossing returned one ordinary Bézier path.
The designer gets the same number of contours but loses the editable spline representation.
The knife converts to cubics to compute hits, and the unpaired single-hit branch returns that temporary conversion.
The layer adapter uses contour identity to decide whether to replace the outline. [Hit classification](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/outline/knife.rs#L73-L86) · [Unpaired hit](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/outline/knife.rs#L145-L174) · [Edit transaction](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/babelfont/edit_effects.rs#L721-L752)

Retain the original path alongside the intersection geometry until a split or join is accepted.
Test zero, one, tangent, and two crossings plus a nested-counter join.
For a no-op, assert path type, point identities, metadata, dirty state, and undo depth.

### 12. Give glyph inventory edits an undo record

**Source trace.**
Add, Duplicate, and Remove Glyph mutate the project and refresh the font cache.
Remove also discards tabs and clears selection without recording the glyph set change in the editor’s undo history.
Removing a glyph acts across masters, so correcting a selection mistake can require reconstructing several drawings. [Inventory mutations](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/font_model.rs#L608-L665) · [Menu commands](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/editor/commands.rs#L178-L246)

Store the changed glyph and its per-source layers in an undoable inventory transaction.
Test Remove → Undo → Redo on a composite with anchors, a background layer, and two masters.
Verify restored source values and a usable selection, then save and reopen.

### 13. Identify undo entries by identity across history eviction

**Source trace.**
The engine retains 128 layer steps.
The workspace stores the layer stack’s length with each UI entry and later compares it with the current length.
Edits 128 and 129 both record depth 128.
After one Undo, the retained engine stack is 127 deep but the next UI entry still expects 128, which can make Undo unavailable from Overview. [Engine retention](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/history.rs#L83-L94) · [Workspace record](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/editor/session.rs#L3139-L3163) · [Replay check](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/editor/inspector.rs#L645-L656)

Return a stable ID for the recorded step and retire corresponding UI entries on eviction.
Apply 129 distinct edits to one glyph, switch between Editor and Overview, and verify undo and redo order through the retention boundary.

### 14. Restore the active master by source identity after an external reload

**Source trace.**
Reload remembers the active master as an array index, reopens the designspace, and selects the same numeric index.
An external reorder can silently send the next point edit to another master.
Resolve the previous source using its persisted identity or normalized source path, with an explicit fallback if it was removed.
Swap the declaration order of Regular and Bold while Bold is active, reload, and verify which UFO the next edit changes. [Reload](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/platform/host.rs#L498-L514) · [Master selection](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/font_model.rs#L568-L585)

## Shipping and using the editor at scale

### 15. Make the release script recognize the CI that actually ran

**Reproduced with the GitHub check-run response for the reviewed SHA.**
All six primary jobs passed.
Replaying that response through `required_checks` raised `Release SHA is missing passing CI: ['macos-latest', 'ubuntu-latest']`.
The matrix now publishes `minimum Rust / macos-latest` and `minimum Rust / ubuntu-latest`, while the release test repeats the script’s obsolete names.
The browser job is also absent from the script’s required set. [Release check](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/scripts/release.py#L125-L146) · [Current workflow](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/.github/workflows/ci.yml#L57-L69) · [Browser job](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/.github/workflows/ci.yml#L184-L188) · [Current test](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/scripts/test_release.py#L60-L73)

Give release validation a stable CI result covering the jobs required for that release.
Test a positive response shaped like the actual workflow and negative cases for a failed, skipped, or stale check.
Keep the Windows diagnostic visible under the project’s existing support policy. [Release validation policy](docs/RELEASING.md) · [Windows run](https://github.com/fontgarden/runebender/actions/runs/45539409904)

### 16. Virtualize glyph list rows and index kerning group membership

**Source trace.**
List mode constructs a widget row for every filtered glyph before placing the rows in the scrolling container.
Each row also asks for both kerning groups, and each lookup scans groups and their members.
Selection changes can therefore rebuild offscreen widgets and repeat a large amount of membership work. [List construction](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/view/canvas/list.rs#L61-L125) · [Membership lookup](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/canonical_metadata.rs#L274-L283)

Build rows for the visible range plus a small overscan and cache the reverse group index per source metadata revision.
Preserve the deterministic choice used for imported duplicate memberships.
Use the existing 1,000-, 4,000-, and 16,000-glyph workloads to measure row count, allocations, and selection/scroll latency. [Large-font fixtures](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/benches/large_font.rs#L20-L26)

### 17. Remove two quadratic scans from text and font opening

**Source trace.**
For every output glyph in a shaped run, the text buffer scans existing clusters and scans the run’s byte-offset table to mark coverage.
A one-glyph-per-character run already incurs quadratic work before painting, and edits can reshape the entire buffer.
Separately, the browser UFO loader linearly searches the selected files to retrieve the original bytes of each decoded GLIF. [Cluster grouping](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/text/buffer/shaping.rs#L178-L205) · [Edit-triggered reshape](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/text/buffer/shaping.rs#L338-L346) · [Original GLIF lookup](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/persistence/memory.rs#L597-L619)

Index cluster coverage by input sort and file bytes by relative path.
Keep multi-glyph clusters, absorbed characters, and original GLIF bytes intact.
Benchmark the text and file algorithms separately over geometrically increasing input sizes, including a long mixed-script line and a large UFO.

### 18. Deliver pointer cancellation and wheel units faithfully in the browser

**Reproduced at the event bridge.**
Dispatching down, move, and cancel to the page handlers produced Rust event kinds `1, 0, 2`, meaning Down, Move, Up.
For a higher-order interpolation drag, the shared widget uses Up to finish and Cancel to discard.
A browser cancellation can therefore commit a gesture.
Pass a distinct Cancel event, preserve pointer identity, and check that cancel and window blur restore the initial geometry and undo depth. [DOM handler](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/web/app.js#L250-L257) · [Rust mapping](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/browser.rs#L336-L352) · [Gesture finalization](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/view/canvas/editor/interpolation.rs#L322-L332)

**Source trace.**
The page sends wheel distances in CSS pixels, Rust tags them as physical deltas, and the canvas divides by display scale.
The browser smoke test already relaxes its assertion at higher densities.
Normalize once at the host boundary and compare the zoom factor for the same logical gesture at 1×, 1.25×, and 2×. [Wheel bridge](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/web/app.js#L260-L267) · [Canvas conversion](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/view/canvas/editor/pointer.rs#L291-L301) · [Current assertion](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/web/quality.cjs#L114-L118)

### 19. Expose the selected glyph and its actions to assistive technology

**Source trace.**
The native grid supports arrow-key selection but exposes a single canvas described by its total glyph count, with no child nodes for the glyphs.
The browser renders the shared visual plan and discards the accessibility update, leaving the HTML canvas and text input as its host elements.
A screen reader needs the selected glyph’s identity and an action to open it. [Grid keyboard and accessibility](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/view/canvas/grid.rs#L1092-L1145) · [Browser redraw](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/browser.rs#L400-L420) · [HTML host](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/web/index.html#L10-L14)

Expose stable accessible identities, names, selection, and activation for visible and focused glyphs.
Forward the tree, focus changes, and actions across the browser bridge.
Use a semantic-tree test, then have a screen-reader user find a named glyph, open it, inspect a selected point, and return to the grid.

### 20. Move optional runtime discovery off the first-frame path

**Source trace.**
Opening a native font initializes the graph subsystem.
When `font-ml` is installed, that initialization runs its task discovery synchronously with a two-second deadline.
An unresponsive helper can delay opening an ordinary UFO even when the graph panel is closed.
Cache capability discovery by executable identity and perform the probe after the window is interactive or when the graph first needs it.
With a deliberately slow helper, verify the designer can select a glyph before discovery finishes. [Opening a workspace](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/platform/host.rs#L405-L414) · [Discovery](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/editor/tools/graph/mod.rs#L179-L190) · [Graph initialization](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/editor/tools/graph/mod.rs#L234-L246)

## Background work, measurement, and maintainability

### 21. Deliver final worker output before retiring its job

**Source trace.**
Chat and voice publish events and terminal status through separate mutexes.
The UI drains events, then reads status, then discards the job if finished.
If a worker publishes its final event and success between the two reads, the final transcript or conversation update can vanish.
Send completion on the same ordered channel as the result, or take a single synchronized terminal snapshot.
Force that interleaving with a barrier and assert exactly-once delivery on success, error, and cancellation. [Voice producer and drain](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/editor/tools/voice.rs#L320-L341) · [Voice completion](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/editor/tools/voice.rs#L379-L389) · [Chat pump](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/editor/tools/chat.rs#L486-L508)

### 22. Keep a failed model load from restarting on every rebuild

**Source trace.**
The worker reports a failed model read or parse and exits.
The pump clears the job, then a subsequent render starts the same file again and clears the error.
The Model view also requests a periodic wakeup.
A broken file can trigger repeated reads and worker creation while the user tries to inspect its error.
Retain failure against the model’s path and version, wake on completion, and offer Retry or reload on file change.
With a corrupt fixture, count one load across repeated redraws and one new load after replacing the file. [Worker failure](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/platform/model.rs#L120-L133) · [Restart](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/platform/model.rs#L279-L291) · [Completion](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/platform/model.rs#L321-L337) · [View wakeup](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/view/render.rs#L482-L490)

### 23. Measure interaction latency across the complete editing operation

**Measurement gap.**
Runebender already has engine scale benchmarks, a native point-drag probe, and frame-stage instrumentation.
The native recorder summarizes a one-second window with count, mean, and maximum.
The preview pipeline also copies a font snapshot on the UI thread after a drag, even though it delays the copy during the gesture.
A fast drag-handler median cannot tell us how long the next input waits after release. [Recorder](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/platform/frame_times.rs#L53-L78) · [Frame report](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/application/platform/frame_times.rs#L146-L166) · [Snapshot scheduling](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/compiler/mod.rs#L848-L861) · [Benchmark guide](docs/BENCHMARKS.md)

Record distributions or bounded histograms for input through present, separating snapshot, shaping, scene construction, and idle time.
For each release build, open the same large family, switch master, scroll the list, edit a mark in context, release the pointer, and type again.
Report p50, p95, and p99 with hardware, display scale, and font size.
Use stable operation counts in shared CI and compare wall-clock budgets on controlled hardware.

### 24. Reduce one dependency cycle through an operation with a testable contract

**Source trace.**
The module-layer test counts four references from `font` back to `analysis`.
Layer-edit code calls the curve-analysis module to *produce edited geometry* for Harmonize, Balance, and Optimize.
That mixes measurement and mutation policy and makes the boundary harder to explain to a contributor adding the next outline command. [Dependency budget](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/tests/module_layering.rs#L70-L83) · [Layer-edit geometry call](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/font/babelfont/handle_cleanup.rs#L327-L342) · [Curve operations](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/analysis/curve.rs#L434-L481)

Move the pure geometric transforms into `outline`, keep measurement in `analysis`, and leave selection, rounding policy, and transaction ownership in the layer adapter.
Verify endpoint, tangent, and no-op behavior through existing public edit commands, then reduce the measured back-edge budget.
This is a tractable first architecture change under the existing module map. [Architecture guide](docs/ARCHITECTURE.md) · [Existing cleanup plan](docs/CLEANUP-PLAN.md)

## How the reference projects change the acceptance tests

The proposed checks below translate the seven reference projects’ published engineering practices and editing workflows into tasks a Runebender contributor can repeat.

| Reference | Concrete bar for Runebender |
| --- | --- |
| Raph Levien describes how bidi, script, shaping clusters, and hit testing operate at different boundaries. | Select the same glyph and caret in a mixed-script proof before and after a substitution, including unencoded alternates. [Text layout is a loose hierarchy of segmentation](https://raphlinus.github.io/text/2020/10/26/text-layout.html) |
| Linebender separates geometry, the widget tree, and the renderer through Kurbo, Masonry, and Vello. | Give a geometric no-op an identity-preservation test, then exercise the corresponding input and accessibility behavior at the widget boundary. [Linebender projects](https://linebender.org/) |
| Zed documents release-build frame measurements and comparisons between revisions. | Repeat a realistic editing trace on the same machine and compare distributions after a change to list construction, shaping, or preview. [Zed performance measurements](https://zed.dev/docs/development#performance-measurements) |
| Counterpunch emphasizes live OpenType shaping for complex scripts. | Drag an Arabic mark anchor and inspect a Hebrew-only substitution in the same proof, then compare live positions and substitutions with the compiled font. [Counterpunch](https://counterpunch.space/) |
| Fontra describes a browser-based, variable-first editing model. | Open a two-axis family in the browser, edit two masters, Save As to an empty folder, and continue editing after reopening that folder. [Fontra documentation](https://docs.fontra.xyz/) |
| Glyphs emphasizes editing in word context, components, and browsing large master sets. | Change a base glyph’s width in word context and verify the updated composite alongside the original native metadata after reopening in Glyphs. [Glyphs](https://glyphsapp.com/) |
| FontLab describes checking kerning in real words and refining the blend between masters. | Shape the same word at both endpoints and an intermediate location in live, variable, and static output, including class fallback and a glyph exception. [FontLab](https://www.fontlab.com/) |

## Where to start

1. Fix browser Save As and native write-back retries first.
   Both failures have small fixtures that let a test reopen the requested destination and inspect the saved data.
2. Build on those two tests to exercise delayed writes, deletions, external edits, and replacement failures.
   Keep the native exporter's staged read-back checks when adding interruption recovery.
3. Put the three kerning measurements and the Hebrew substitution in the compiler and text suites.
   They catch plausible-looking output that passes the existing build checks.
4. Repair the release check independently using the job results from the reviewed SHA as its regression fixture.
5. Use the existing large-font benchmarks and dependency checks to guide subsequent work on the glyph list, browser input, and geometry ownership.

The [November 23 review](docs/QUALITY-REVIEW-2026-11-23.md) and [cleanup plan](docs/CLEANUP-PLAN.md) contain additional lower-level work.
Check their status notes and the current implementation before scheduling an item.
For example, the primary CI failure described in the earlier review has cleared, and the text buffer now retains multiple output glyphs per cluster. [Primary CI](https://github.com/fontgarden/runebender/actions/runs/45539409976) · [Current cluster storage](https://github.com/fontgarden/runebender/blob/dfa8ace636c02d7533743ecfa72e6db1b51547dc/src/text/buffer/shaping.rs#L174-L215)

## Browser usability research · November 25, 2026

**The first release target should be a type designer finishing a session on their own font.**
They should be able to open a family, edit an outline in one master, check its spacing and marks in a word, recover from an interruption, and reopen editable sources in a fresh session.
Switching masters and checking a substitution while drawing exercise workflows highlighted by Fontra and Counterpunch. [Fontra](https://docs.fontra.xyz/) · [Counterpunch](https://counterpunch.space/)

### Published behavior at the time of review

In an isolated headless Chrome profile at 1280 × 800 and 1× density, the public editor opened the 863-glyph demo through the website’s iframe.
The public build manifest identified source `02df900` and release `0e3910c0aa18816f`, predating the implementation reviewed here.
The browser reported 7,452,082 encoded bytes and 25,576,822 decoded bytes for the public Wasm response.
The local pre-fix module was 26,487,732 bytes, served uncompressed.
These counts measure the module payload.
Startup and input latency need separate measurements. [Published editor](https://runebender.org/wasm-editor/) · [Published build manifest](https://runebender.org/app/build-info.json)

The shared widget tree already draws and edits outlines, and File → Export downloads a compiled TTF.
On a browser with directory handles, the page can open a UFO package or a folder containing one designspace and its sibling UFOs.
When the directory picker is unavailable, the current picker workflow offers no way to import the designer’s own source and download an editable copy.
IndexedDB remembers a directory handle.
The latest unsaved drawing lives in the tab. [Browser host](web/README.md) · [Folder discovery and persistence](web/app.js) · [Font export](src/application/platform/export.rs)

### Milestones and acceptance tests

1. **Let designers bring a project and take the editable source home.**
   Keep direct folder writing when supported and permitted.
   Offer directory import using a file input and relative paths, with a downloadable ZIP containing the complete source tree as the portable save path.
   Reuse the Rust package decoder and full-copy encoder for both routes.
   Show the current destination explicitly and distinguish downloading editable sources from exporting a TTF.
   Test Chrome and Edge with a writable folder, then exercise the portable route in Firefox and Safari and after permission denial.
   In a fresh session, open the saved family, compare both masters, advances, anchors, features and an untouched GLIF, then export and shape a word.
   Include a descriptor that references `masters/Regular.ufo`, which the current top-level folder scan misses.
   Resolve those referenced paths within the selected directory or explain which source is missing. [Folder reader](web/app.js) · [Directory input and relative paths](https://developer.mozilla.org/en-US/docs/Web/API/HTMLInputElement/webkitdirectory) · [Directory picker requirements](https://developer.mozilla.org/en-US/docs/Web/API/Window/showDirectoryPicker)

2. **Recover an interrupted session and preserve edits from other tools.**
   Store a revisioned recovery snapshot in origin-private storage independently of an explicit source save.
   Record which snapshot completed, keep the previous complete one during a write, and offer recovery on the next visit.
   Recheck remembered directory permissions on return and request renewed access from a user gesture.
   If storage is full or access is revoked, keep the document open and offer source download.
   Test closing the tab mid-checkpoint, finishing an old save after a newer edit, and editing a GLIF externally between open and save.
   The next session should identify the last recoverable revision, and writing over an external change should require a decision.
   Browser-managed storage can be evicted or cleared, so communicate when a user-owned copy last succeeded. [Chrome’s permission guidance](https://developer.chrome.com/docs/capabilities/web-apis/file-system-access) · [Origin-private storage](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system) · [Storage quotas and eviction](https://developer.mozilla.org/en-US/docs/Web/API/Storage_API/Storage_quotas_and_eviction_criteria)

3. **Keep the editor responsive during a real proof and export.**
   The host writes a full RGBA image to the 2D canvas on every rendered frame, and browser export presently builds the font synchronously.
   At 1280 × 800 and 2× density the pixel payload alone is 16,384,000 bytes per frame.
   Measure cold load to first usable glyph, folder open, input to next paint during a drag, and the longest main-thread task during export.
   Record device, browser, network, release, fixture, and display scale.
   Start with the two-master Core fixture and a production-sized family at 1× and 2×.
   A proposed 60 Hz target is p95 drag-to-next-paint within 16.7 ms on a named reference machine.
   Move compilation to a worker if traces show it blocking typing or cancellation, then re-run the same trace and compare the exported font. [Frame upload](web/app.js) · [Browser compiler path](src/application/platform/export.rs) · [Long-task measurement](https://web.dev/articles/optimize-long-tasks)

4. **Qualify the build users actually launch.**
   Run the folder round-trip and portable source-download checks through the packaged iframe, assert the manifest’s source revision, and repeat with a returning browser profile across an update.
   Extend the existing Chromium checks with Firefox and WebKit for the portable route.
   On shipping Safari and Chrome, verify OS file selection, IME composition in a proof, keyboard-only glyph selection, and screen-reader announcement of the selected glyph.
   The existing suite exercises synthetic composition, outline dragging, undo, and export and provides a starting point.
   The installation manifest currently launches an app that needs network access at startup.
   If offline launch becomes a release promise, cache the wrapper, loader, bindings, and Wasm as one version and verify a disconnected restart with a recoverable project. [Current browser checks](web/quality.cjs) · [CI matrix](.github/workflows/ci.yml) · [Installation and wrapper](web/README.md) · [Playwright browser coverage](https://playwright.dev/docs/browsers)
