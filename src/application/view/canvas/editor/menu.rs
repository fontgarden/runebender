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
    /// Open the context menu for a secondary press at `at`, with rows for the picture, the
    /// component and the open component under the pointer.
    pub(super) fn open_context_menu(&mut self, ctx: &mut EventCtx<'_>, at: Point) {
        let design = self.screen_to_glyph_design(at);
        let mut rows = MENU_ITEMS.to_vec();
        let on_image = self.image_frame(true).is_some_and(|(t, w, h)| {
            (t * Rect::new(0.0, 0.0, w, h).to_path(0.1))
                .bounding_box()
                .contains(design)
        });
        if on_image {
            let locked = self.session.image_locked();
            rows.splice(
                0..0,
                [
                    MenuRow {
                        label: std::borrow::Cow::Borrowed(if locked {
                            "Unlock Image"
                        } else {
                            "Lock Image"
                        }),
                        action: MenuAction::Op(|s| s.toggle_image_lock()),
                    },
                    MenuRow {
                        label: std::borrow::Cow::Borrowed("Remove Image"),
                        action: MenuAction::Op(|s| s.remove_image()),
                    },
                ],
            );
        }
        if let Some(component) = self.session.component_at(design) {
            self.session.select_component_id(component);
            self.emit(ctx, false);
            let aligned = self.session.selected_component_aligned() == Some(true);
            let mut component_rows = vec![MenuRow {
                label: std::borrow::Cow::Borrowed("Edit Component in Place"),
                action: MenuAction::EditComponent(component),
            }];
            if let Some(base) = self.session.component_reference(component) {
                component_rows.push(MenuRow {
                    label: std::borrow::Cow::Owned(format!("Open {base}")),
                    action: MenuAction::OpenBaseGlyph(base),
                });
            }
            component_rows.push(MenuRow {
                label: std::borrow::Cow::Borrowed(if aligned {
                    "Disable Automatic Alignment"
                } else {
                    "Enable Automatic Alignment"
                }),
                action: MenuAction::ToggleComponentAlignment,
            });
            rows.splice(0..0, component_rows);
        }
        if let Some(host) = &self.session.host {
            rows.insert(
                0,
                MenuRow {
                    label: std::borrow::Cow::Owned(format!("Back to {}", host.root)),
                    action: MenuAction::ExitComponent,
                },
            );
            rows.insert(
                1,
                MenuRow {
                    label: std::borrow::Cow::Owned(format!("Open {}", self.session.glyph_name)),
                    action: MenuAction::OpenBaseGlyph(self.session.glyph_name.clone()),
                },
            );
        }
        let menu = ContextMenu::new(
            ctx.widget_id(),
            MenuTarget::Editor,
            rows,
            self.palette.clone(),
            design,
        );
        let menu = NewWidget::new(menu);
        self.menu = Some(menu.id());
        // Attached, so Masonry removes the menu with this canvas. A plain layer stayed on
        // screen with no way to close it when the canvas left the tree first.
        ctx.create_attached_layer(LayerType::Other, menu, ctx.to_window(at));
    }

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
