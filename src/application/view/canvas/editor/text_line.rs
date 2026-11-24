// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The text line of the edit canvas during gestures and refreshes.
//!
//! Two rules of section 5.2 in `docs/ARABIC-TEXT-LAYOUT-PLAN.md` are kept here.
//!
//! - Rule 4: a gesture shows its values in the line in the same frame.
//!   A point, anchor, sidebearing or kerning drag keeps its edit in a draft until the pointer is
//!   released.
//!   Each drag step gives the draft's layout values to [`TextState::follow`], which builds the
//!   shaping font through the same call that the commit takes.
//! - Rule 8: the view is anchored on the edited glyph.
//!   When new layout moves the active sort, the viewport moves by the same distance on the
//!   screen, so the glyph under the pointer stays where it is, in left-to-right and
//!   right-to-left lines.
//!
//! [`TextState::follow`]: crate::application::editor::tools::text::TextState::follow

use super::*;
use crate::application::editor::tools::text::{TextInputs, TextState};
use runebender::text::layout_tables::LayoutOverrides;

impl EditorWidget {
    /// The layout values of the gesture in progress that the project does not have yet.
    fn gesture_overrides(&self) -> LayoutOverrides {
        let mut overrides = LayoutOverrides::default();
        if let Some(text) = &self.text
            && let Some((layer, glyph)) = self.session.gesture_layout_values()
            && text.reads_layer(layer)
        {
            overrides.glyphs.push(glyph);
        }
        if let Drag::Kern {
            first,
            second,
            original,
            value,
            ..
        } = &self.drag
            && value != original
        {
            overrides
                .kerning
                .push((first.clone(), second.clone(), *value));
        }
        overrides
    }

    /// Shape the text line with the values of the gesture in progress, or with the committed
    /// values when no gesture is in progress, and keep the active sort in place on the screen.
    ///
    /// Every drag step and the end of every gesture call this.
    /// Returns whether the line changed.
    pub(super) fn follow_gesture(&mut self) -> bool {
        let overrides = self.gesture_overrides();
        self.change_text_anchored(|text| text.follow(&overrides))
    }

    /// End the gesture in the text line.
    ///
    /// A cancelled gesture goes back to the committed values at once.
    /// A committed one keeps showing its values until the commit hands the widget new inputs,
    /// so no frame between the release and the commit shows the values before the gesture.
    /// When the commit changes nothing, the next session from the app restores the committed
    /// values; see [`Self::follow_session`].
    pub(super) fn end_gesture(&mut self, cancelled: bool) {
        if cancelled {
            self.follow_gesture();
        }
    }

    /// After the app supplies a new session without new text inputs, shape the line with the
    /// values that session holds, so a gesture whose commit changed nothing, or was rejected,
    /// does not keep its values in the line.
    /// Returns whether the line changed.
    pub(super) fn follow_session(&mut self) -> bool {
        self.follow_gesture()
    }

    /// Give the text line inputs that the app read again, keeping what was typed, and keep
    /// the active sort in place on the screen.
    pub(super) fn refresh_text(&mut self, inputs: &TextInputs) {
        self.change_text_anchored(|text| {
            text.refresh(inputs);
            true
        });
        // A gesture that is still in progress applies its values to the new inputs.
        self.follow_gesture();
    }

    /// Run `change` on the text line, then move the viewport by the distance on the screen
    /// that the change moved the active sort, so the edited glyph does not move.
    ///
    /// Nothing moves when the change activated another sort or left none active.
    fn change_text_anchored(&mut self, change: impl FnOnce(&mut TextState) -> bool) -> bool {
        let Some(text) = self.text.as_mut() else {
            return false;
        };
        let before = text.active_placement();
        if !change(text) {
            return false;
        }
        if let (Some((index, from)), Some((after, to))) = (before, text.active_placement())
            && index == after
            && from != to
        {
            let viewport = &mut self.session.viewport;
            let shift = viewport.affine() * from - viewport.affine() * to;
            viewport.offset += shift;
        }
        true
    }
}
