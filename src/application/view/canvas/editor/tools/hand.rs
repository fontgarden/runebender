// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The hand tool, and the pan that a press of another button starts with any tool.

use super::super::*;

/// Start a pan at the pointer.
pub(in super::super) fn press(editor: &mut EditorWidget, at: Point) {
    editor.drag = Drag::Pan { last: at };
}

/// Pan the view by the pointer travel.
pub(in super::super) fn drag(editor: &mut EditorWidget, ctx: &mut EventCtx<'_>, at: Point) {
    if let Drag::Pan { last } = &mut editor.drag {
        let d = at - *last;
        *last = at;
        editor.session.viewport.pan(d.x, d.y);
        ctx.request_render();
    }
}

/// Finish the pan.
pub(in super::super) fn release(editor: &mut EditorWidget) {
    if let Drag::Pan { .. } = &editor.drag {
        editor.drag = Drag::None;
    }
}
