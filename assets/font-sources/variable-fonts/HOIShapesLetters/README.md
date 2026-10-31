# HOI Shapes Letters

Open [HOIShapesLetters.designspace](HOIShapesLetters.designspace) in Runebender, open `a`, and press `I`.
Choose **Regular to Bold**, then use **Edit Paths** to bend connections or **Preview** to inspect the filled glyph while moving the Progress slider.
Save experiments in this demo.

The `a` is copied byte-for-byte from Virtua Grotesk, including its advance and anchors.
Regular is the approved drawing; Bold is the existing unfinished drawing, copied as-is.
Light repeats Regular because Virtua has no Light source.
The demo maps Virtua's Regular/Bold drawings to its own weights 400/900; it does not reproduce Virtua's weight range or bracket substitutions.
The demo uses Virtua's 1024 units per em, so the copied drawings do not need scaling.
The synthetic shapes retain their original coordinates.
`box` and `dot` deliberately travel sideways; use `a` or `ring` to judge a shape changing in place.

The copies have matching two-contour topology with 51 points, including off-curves.
No HOI corrections are pre-authored for `a`.
This demo is separate from the open HOI Shapes Playground so adding letters does not overwrite its unsaved experiments.
Virtua Grotesk and the HOI Shapes automated-test baseline remain unchanged.
The copied Virtua outlines are covered by the accompanying [SIL Open Font License](OFL.txt).
