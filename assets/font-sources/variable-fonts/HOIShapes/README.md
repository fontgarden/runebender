# HOI Shapes

A small synthetic source font for developing and testing higher-order interpolation in Runebender.
All geometry was created for this fixture; nothing was copied from Virtua Grotesk or another production font.
The sources use ordinary interpolation and contain no HOI metadata yet, so the baseline stays independent of the new authoring schema.
They do not demonstrate an already implemented HOI editing or export workflow.

Open [HOIShapes.designspace](HOIShapes.designspace) to load all three masters.
Type `ABCD` to display the four test shapes.
The family also includes `.notdef` and `space`.

From the repository root, the shared browser workspace server can open the sources:

```sh
runebender-serve assets/font-sources/variable-fonts/HOIShapes/HOIShapes.designspace --open
```

The fixture was not opened in a foreground editor as part of its creation or Phase 1 validation.
The headless browser check now opens it through the actual browser host and verifies byte-preserving saves.

## Masters

The font uses 1000 units per em, a `Weight` axis tagged `wght`, and a default at Regular.
The axis has no remapping.
The default follows the axis default at 400; redundant legacy source-copy directives are omitted for Runebender compatibility.

| Source | Weight | Role |
| --- | --- | --- |
| Light | 100 | Lower endpoint |
| Regular | 400 | Default and shared interval boundary |
| Bold | 900 | Upper endpoint |

The two intervals have lengths 300 and 500, which helps catch smoothing that compares handles without accounting for axis distance.
All masters have compatible contour order, point order, point types, and identifiers.
Each visible glyph has advance width 1000 and shares a vertical center of 350.
These are test shapes, not production type design.

## Shapes and intended tests

| Key | Glyph | Source geometry | Intended HOI exercise |
| --- | --- | --- | --- |
| A | `box` | A 140 by 140 square with centers at x = 220, 460, 760 | Bend all four node trajectories into an arc; test selected-point locality, easing, and a join across Regular |
| B | `dot` | A cubic circle at the same x centers, with radii 75, 90, 110 | Bend an on-curve trajectory and check its connected off-curves follow while the source-defined handle lengths still vary |
| C | `ring` | Two cubic ellipses centered at x = 500; outer horizontal radii 150, 210, 260 and vertical radius 230 | Select one contour or node group and inspect the counter, winding, attached handles, and exported shape |
| D | `fixed` | The same 200 by 200 square centered at x = 500 in every master | Confirm unrelated geometry stays unchanged; deliberately select it later to test trajectories with coincident spatial endpoints |

The ring's inner horizontal radii are 100, 135, 160, and its inner vertical radii are 180, 165, 140.
Its outer contour is clockwise and its inner contour is counterclockwise.
Circles and ellipses use four cubic segments with the usual quarter-circle control factor `4 * (sqrt(2) - 1) / 3`, rounded to six decimal places.
The source files retain explicit contour and point identifiers to support correspondence tests.
There are no bracket substitutions, sparse sources, components, anchors, kerning, or feature code in this baseline.
Add targeted fixtures for those behaviors later rather than complicating this starting font.

## Master proof

This image renders the actual saved UFO contours at a consistent scale.
It shows the source drawings, not a curved-interpolation preview.

![Light, Regular, and Bold source shapes](masters.png)

## Validation and use

Created on 2026-10-29 alongside the [HOI implementation plan](../../../../docs/HOI-RESEARCH-PLAN.md).
UFO validation and compatibility checking passed through `fontmake`, and a scratch TrueType variable font compiled successfully.
Additional checks verified consistent point identities, contour topology, endpoint locations, unchanged fixed-square geometry, and compiled glyph bounds and widths at ten weights, including interior locations.
The initial fixture validation used FontTools/ufo2ft/fontmake.
Phase 1 also passed Runebender storage/rebinding tests and native/browser GLIF-preservation checks using this fixture; curved evaluation and compiled HOI output remain Phase 2 work.
The generated TTF was kept outside the repository.

For a baseline compilation check with an existing fontmake installation, run from the repository root:

```sh
fontmake -m assets/font-sources/variable-fonts/HOIShapes/HOIShapes.designspace \
  -o variable --output-path /tmp/HOIShapes-test.ttf \
  --validate-ufo --check-compatibility
```

Use temporary copies for automated mutation, save, and export tests so that this baseline stays deterministic.
Keep generated intermediate sources and future HOI authoring metadata in separate test outputs until their format and expected results are defined.
Do not use this fixture as a reason to edit Virtua's approved Regular `a`.

## Runebender Phase 1 checks

From the repository root, run the focused storage, identity, and preservation contracts:

```sh
cargo test --lib --locked font::hoi::tests -- --test-threads=1
```

The Rust tests use temporary copies or in-memory packages, including mapped-axis, sparse-source, missing-identifier, and stale-reference variants.
They do not change these baseline UFOs or Virtua.
After building and serving `web/`, use the Playwright environment described in `web/README.md` and run:

```sh
node web/hoi-preservation.cjs
```

This checks the actual browser host with the same baseline and makes no font-file writes.
