// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Keyboard and text input handling of the editor widget.

use super::*;

impl EditorWidget {
    /// Handle a keyboard or text event.
    /// [`Widget::on_text_event`] calls this.
    pub(super) fn handle_text_event(&mut self, ctx: &mut EventCtx<'_>, event: &TextEvent) {
        if self.tool == Tool::Interpolation {
            self.interpolation_key(ctx, event);
            return;
        }
        if self.tool == Tool::Text && self.text.is_some() {
            self.cursor_blink_elapsed_ns = 0;
            self.cursor_visible = true;
            ctx.request_anim_frame();
        }
        if self.tool == Tool::Text
            && let Some(text) = self.text.as_mut()
            && let TextEvent::Ime(ime) = event
        {
            let changed = match ime {
                masonry::core::Ime::Preedit(value, _) => {
                    text.set_preedit(value.clone());
                    false
                }
                masonry::core::Ime::Commit(value) => text.commit_preedit(value),
                masonry::core::Ime::Disabled => {
                    text.set_preedit(String::new());
                    false
                }
                masonry::core::Ime::Enabled => false,
            };
            if changed {
                self.emit_text_changed(ctx);
            }
            // Composition can change the visible run even when the commit
            // contains no glyph the document can place. Refit after every
            // IME transition so cancelling or rejecting a preedit cannot
            // leave the viewport sized for stale composition text.
            self.fit_text();
            ctx.request_render();
            ctx.set_handled();
            return;
        }
        if self.tool == Tool::Text
            && let Some(text) = self.text.as_mut()
            && let TextEvent::ClipboardPaste(value) = event
        {
            if text.commit_preedit(value) {
                self.fit_text();
                ctx.request_render();
                self.emit_text_changed(ctx);
            }
            ctx.set_handled();
            return;
        }
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        if self.tool == Tool::AiShape
            && self.ai_box.is_some()
            && !(key.modifiers.meta() || key.modifiers.ctrl())
            && self.ai_key(ctx, key)
        {
            ctx.request_render();
            ctx.set_handled();
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

        if self.tool == Tool::Text
            && let Some(text) = self.text.as_mut()
            && cmd
            && let Key::Character(character) = &key.key
        {
            if character.eq_ignore_ascii_case("c") || character.eq_ignore_ascii_case("x") {
                if let Some(selected) = text.buffer.selected_text().filter(|text| !text.is_empty())
                {
                    ctx.set_clipboard(selected);
                    if character.eq_ignore_ascii_case("x") {
                        text.buffer.delete_after_cursor();
                        text.buffer.shape_arabic_if_rtl();
                        self.fit_text();
                        ctx.request_render();
                        self.emit_text_changed(ctx);
                    }
                }
                ctx.set_handled();
                return;
            }
            if character.eq_ignore_ascii_case("a") {
                text.buffer.select_range(0, text.buffer.len());
                self.emit_text_proof_selection(ctx);
                ctx.request_render();
                ctx.set_handled();
                return;
            }
        }

        if self.tool == Tool::Select
            && self.session.image_selected
            && self.session.selection.is_empty()
            && self.field.is_none()
            && !cmd
            && let Some(transform) = self.session.layer_image().map(|image| image.transform())
        {
            let nudge = |dx: f64, dy: f64| Affine::translate((dx, dy)) * transform;
            let locked = self.session.image_locked();
            let (handled, edited) = match &key.key {
                Key::Named(NamedKey::Escape) => {
                    self.session.set_image_selected(false);
                    (true, false)
                }
                // A locked picture can be selected and nothing more.
                _ if locked => (false, false),
                Key::Named(NamedKey::ArrowLeft) => {
                    (true, self.session.set_image_transform(nudge(-step, 0.0)))
                }
                Key::Named(NamedKey::ArrowRight) => {
                    (true, self.session.set_image_transform(nudge(step, 0.0)))
                }
                Key::Named(NamedKey::ArrowUp) => {
                    (true, self.session.set_image_transform(nudge(0.0, step)))
                }
                Key::Named(NamedKey::ArrowDown) => {
                    (true, self.session.set_image_transform(nudge(0.0, -step)))
                }
                Key::Named(NamedKey::Backspace | NamedKey::Delete) => {
                    (true, self.session.remove_image())
                }
                _ => (false, false),
            };
            if handled {
                self.emit(ctx, edited);
                ctx.request_render();
                ctx.set_handled();
                return;
            }
        }

        if self.tool == Tool::Label && self.field.is_none() && !cmd {
            let mut edited = false;
            let handled = match &key.key {
                Key::Named(NamedKey::Tab) if shift => {
                    self.session.step_label_letter(-1);
                    true
                }
                Key::Named(NamedKey::Tab | NamedKey::Enter) => {
                    self.session.next_label_letter();
                    true
                }
                Key::Named(NamedKey::Backspace | NamedKey::Delete) => {
                    edited = self.session.delete_label_selection();
                    true
                }
                Key::Named(
                    arrow @ (NamedKey::ArrowLeft
                    | NamedKey::ArrowRight
                    | NamedKey::ArrowUp
                    | NamedKey::ArrowDown),
                ) if self.session.label.selected.is_some() => {
                    // One grid step, or eight with Shift.
                    use crate::application::editor::tools::label::LABEL_GRID;
                    let step = LABEL_GRID * if shift { 8.0 } else { 1.0 };
                    let by = match arrow {
                        NamedKey::ArrowLeft => kurbo::Vec2::new(-step, 0.0),
                        NamedKey::ArrowRight => kurbo::Vec2::new(step, 0.0),
                        NamedKey::ArrowUp => kurbo::Vec2::new(0.0, step),
                        _ => kurbo::Vec2::new(0.0, -step),
                    };
                    edited = self.session.nudge_label_selection(by);
                    true
                }
                Key::Named(NamedKey::Escape) if self.session.label.selected.is_some() => {
                    self.session.clear_label_selection();
                    true
                }
                Key::Named(NamedKey::Escape) if self.session.label.sample.is_some() => {
                    self.session.select_sample(None);
                    true
                }
                _ => false,
            };
            if handled {
                self.emit(ctx, edited);
                ctx.request_render();
                ctx.set_handled();
                return;
            }
        }

        let edits_metaballs = self.tool == Tool::Metaball
            || (self.tool == Tool::Select
                && !self.session.metaballs.selected.is_empty()
                && !self.session.has_selection()
                && self.session.selected_component.is_none());
        if edits_metaballs && self.field.is_none() {
            let mut edited = false;
            let handled = match &key.key {
                Key::Character(c) if cmd && c.eq_ignore_ascii_case("a") => {
                    self.session.select_all_metaballs();
                    true
                }
                Key::Named(NamedKey::Backspace | NamedKey::Delete) => {
                    edited = self.session.delete_metaballs();
                    true
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    edited = self
                        .session
                        .move_metaballs(kurbo::Vec2::new(-step, 0.0), false);
                    true
                }
                Key::Named(NamedKey::ArrowRight) => {
                    edited = self
                        .session
                        .move_metaballs(kurbo::Vec2::new(step, 0.0), false);
                    true
                }
                Key::Named(NamedKey::ArrowUp) => {
                    edited = self
                        .session
                        .move_metaballs(kurbo::Vec2::new(0.0, step), false);
                    true
                }
                Key::Named(NamedKey::ArrowDown) => {
                    edited = self
                        .session
                        .move_metaballs(kurbo::Vec2::new(0.0, -step), false);
                    true
                }
                Key::Named(NamedKey::Escape) => {
                    self.session.clear_metaball_selection();
                    true
                }
                _ => false,
            };
            if handled {
                self.emit(ctx, edited);
                ctx.request_render();
                ctx.set_handled();
                return;
            }
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

        // The text tool types. Everything a key would otherwise do to
        // the outline is off while it is in hand, because a person
        // typing "n" means the letter, not the pen.
        if self.tool == Tool::Text
            && let Some(text) = self.text.as_mut()
            && !cmd
        {
            let (handled, changed_text) = match &key.key {
                // Masonry, GPUI, and Web all deliver ordinary typing as a
                // logical character key. IME commits remain a separate path
                // for composed text.
                Key::Character(value) => {
                    let value: String = value.chars().filter(|c| !c.is_control()).collect();
                    let changed = !value.is_empty() && text.commit_preedit(&value);
                    (true, changed)
                }
                Key::Named(NamedKey::Backspace) => {
                    let changed = text.buffer.delete_before_cursor().is_some();
                    if changed {
                        text.buffer.shape_arabic_if_rtl();
                    }
                    (changed, changed)
                }
                Key::Named(NamedKey::Delete) => {
                    let changed = text.buffer.delete_after_cursor().is_some();
                    if changed {
                        text.buffer.shape_arabic_if_rtl();
                    }
                    (changed, changed)
                }
                Key::Named(NamedKey::Enter) => {
                    text.buffer.insert_line_break();
                    (true, true)
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    if shift {
                        text.buffer.extend_selection_visual_left();
                    } else {
                        text.buffer.move_cursor_visual_left();
                    }
                    (true, false)
                }
                Key::Named(NamedKey::ArrowRight) => {
                    if shift {
                        text.buffer.extend_selection_visual_right();
                    } else {
                        text.buffer.move_cursor_visual_right();
                    }
                    (true, false)
                }
                Key::Named(NamedKey::ArrowUp) => {
                    if shift {
                        text.buffer
                            .extend_selection_vertically(-1, text.line_height);
                    } else {
                        text.buffer.move_cursor_vertically(-1, text.line_height);
                    }
                    (true, false)
                }
                Key::Named(NamedKey::ArrowDown) => {
                    if shift {
                        text.buffer.extend_selection_vertically(1, text.line_height);
                    } else {
                        text.buffer.move_cursor_vertically(1, text.line_height);
                    }
                    (true, false)
                }
                Key::Named(NamedKey::Home) => {
                    if shift {
                        text.buffer.extend_selection_to_line_edge(false);
                    } else {
                        text.buffer.move_cursor_to_line_edge(false);
                    }
                    (true, false)
                }
                Key::Named(NamedKey::End) => {
                    if shift {
                        text.buffer.extend_selection_to_line_edge(true);
                    } else {
                        text.buffer.move_cursor_to_line_edge(true);
                    }
                    (true, false)
                }
                _ => (false, false),
            };
            if handled {
                // A longer line needs more room; refit only while the
                // caret is at the end, so it does not fight a person who
                // has zoomed in on a pair.
                let at_end = text.buffer.cursor() == text.buffer.len();
                if at_end {
                    self.fit_text();
                }
                if changed_text {
                    self.emit_text_changed(ctx);
                } else {
                    self.emit_text_proof_selection(ctx);
                }
                ctx.request_render();
                ctx.set_handled();
            }
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
