// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Session-only glyph review: a pinned Before and live Working, or an explicitly chosen proposal.
//!
//! This state owns no font mutations. Applying a proposal uses the existing guarded Project
//! operations, and the single-glyph view never silently installs an atomic multi-glyph candidate.

use std::sync::Arc;

use masonry::kurbo::BezPath;
use runebender::font::variable::SourceId;

use crate::application::workspace::Workspace;

/// How the same comparison drawings are presented.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ReviewMode {
    /// Show both versions beside one another.
    #[default]
    Compare,
    /// Show either version in the same drawing field.
    Toggle,
}

/// The geometry and spacing of one frozen or live review drawing.
#[derive(Clone, Debug)]
pub(crate) struct ReviewDrawing {
    pub(crate) outline: Arc<BezPath>,
    pub(crate) advance: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReviewIdentity {
    document: u64,
    source: SourceId,
    glyph: String,
}

#[derive(Clone, Debug)]
struct ReviewBaseline {
    identity: ReviewIdentity,
    drawing: ReviewDrawing,
}

/// View preferences and one bounded, in-memory Before snapshot.
#[derive(Debug, Default)]
pub(crate) struct ReviewState {
    pub(crate) mode: ReviewMode,
    /// Proof-only appearance; the surrounding editor retains its selected theme.
    pub(crate) dark: bool,
    pub(crate) show_before: bool,
    pub(crate) chooser_open: bool,
    baseline: Option<ReviewBaseline>,
}

/// One coherent comparison, including the scope that its Apply action can change.
pub(crate) struct ReviewComparison {
    pub(crate) glyph: String,
    pub(crate) before: Option<ReviewDrawing>,
    pub(crate) working: ReviewDrawing,
    pub(crate) left_caption: &'static str,
    pub(crate) right_caption: &'static str,
    pub(crate) proposal_task: Option<String>,
    pub(crate) proposal_glyph_count: usize,
    pub(crate) can_apply_glyph: bool,
    pub(crate) message: Option<String>,
}

impl Workspace {
    fn review_identity(&self) -> Option<ReviewIdentity> {
        Some(ReviewIdentity {
            document: self.document_id,
            source: self.font.project.source_id(self.font.active())?,
            glyph: self.session.glyph_name.clone(),
        })
    }

    fn review_working_drawing(&self) -> ReviewDrawing {
        let mut outline = (*self.session.outline_arc()).clone();
        outline.extend((*self.session.components_arc()).clone());
        ReviewDrawing {
            outline: Arc::new(outline),
            advance: self.session.advance(),
        }
    }

    fn capture_review_before(&self) -> Option<ReviewBaseline> {
        if !self.on_active_master() {
            return None;
        }
        Some(ReviewBaseline {
            identity: self.review_identity()?,
            drawing: self.review_working_drawing(),
        })
    }

    /// Open review with an initial Before, or return to editing without losing that snapshot.
    pub(crate) fn toggle_proof_view(&mut self) {
        self.proof_view = !self.proof_view;
        if self.proof_view && self.review.baseline.is_none() {
            self.review.baseline = self.capture_review_before();
        }
    }

    /// Pin this glyph's current drawing as Before and compare it with subsequent edits.
    pub(crate) fn pin_review_before(&mut self) {
        let Some(baseline) = self.capture_review_before() else {
            self.note = "Return to the active master before setting Before".into();
            return;
        };
        self.review.baseline = Some(baseline);
        self.review.show_before = false;
        self.review.chooser_open = false;
        self.ai.preview_task = None;
        self.note = format!("Before pinned for {}", self.session.glyph_name);
    }

    /// Choose a proposal by identity, or return to the pinned Before and Working comparison.
    pub(crate) fn select_review_proposal(&mut self, task: Option<String>) {
        self.review.chooser_open = false;
        if let Some(task) = &task
            && !self.ai.proposals.iter().any(|proposal| {
                proposal.task == *task && proposal.glyphs.contains(&self.session.glyph_name)
            })
        {
            self.note = "That proposal does not include this glyph in the active source".into();
            return;
        }
        self.ai.preview_task = task;
        self.review.show_before = false;
    }

    /// Describe the selected review without changing the font or replacing its pinned baseline.
    pub(crate) fn review_comparison(&self) -> ReviewComparison {
        let mut result = ReviewComparison {
            glyph: self.session.glyph_name.clone(),
            before: None,
            working: self.review_working_drawing(),
            left_caption: "Before",
            right_caption: "Working",
            proposal_task: None,
            proposal_glyph_count: 0,
            can_apply_glyph: false,
            message: None,
        };
        // Checkpoint one reviews editable masters only. Do not disguise a master drawing as
        // an interpolated instance or compare its proposal with another axis location.
        if !self.on_active_master() {
            result.working = ReviewDrawing {
                outline: Arc::new(BezPath::new()),
                advance: 0.0,
            };
            result.message = Some("Return to the active master to compare drawings".into());
            return result;
        }
        if let Some(task) = &self.ai.preview_task {
            let summary =
                self.ai.proposals.iter().find(|proposal| {
                    proposal.task == *task && proposal.glyphs.contains(&result.glyph)
                });
            let Some((summary, outline)) =
                summary.zip(self.model_proposal_outline(task, &result.glyph))
            else {
                result.message = Some(
                    "The selected proposal is unavailable for this glyph or has become stale"
                        .into(),
                );
                return result;
            };
            let advance = self
                .model_proposal_advance(task, &result.glyph)
                .unwrap_or(result.working.advance);
            result.before = Some(result.working);
            result.working = ReviewDrawing {
                outline: Arc::new(outline),
                advance,
            };
            result.left_caption = "Working";
            result.right_caption = "Proposal";
            result.proposal_task = Some(task.clone());
            result.proposal_glyph_count = summary.glyphs.len();
            match self.review_proposal_eligibility(task, &result.glyph) {
                Ok(()) => result.can_apply_glyph = true,
                Err(message) => result.message = Some(message),
            }
            return result;
        }
        if let Some(baseline) = &self.review.baseline
            && Some(&baseline.identity) == self.review_identity().as_ref()
        {
            result.before = Some(baseline.drawing.clone());
        } else {
            result.message = Some("Set Before to start comparing this glyph's edits".into());
        }
        result
    }

    /// Apply only the glyph actually displayed, retaining its pre-apply drawing for comparison.
    pub(crate) fn apply_review_glyph(&mut self) {
        let comparison = self.review_comparison();
        if !comparison.can_apply_glyph {
            self.note = comparison
                .message
                .unwrap_or_else(|| "Choose a proposal for this glyph before applying".into());
            return;
        }
        let Some(task) = comparison.proposal_task else {
            return;
        };
        let before = self.capture_review_before();
        let revision = self.font.project.document_revision();
        self.install_proposal(&task, Some(vec![comparison.glyph]));
        if self.font.project.document_revision() != revision {
            self.review.baseline = before;
            self.review.show_before = false;
        } else if self
            .ai
            .proposals
            .iter()
            .any(|proposal| proposal.task == task)
        {
            // The installer rechecks eligibility. A failed attempt must not erase the earlier
            // pinned Before or dismiss the proposal the designer was inspecting.
            self.ai.preview_task = Some(task);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> Workspace {
        let mut app = Workspace::untitled().expect("a new font opens");
        app.proof_view = false;
        let index = app.font.index_of("A").expect("Latin Core contains A");
        app.open_glyph(index);
        app
    }

    #[test]
    fn before_stays_pinned_across_edits_and_reopening() {
        let mut app = workspace();
        let original = app.session.advance();
        app.toggle_proof_view();
        Arc::make_mut(&mut app.session).set_advance(original + 80.0);
        let comparison = app.review_comparison();
        assert_eq!(comparison.before.unwrap().advance, original);
        assert_eq!(comparison.working.advance, original + 80.0);
        app.toggle_proof_view();
        app.toggle_proof_view();
        assert_eq!(app.review_comparison().before.unwrap().advance, original);
        app.pin_review_before();
        assert_eq!(
            app.review_comparison().before.unwrap().advance,
            original + 80.0
        );
    }

    #[test]
    fn before_never_crosses_glyph_source_or_document_identity() {
        let mut app = workspace();
        app.pin_review_before();
        let original = app.review.baseline.clone().unwrap();
        let next = app.font.index_of("B").unwrap();
        app.open_glyph(next);
        assert!(app.review_comparison().before.is_none());
        app.open_glyph(app.font.index_of("A").unwrap());
        assert!(app.review_comparison().before.is_some());
        app.review.baseline.as_mut().unwrap().identity.source = SourceId(usize::MAX);
        assert!(app.review_comparison().before.is_none());
        app.review.baseline = Some(original);
        app.document_id += 1;
        assert!(app.review_comparison().before.is_none());
    }

    #[cfg(unix)]
    fn propose(app: &mut Workspace, task: &str, glyphs: &[&str], delta: f64) {
        use runebender::automation::agent::ToolCall;
        let source = app.font.project.source_id(app.font.active()).unwrap();
        let edits: Vec<_> = glyphs
            .iter()
            .map(|glyph| {
                let read = app.call_live(&ToolCall {
                    name: "read_glyph".into(),
                    arguments: serde_json::json!({"glyph": glyph, "source": source.0}),
                });
                serde_json::json!({"glyph": glyph, "expected_revision": read["revision"],
                "operations": [{"op": "adjust_width", "delta": delta}]})
            })
            .collect();
        let reply = app.call_live(&ToolCall {
            name: "propose_edits".into(),
            arguments: serde_json::json!({"source": source.0, "task": task,
                "reason": "proof review regression test", "edits": edits}),
        });
        assert_eq!(reply["ok"], true, "{reply}");
        app.refresh_proposals();
    }

    #[cfg(unix)]
    #[test]
    fn comparison_uses_only_the_explicitly_selected_proposal() {
        let mut app = workspace();
        let original = app.session.advance();
        app.pin_review_before();
        propose(&mut app, "first", &["A"], 20.0);
        propose(&mut app, "second", &["A"], 60.0);
        app.ai.preview_task = None;
        assert!(app.review_comparison().proposal_task.is_none());
        app.select_review_proposal(Some("first".into()));
        let comparison = app.review_comparison();
        assert_eq!(comparison.proposal_task.as_deref(), Some("first"));
        assert_eq!(comparison.before.unwrap().advance, original);
        assert_eq!(comparison.working.advance, original + 20.0);
        app.select_review_proposal(Some("second".into()));
        assert_eq!(app.review_comparison().working.advance, original + 60.0);
    }

    #[cfg(unix)]
    #[test]
    fn stale_persisted_proposal_cannot_replace_before_or_dismiss_selection() {
        let mut app = workspace();
        let before = app.session.advance();
        app.pin_review_before();
        propose(&mut app, "stale", &["A"], 20.0);
        app.select_review_proposal(Some("stale".into()));
        let address = app.font.active_layer_address("A").unwrap();
        app.font
            .project
            .edit_document_layer(&address.glyph, &address.layer, |draft| {
                draft.set_width(before + 80.0)?;
                Ok(())
            })
            .unwrap();
        app.after_font_change(&["A".into()]);
        let comparison = app.review_comparison();
        assert!(!comparison.can_apply_glyph);
        assert!(comparison.message.unwrap().contains("stale proposal"));
        let revision = app.font.project.document_revision();
        app.apply_review_glyph();
        assert_eq!(app.font.project.document_revision(), revision);
        assert_eq!(app.ai.preview_task.as_deref(), Some("stale"));
        assert_eq!(
            app.review.baseline.as_ref().unwrap().drawing.advance,
            before
        );
        assert_eq!(app.session.advance(), before + 80.0);
        assert!(app.ai.installed_order.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn applying_a_review_changes_only_the_visible_glyph() {
        let mut app = workspace();
        let original_a = app.session.advance();
        let original_b = app.font.glyphs[app.font.index_of("B").unwrap()].advance;
        propose(&mut app, "pair", &["A", "B"], 20.0);
        app.select_review_proposal(Some("pair".into()));
        assert_eq!(app.review_comparison().proposal_glyph_count, 2);
        app.apply_review_glyph();
        assert_eq!(app.session.advance(), original_a + 20.0);
        assert_eq!(
            app.font.glyphs[app.font.index_of("B").unwrap()].advance,
            original_b
        );
        assert_eq!(app.review_comparison().before.unwrap().advance, original_a);
        assert_eq!(app.ai.proposals[0].glyphs, vec!["B"]);
    }
}
