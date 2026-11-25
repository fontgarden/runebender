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
5. For a word proof, stage one target glyph per `propose_edits` task with a unique name, a `reason` and the glyph's revision.
   For a drawing with ink, translate the whole glyph by the requested change in its left sidebearing, including components and anchors.
   Adjust the advance by the sum of the left and right sidebearing changes.
   For example, adding eight units to the left and keeping the right calls for `{"op":"translate","dx":8,"dy":0}` followed by `{"op":"adjust_width","delta":8}`.
   Omit operations with zero change and handle spaces and zero-advance marks according to the project's rules.
6. Open the target with `editor_open_glyph` and read `editor_context`.
   Call `editor_review` with `action` set to `spacing`, the proposal's `task`, the fresh `expected_context_revision` and a few real `words` containing the glyph in different contexts.
   For example, to review lowercase h, try “ahead”, “rhythm” and “which”.
7. Tell the designer which side changed and by how many font units, and ask whether to accept or reject the word proof in the editor.
   They can reply in this client.
   Bind their decision to the same source, task, document epoch and document revision returned at review.
   On accept, call `proposal_install` with `keep_structure: true` and `authorization: "user-approved"`.
   On reject, call `proposal_discard`.
   If the revision changed, open a fresh comparison for review before acting.
