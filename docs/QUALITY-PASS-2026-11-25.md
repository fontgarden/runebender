# Engineering review, November 25, 2026

The largest remaining risk for a working type designer is an interrupted native save of a multi-master family.
A second priority is moving and selecting text across a change of writing direction.
Both have specific acceptance tests below.

## Reading the changes offline

1. **Follow a save.** Start at `publish`, then read `publish_one` in the same file.
   The outer function owns the list of completed replacements and the one rollback path for the whole family.
   The inner function owns the old and new versions of one file.
   At the test `publication_restores_both_sources_when_the_second_staged_file_disappears`, follow what happens to Regular when publishing Bold fails.
   Then read `ForeignDocument::save` to see how the editor keeps a working UFO and its Glyphs or Fontra original in sync. [Native publication](../src/font/persistence/mod.rs) · [Writing back to an original](../src/formats/foreign.rs)
2. **Follow a word.** Start with the two short `AV` cases and read `check` in the CLI.
   The font compiles once, each case records the actual glyphs and positions, and the terminal and JSON output come from the same `CaseReport`.
   In `script_runs`, look for the decision that keeps an inherited combining mark with its base.
   In the editor buffer, `sort_at_byte` maps a UTF-8 offset reported by the shaper back to an editable position.
   This boundary is worth close review because one character can produce several glyphs or several characters can produce a single glyph. [Proof cases](../tests/fixtures/core/proofs.json) · [Proof command](../src/application/cli/proof_check.rs) · [Script runs](../src/text/shape.rs) · [Editor buffer](../src/text/buffer/shaping.rs)
3. **Follow a browser file.** Read `desiredFiles` and `writeChanges` together.
   The first says which missing entries the editor intends to delete.
   The second compares the requested bytes with the bytes on disk and records completed writes so a retry can continue.
   Use the test that clears feature code while retaining an unrelated empty kerning table to check that ownership rule. [Browser save path](../web/app.js) · [Browser preservation test](../web/hoi-preservation.cjs)

For the rest of the checkout, the [module map](ARCHITECTURE.md) identifies each directory's owner and the normal entry point for a change.
Two older files deserve particular attention during manual review.
The Fontra adapter combines package traversal, geometry conversion, merge rules and tests in one file.
The inspector sections module combines several independent panels.
The glyph geometry codec and individual panel sections are natural boundaries for a future split when work next touches them. [Fontra adapter](../src/formats/fontra.rs) · [Inspector sections](../src/application/view/panels/sections.rs)

## Scope and standard

The starting revision was `24c6192`.
Four reviewers split the checkout by responsibility and followed edits across the editor, font engine, format adapters, command-line tools, browser and release scripts.
We selected paths where a plausible result on screen can hide a damaged source, a wrong export or a lost edit.
The repo already used Linebender's canonical lints, rustfmt configuration and an automatically checked module map.
The review used those requirements alongside behavioral tests and independent checks against saved files and a second shaping engine. [Canonical lints](https://linebender.org/wiki/canonical-lints/) · [Formatting scheme](https://linebender.org/wiki/formatting-scheme/)

## Verification

On Apple Silicon macOS the updater-enabled workspace suite passed 1,756 tests with 12 ignored, using the pinned Virtua fixtures and Designbot renderer.
The separate no-default-features engine configuration passed 887 tests with three ignored.
The native release build, warning-denied Clippy and rustdoc checks, and release WASM build passed.
All seven browser scripts exited successfully, including the portable-source checks in Chromium, Firefox and WebKit.
The release, Sparkle and browser preparation scripts passed 18 Python tests.
The final native and browser Rust advisory checks passed after moving the filesystem dependency to the patched `rustix` 1.1.6, and npm reported zero advisories for the browser lockfile. [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2026-0486)

The Gray-theme inspector, Shaping controls and master comparison were checked with headless images.
HarfBuzz reproduced the numeric Latin and Arabic proofs, including the kerning lookup that reduces A's advance from 656 to 592 at weight 550.
Native Linux and Windows execution, a physical microphone, native screen-reader behavior and production typeface quality require separate validation.

## Defects corrected

| Area | What could go wrong | Evidence to repeat |
| --- | --- | --- |
| Native source interchange | Changing an advance or moving an anchor could rewrite unrelated native fields in Glyphs and Fontra. Saving a converted working copy could also replace a source that another editor had changed. | Check the Glyphs package's nested binary asset and its pre-existing backup layer. Compare complete Fontra glyph JSON before and after changing a width and a named anchor in different layers. Modify the original during save and inspect both versions. [Glyphs](../src/formats/glyphs_export.rs) · [Fontra](../src/formats/fontra.rs) · [Save and Save As](../src/font/project/save_as.rs) |
| Browser persistence | An ordinary Save could delete an untouched empty table, overwrite an external edit or prune a file the editor did not own. A second Save could be lost while a write was pending. | Test ordinary Save, deliberate removal of features, folder conflicts, consecutive saves and draft recovery. The tests use the browser's file API. [Preservation tests](../web/hoi-preservation.cjs) · [Recovery tests](../web/recovery.cjs) |
| Text layout | One direction can contain several scripts. Sending them through a single script run could suppress a script-specific substitution or mark lookup. | The shaper tests exercise mixed scripts and global UTF-8 clusters. The live editor also offers the actual GSUB/GPOS features and language systems of the font. [Text buffer tests](../src/text/buffer/tests.rs) · [Layout options](../src/text/layout_options.rs) |
| Drawing | A single knife hit could convert a contour that it never cut. Curvature continuity diagnostics could change when a cubic was subdivided without changing its shape. | Check the contour ID and node types after the ineffective cut, and compare continuity across an uneven subdivision. The NUFO tests exercise open and all-off-curve UFO paths. [Knife](../src/outline/knife.rs) · [Curvature](../src/analysis/curve.rs) · [NUFO](../crates/nufo/src/lib.rs) |
| Anchors and masters | The inspector lacked an anchor name field, and a compatibility warning gave the designer little help locating the error. | Rename a selected anchor, check attachment offsets on transformed components, then undo. Open an incompatible glyph from Compare, select a matching start point, apply the repair and undo it. [Attachment geometry](../src/font/composites.rs) · [Inspector](../src/application/editor/inspector.rs) · [Master comparison](../src/application/editor/sources.rs) |
| Background work | A final chat or voice event could be left in the queue after completion. A graph containing a cycle could report success without running the cycle. | Interleave the terminal event with a poll and assert that the event is delivered. Submit a cyclic graph and verify the error and absence of a successful cache entry. [Chat jobs](../src/application/editor/tools/chat.rs) · [Graph runner](../src/workflows/graph_run.rs) |
| Voice capture | Stop blocked the UI for 80 ms and assumed the last microphone callback had finished. | Hold the last buffer until Stop returns, release it, and verify the capture worker passes the full buffer through to transcription and the result reaches the chat field. [Voice capture](../src/application/editor/tools/voice.rs) |
| Release verification | A renamed CI job could disqualify a good commit. Looking up checks independently could also combine results from different runs. | Match the exact commit to one completed primary workflow run, then verify the required jobs from that run. The release tests cover paginated results, incomplete runs, and a successful run for a different commit. [Release tests](../scripts/test_release.py) |

## What to address next

1. **High priority — recover an interrupted native family save.**
   Native publication renames staged sources one at a time and keeps the rollback list in memory.
   A process exit between renames can leave different masters from different saves on disk.
   Persist enough information to recover on open, and inject a process exit at every rename boundary in a temporary two-source family.
   After recovery, reopening must yield either complete revision, including its designspace and auxiliary files. [Publication and rollback](../src/font/persistence/mod.rs)
2. **High priority — align bidi caret movement with the painted text.**
   In a line such as `AאבB`, horizontal arrow movement and click placement use the paragraph direction even inside the Hebrew run.
   Generate caret stops from the visual runs, retain which side of a shared boundary the caret occupies, and use those stops for arrows, shift-selection and clicks.
   Test each visible position by keyboard and pointer in both paragraph directions. [Horizontal movement](../src/text/buffer/mod.rs) · [Hit testing](../src/text/buffer/layout.rs) · [Raph Levien on text segmentation](https://raphlinus.github.io/text/2020/10/26/text-layout.html)
3. **High priority for screen-reader use — expose individual glyphs in the grid.**
   The grid provides a canvas description with the glyph count.
   Its accessibility implementation does not identify the focused glyph, its selection or a way to open it.
   Start with accessible identities for the visible cells, then verify selecting and opening a named glyph with VoiceOver on a real window. [Grid widget](../src/application/view/canvas/grid.rs)
4. **Medium priority — preserve native fields through topology edits.**
   The new interchange tests cover advances, moving named anchors and unrelated package content.
   Rebuilding a changed contour can still lose native per-node fields, and duplicate anchor names can make ownership ambiguous.
   Add a native fixture with per-node metadata, split a segment, reopen the result in its originating editor and check the identity and metadata of every surviving node. [Glyphs export](../src/formats/glyphs_export.rs) · [Fontra merge](../src/formats/fontra.rs)

## Product implications

A designer can now check a specific word at a specific weight in the editor and keep its approved glyph positions or rendering in the font repository.
The same compiler and proof code runs when the team reviews a change.
For example, the Arabic case checks that two letters still join around a vowel mark and that the mark lands at the approved position.
A lookup trace lets an engineer inspect which compiled substitution or positioning step produced the change. [Project-owned proofs](PROJECT-PROOFS.md)

The next competitor feature I would copy is a kerning review queue built from real words.
The current proof command already supplies a regression check for a pair at a chosen weight.
A first version could show “Table”, “Taste” and “Tangent” for a selected `Ta` pair, using a word list chosen by the project.
Let the designer edit the pair or class in the active master, compare Regular, Bold and an intermediate weight, and save one approved word as a proof case. [FontLab 9](https://www.fontlab.com/)

Other useful references are Glyphs' visual master compatibility tools, Counterpunch's persistent live glyph filters and Fontra's variable-first browser workflow.
The new compatibility repair covers the selected start point of a matching closed contour.
General contour reordering, a lookup inspector in the editor and direct WOFF2 export remain opportunities for future work.
Counterpunch's saved live filters address a real task, such as finding Arabic letters with no top anchor.
Runebender deliberately retired its previous filter language and unused storage keys, so a future version should start with a designer's concrete check and its visible results. [Glyphs Handbook](https://handbook.glyphsapp.com/interpolation/outline-compatibility/) · [Counterpunch](https://counterpunch.space/) · [Fontra](https://docs.fontra.xyz/) · [Earlier filter decision](CLEANUP-PLAN.md)

A longer-term idea from Fontra is to let a designer vary an individual component independently of the surrounding glyph.
For example, two letters could reuse one component at different width settings.
Runebender's Fontra adapter preserves the native component payload in supported round trips, but leaves its location and glyph-local axes outside the editing model.
The first milestone would be opening an existing variable component, changing one local axis, undoing the change, and reopening it in Fontra. [Fontra's variable-component design](https://fontra.xyz/Fontra_ATypI_2023.pdf) · [Adapter and its documented limits](../src/formats/fontra.rs)

Interactive performance needs its own evidence.
Use the project's release-build benchmarks and compare two revisions on the same machine before making a latency claim. [Runebender benchmarks](BENCHMARKS.md) · [Zed performance measurements](https://zed.dev/docs/development#performance-measurements)
