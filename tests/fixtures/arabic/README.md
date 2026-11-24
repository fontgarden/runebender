# Arabic fixture

This is an original synthetic Arabic font for the layout tests.
`generate.py` writes every file in this folder with the Python 3 standard library.
No outline, anchor or feature rule comes from an existing font.
Run `python3 tests/fixtures/arabic/generate.py` to regenerate it, and commit the UFOs that it writes.

The font has 52 glyphs and two masters, Regular and Bold, on a `wght` axis from 400 to 700.
The outlines are simple polygons; they are not a model of good Arabic letterforms.

## Contents

- Joining forms: `beh-ar`, `seen-ar`, `lam-ar`, `meem-ar`, `heh-ar` and `yeh-ar` have isolated, `.init`, `.medi` and `.fina` forms.
  `alef-ar` and `reh-ar` have isolated and `.fina` forms.
- `ccmp` decomposes `alefHamzaabove-ar` into `alef-ar` and `hamzaabove-ar`.
- `rlig` makes `lam_alef-ar` and `lam_alef-ar.fina` with `lookupflag IgnoreMarks`.
- Marks: `fatha-ar`, `damma-ar`, `kasra-ar`, `shadda-ar`, `sukun-ar`, `hamzaabove-ar` and `alefabove-ar`.
  Bases have `top` and `bottom` anchors, and marks have `_top` or `_bottom`.
  `shadda-ar` also has `top`, for mark-to-mark attachment.
- The ligature has `top_1`, `top_2`, `bottom_1` and `bottom_2`; component 1 is lam.
- `seen-ar.init`, `seen-ar.medi` and `seen-ar.fina` have `entry` and `exit` anchors.
  Each exit is 80 units above the entry, so the baseline of a seen word rises 80 units at each join toward the end of the word.
- Kerning: the group pair `public.kern1.reh` and `public.kern2.alef`, the glyph pair `reh-ar` and `meem-ar`, and the Latin group pair `public.kern1.A` and `public.kern2.V`.
- Latin and common glyphs: `A`, `T`, `V`, `a`, `n`, `o`, `zero`, `one`, `two`, `space`, `period` and `.notdef`.
- Every anchor is on the glyph itself; no glyph uses components.

`features.fea` follows the style of Amiri and ufo2ft.
It has hand-written substitution features, then `# Automatic Code` markers in `curs`, `kern`, `mark` and `mkmk`.
The `kern` feature has two hand-written lookups after its marker.
`kern_reh_fatha` adds 40 units to reh when a fatha follows it.
`kern_reh_heh` uses `lookupflag IgnoreMarks`, so it kerns reh and heh across a mark.
The `mark` feature has `kasra_below_yeh` after its marker; it lowers a kasra under yeh by 80 units.
That lookup has an effect only because it runs after the generated attachment.

## Unfinished masters

Two glyphs are deliberately unfinished, as in real sources such as Amiri.
No test string and no feature rule uses them.

- `noon-ar` does not interpolate: its first contour has four points in Regular and five points in Bold.
- `dal-ar` has a `top` anchor in Bold and no `top` anchor in Regular.

A complete fontc compile of `Arabic.designspace` fails because of these glyphs.
Tests that need a compiled font remove both glyphs from the loaded project first.

## Test strings

The values are for the Regular master.

| Text | Direction | Result |
|---|---|---|
| `بسم` | right to left | `beh-ar.init`, `seen-ar.medi`, `meem-ar.fina` |
| `لا` | right to left | `lam_alef-ar` |
| `بلا` | right to left | `beh-ar.init`, `lam_alef-ar.fina` |
| `لَاُ` | right to left | `fatha-ar` on `top_1` and `damma-ar` on `top_2` of `lam_alef-ar` |
| `لأ` | right to left | `ccmp`, then `lam_alef-ar` with `hamzaabove-ar` on `top_2` |
| `بَّ` | right to left | `shadda-ar` on `beh-ar`, `fatha-ar` on `shadda-ar` |
| `بِ` | right to left | `kasra-ar` at the `bottom` anchor of `beh-ar`, y offset -260 |
| `يِ` | right to left | `kasra-ar` 80 units lower than under beh, y offset -340 |
| `سسس` | right to left | cursive attachment: y offsets -160, -80 and 0 in logical order |
| `را` | right to left | group kerning of -60 on `reh-ar` |
| `رم` | right to left | glyph pair kerning of -40 on `reh-ar` |
| `رَا` | right to left | `kern_reh_fatha` adds 40 to the kerning of -60, which gives -20 |
| `رُه` | right to left | `kern_reh_heh` kerns -30 across the damma |
| `AV` | left to right | Latin group kerning of -70 on `A` |
