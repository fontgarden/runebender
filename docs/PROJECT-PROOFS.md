# Check a word in the exported font

Keep a proof manifest alongside a UFO or designspace to catch changes to a word at a particular weight.
Runebender compiles the source once, shapes the requested cases from those bytes, and compares the results with the checked-in expectations.
The command runs headlessly and leaves the sources and expectations untouched.

```sh
runebender check-proof tests/fixtures/core/Core.designspace tests/fixtures/core/proofs.json
```

The [Core manifest](../tests/fixtures/core/proofs.json) checks the same pair at weight 550 with kerning enabled and disabled.
The [Arabic manifest](../tests/fixtures/arabic/proofs.json) checks a mark between two joining letters and includes a reference image.
Its reference drawings are geometric test fixtures.

## Record a case

Each entry in the JSON array has a name, the literal proof text, and an expected list of glyphs in paint order.
Each glyph entry gives the glyph name, UTF-8 byte offset into the text, horizontal advance, horizontal offset, and vertical offset.
The three distances are in font units.
Use a user-space axis tag and coordinate in `location`, for example `{"wght": 550}`.
Omitted axes take their default positions.
Unknown axes and out-of-range positions are errors.
Use `features` for tag-to-boolean overrides, `right_to_left` for an RTL run, and the optional `script` and `language` entries for an ISO 15924 script and a BCP 47 language.

Start a new case with `"expected": []`.
Run it on the approved source with `--json`, inspect the actual sequence and its rendering, and copy the reviewed placements into `expected`.
For an independent check, compile the source with `runebender compile` and inspect it with HarfBuzz.
The following command uses the same cluster offsets as the Core proof.

```sh
hb-shape --utf8-clusters --cluster-level=1 --variations=wght=550 Core.ttf AV
```

## Review changed outlines

Pass `--images DIR` to write a PNG for every case to a **new** directory.
The numbers start at one and follow manifest order.
Optional `rendering` settings set the proof dimensions, scale, margins, and baseline.
To protect an outline or mark from a visual regression even when advances and offsets stay the same, keep the reviewed PNG next to the manifest and set `reference_png` to its filename.
The check compares decoded RGBA pixels and dimensions.
When an output directory is requested, it includes both images for reference cases.
Image checks use the [project's pinned Designbot renderer](CONTRIBUTING.md#native-test-prerequisites).
Placement-only checks run without it.

```sh
runebender check-proof tests/fixtures/arabic/Arabic-Regular.ufo \
  tests/fixtures/arabic/proofs.json --images proof-output
```

## Use the result in CI

A passing manifest exits with status 0.
A different glyph, position, or reference image exits with status 4 and identifies the affected case.
Invalid input exits with status 2.
Pass `--json` to collect expected and actual placements for each case in a machine-readable report.
The repository's normal native suite runs the manifest cases, and its Linux CI job attaches the current and reference Arabic word images for review.

For lookup-level diagnosis, pass `--trace-case` followed by the exact case name.
This optional mode traces the compiled bytes with the installed HarfBuzz `hb-shape` command and records the intermediate buffers.
The lookup numbers refer to the GSUB and GPOS tables in that particular build.
The trace runs on a directional word or run with the case's settings.
