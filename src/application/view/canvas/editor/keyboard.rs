// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Keyboard and text input handling of the editor widget.

use super::tools::{label, metaball, select, shapes, text};
use super::*;

impl EditorWidget {
    /// Handle a keyboard or text event.
    /// [`Widget::on_text_event`] calls this.
    pub(super) fn handle_text_event(&mut self, ctx: &mut EventCtx<'_>, event: &TextEvent) {
        if self.tool == Tool::Interpolation {
            self.interpolation_key(ctx, event);
            return;
        }
        if text::input(self, ctx, event) {
            return;
        }
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        if shapes::key(self, ctx, key) {
            return;
        }
        // Read-only instance: don't handle edit keys, but let Escape bubble to
        // the shortcut host (so it still leaves the editor).
        if self.interp.is_some() {
            return;
        }
        let cmd = key.modifiers.meta() || key.modifiers.ctrl();
        let shift = key.modifiers.shift();
        let alt = key.modifiers.alt();
        // From the config file; by default one grid step, so a nudge cannot round back to
        // where it started.
        // Command and Shift together make large moves.
        let step = crate::application::platform::config::nudge().for_modifiers(shift, cmd);

        if text::command_key(self, ctx, key, cmd) {
            return;
        }
        if select::image_key(self, ctx, key, cmd, step) {
            return;
        }
        if label::key(self, ctx, key, cmd, shift) {
            return;
        }
        if metaball::key(self, ctx, key, cmd, step) {
            return;
        }

        // A focused metric box takes the keys first. Everything below
        // this point would otherwise read a digit as a nudge or a tool.
        if self.field.is_some() && !cmd {
            let (edited, handled) = self.metric_key(key);
            if handled {
                self.emit(ctx, edited);
                ctx.set_handled();
                return;
            }
        }

        if text::typing_key(self, ctx, key, cmd, shift) {
            return;
        }
        let (edited, handled) = match &key.key {
            Key::Named(NamedKey::Escape) => {
                if self.tool == Tool::Measure && self.ruler.is_some() {
                    self.ruler = None;
                    ctx.request_render();
                    ctx.set_handled();
                } else if self.session.pen_is_active() || self.session.contour_drawing_is_active() {
                    self.session.pen_cancel();
                    self.emit(ctx, false);
                    ctx.set_handled();
                } else if self.session.host.is_some() {
                    // Inside a component, Escape climbs back out to its composite.
                    ctx.submit_action::<EditorEvent>(EditorEvent::ExitComponent);
                    ctx.set_handled();
                }
                // Otherwise do not handle: the shortcut host turns Escape into
                // "back to overview".
                return;
            }
            // Option nudges on-curve points without their handles.
            Key::Named(NamedKey::ArrowLeft) => (self.session.nudge_with(-step, 0.0, alt), true),
            Key::Named(NamedKey::ArrowRight) => (self.session.nudge_with(step, 0.0, alt), true),
            Key::Named(NamedKey::ArrowUp) => (self.session.nudge_with(0.0, step, alt), true),
            Key::Named(NamedKey::ArrowDown) => (self.session.nudge_with(0.0, -step, alt), true),
            // Tab and Shift-Tab walk the selection along its contour, as in Glyphs.
            Key::Named(NamedKey::Tab) if !cmd => {
                self.session.select_adjacent_point(!shift);
                ctx.request_render();
                (false, true)
            }
            Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace) => {
                (self.session.delete_selected(), true)
            }
            Key::Character(c) if cmd && c.eq_ignore_ascii_case("a") => {
                self.session.select_all();
                (false, true)
            }
            Key::Character(c) if cmd && !shift && c.eq_ignore_ascii_case("z") => {
                ctx.submit_action::<EditorEvent>(EditorEvent::Undo);
                ctx.set_handled();
                return;
            }
            Key::Character(c) if cmd && (c == "y" || (shift && c.eq_ignore_ascii_case("z"))) => {
                ctx.submit_action::<EditorEvent>(EditorEvent::Redo);
                ctx.set_handled();
                return;
            }
            _ => return,
        };
        if handled {
            self.emit(ctx, edited);
            ctx.set_handled();
        }
    }
}
