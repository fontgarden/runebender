# Proof reference fonts

Native Runebender and Virtua's live browser proof use the same `reference-fonts/` convention.
The folder contains your local choices; neither application downloads fonts or supplies default references.

## Choose references

For a font inside a Git repository, create `reference-fonts/` at the nearest Git root.
Outside Git, create it next to the font or designspace you open.
An unsaved new font has no reference folder until it is saved.
Add `/reference-fonts/` to that project's `.gitignore` before putting private or licensed fonts there.
Runebender reads the folder without editing your Git settings or font sources.

```text
my-font/
  .gitignore             # includes /reference-fonts/
  reference-fonts/
    Reference-Regular.ttf
    Reference-Bold.ttf
    AnotherFamily-VF.otf
  sources/
    MyFont.designspace
```

TTF and OTF files are discovered recursively, including uppercase extensions.
Hidden paths, symlinks, collections and webfonts are skipped.
Keep license files alongside the fonts; they do not become specimens.
A missing or empty directory means no references and no References control.
Invalid files are reported without preventing the remaining files from loading.
Each font is limited to 32 MiB.

While Proof is open, Runebender checks for changes about once a second.
Adding, replacing or removing files updates the references without reopening the font or losing unsaved edits.
Virtua's default `make proof-web` launcher does the same in the browser.

## Matching faces and weights

Faces are grouped using the typographic family name (legacy family when absent), OS/2 width class and italic/oblique flag.
The list is sorted by that identity; identical font bytes are deduplicated.
Four Plex weights therefore make one reference family, while an italic remains a separate reference.

Both implementations choose the closest supplied weight, with the lower weight winning a tie.
A variable face uses the requested `wght`, clamped to its range, and wins an otherwise exact static/variable tie.
Remaining ties use filename order.
Other reference axes stay at that reference's defaults, rather than inheriting unrelated axes from the working font.
There is no synthetic bold or italic, and a nearest-weight substitution is labeled.
References follow the proof weight in Waterfall/Text and the editor's active weight in Compare/Toggle.
An unweighted document uses 400.

## Review and limitations

Compare shows Before and Working together; Toggle changes only the revision, leaving references fixed.
References follow in pairs below the revisions, sharing an em scale and baseline within each glyph-comparison row.
The References button hides or shows them in all four modes.
Waterfall and Text remain selectable native text.
Missing coverage is shown explicitly and the affected specimen is withheld, so another font cannot masquerade as the reference.
Compare/Toggle inspect the encoded glyph's outline; unencoded alternates cannot be matched to references automatically.
Use shaped Waterfall/Text samples for contextual Arabic forms and other OpenType behavior.

The convention aligns file discovery and face selection, not rasterization across native and browser engines.
Before/Working also retain their existing sources: Runebender uses the open document and in-memory baseline; Virtua uses the review's verified snapshots.
Virtua's archived reviews preserve their original embedded reference files for reproduction.
Changing the live reference folder does not rewrite that evidence.

## Agents, including Pi

The native `review_proof` MCP tool captures the open glyph's comparison and the same project references.
Read `editor_context`, then supply the exact `expected_document_epoch`, `expected_document_revision`, `source`, and `glyph`.
It does not open the page, reset Before, switch sources, apply a proposal, or save the font.
A missing Before is labeled explicitly.
Use Compare or Toggle at an editable master; shaped text uses `proof_start` instead.
The image always includes both available revisions, uses neutral light paper, and contains up to four references per page.
Follow `next_reference_offset` to inspect more references; every page retains the same scale.
Designbot must be installed for image transport.
Local Chat uses this same capture and sends the image to its configured vision model.
Graph's existing retained compiled proof images are separate from this active-glyph comparison.

Pi reads `.pi/mcp.json` rather than the root `.mcp.json`.
With a current Runebender executable on PATH, register it from the font project's directory:

```sh
pi mcp add runebender -l --exposure direct -- runebender mcp --live
pi
```

Trust the project configuration in Pi, then use `editor_sessions` and `editor_connect` to select the intended open document.
During Cargo development, use the absolute `target/debug/runebender` path in place of `runebender` so Pi uses the build being tested.
The reference folder itself requires no agent-specific configuration.
