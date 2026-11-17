// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The rows of the canvas context menu and how the widget applies a choice.

use super::*;

/// Context-menu items: (label, op). Op returns whether the glyph changed.
/// The right-click menu's rows. Shared with the layer that draws them.
pub(super) const MENU_ITEMS: &[MenuRow] = &[
    MenuRow {
        label: std::borrow::Cow::Borrowed("Add Anchor"),
        action: MenuAction::AddAnchor,
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Set Start Point"),
        action: MenuAction::Op(|s| s.set_start()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Round Corners"),
        action: MenuAction::Op(|s| s.round_corners()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Reverse Contours"),
        action: MenuAction::Op(|s| s.reverse()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Remove Overlap"),
        action: MenuAction::Op(|s| s.remove_overlap()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Flip Horizontal"),
        action: MenuAction::Op(|s| s.flip_horizontal()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Flip Vertical"),
        action: MenuAction::Op(|s| s.flip_vertical()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Rotate 90"),
        action: MenuAction::Op(|s| s.rotate_90()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Duplicate"),
        action: MenuAction::Op(|s| s.duplicate()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Harmonize"),
        action: MenuAction::Op(|s| s.harmonize()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Balance"),
        action: MenuAction::Op(|s| s.balance()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Optimize"),
        action: MenuAction::Op(|s| s.optimize()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Decompose"),
        action: MenuAction::Op(|s| s.decompose()),
    },
];

impl EditorWidget {
    /// Apply a context-menu choice. Called from the menu layer, which is
    /// not a child of this widget and so cannot reach it with an action.
    pub(crate) fn apply_menu_choice(
        this: &mut masonry::core::WidgetMut<'_, Self>,
        action: MenuAction,
        at: Point,
    ) {
        let changed = match action {
            MenuAction::AddAnchor => {
                this.widget.session.add_anchor(at.x.round(), at.y.round());
                true
            }
            MenuAction::Op(op) => op(&mut this.widget.session),
            MenuAction::ToggleComponentAlignment => {
                this.ctx
                    .submit_action::<EditorEvent>(EditorEvent::ToggleComponentAlignment);
                false
            }
            MenuAction::EditComponent(id) => {
                this.ctx
                    .submit_action::<EditorEvent>(EditorEvent::EnterComponent(id));
                false
            }
            MenuAction::ExitComponent => {
                this.ctx
                    .submit_action::<EditorEvent>(EditorEvent::ExitComponent);
                false
            }
            MenuAction::OpenBaseGlyph(name) => {
                this.ctx
                    .submit_action::<EditorEvent>(EditorEvent::OpenGlyphTab(name));
                false
            }
            // Not this canvas's menu.
            MenuAction::AddNode(_) => false,
        };
        this.widget.menu = None;
        if changed {
            this.ctx.submit_action::<EditorEvent>(EditorEvent::Edited);
        }
        this.ctx.request_render();
    }

    /// The menu closed without a choice.
    pub(crate) fn forget_menu(this: &mut masonry::core::WidgetMut<'_, Self>) {
        this.widget.menu = None;
    }
}
