// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The brush tool: a press and drag paint session ink, and the release reports it.

use super::super::*;

/// Start a brush stroke at the pointer.
pub(in super::super) fn press(editor: &mut EditorWidget, ctx: &mut EventCtx<'_>, at: Point) {
    let design = editor.screen_to_glyph_design(at);
    let mut sketch = editor
        .sketch
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if !sketch.matches(&editor.session.glyph_name, editor.sketch_source) {
        *sketch = sketch.for_glyph(
            editor.session.glyph_name.clone(),
            editor.sketch_source,
            editor.session.advance(),
        );
    }
    let changed = sketch.stroke((design.x, design.y), (design.x, design.y));
    editor.drag = Drag::Sketch {
        last: design,
        changed,
    };
    ctx.request_render();
}

/// Continue the brush stroke to the pointer.
pub(in super::super) fn drag(
    editor: &mut EditorWidget,
    ctx: &mut EventCtx<'_>,
    glyph_design: Point,
) {
    if let Drag::Sketch { last, changed } = &mut editor.drag {
        let mut sketch = editor
            .sketch
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *changed |= sketch.stroke((last.x, last.y), (glyph_design.x, glyph_design.y));
        *last = glyph_design;
        ctx.request_render();
    }
}

/// Finish the brush stroke, and report the ink when it changed.
pub(in super::super) fn release(editor: &mut EditorWidget, ctx: &mut EventCtx<'_>) {
    if let Drag::Sketch { changed, .. } = &editor.drag {
        if *changed {
            ctx.submit_action::<EditorEvent>(EditorEvent::SketchChanged);
        }
        editor.drag = Drag::None;
    }
}
