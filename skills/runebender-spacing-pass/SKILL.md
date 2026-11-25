---
name: runebender-spacing-pass
description: Propose sidebearing changes for a group of glyphs in the open Runebender editor, proofed in a text line, as a proposal the designer installs or discards. Use when asked to space, fit or even out glyphs.
surface: runebender-live-v1
---

# Spacing pass

Glyphs: {{glyphs}}. Source id: {{source}}.

A proposal, never a direct edit. The designer installs it.

1. Connect, call `project_info`, choose the source id above, and read the family's `design_context`.
2. Choose approved controls for the target script and style.
   For Latin, read the sidebearings of H, O, n and o with `read_glyph` and compare stem measurements with `analyze_glyph`.
3. Read each target with `read_glyph` and keep its `revision`.
   Compare straight, round and open sides in context and follow the family's grid and spacing rules.
4. Inspect the original spacing in a text line.
   For Latin, use `specimen` with control strings such as `HHxHH OOxOO nnxnn oxo`.
   For other scripts, use a compiled proof with the appropriate text, direction and features.
5. Group the changes in one `propose_edits` batch with a unique `task`, a `reason` and each glyph's revision.
   For a drawing with ink, translate the whole glyph by the requested change in its left sidebearing, including components and anchors.
   Adjust the advance by the sum of the left and right sidebearing changes.
   For example, adding eight units to the left and keeping the right calls for `{"op":"translate","dx":8,"dy":0}` followed by `{"op":"adjust_width","delta":8}`.
   Omit operations with zero change and handle spaces and zero-advance marks according to the project's rules.
6. Proof the proposal layer with `proof` using `layer` set to the proposal's layer name, in the same groups as step 4. Compare.
7. Report the old and new sidebearings per glyph in a table and the rule you used. Call `proposal_install` only after the designer says to, with `authorization` set to `user-approved`.
