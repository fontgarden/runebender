// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Capture native review geometry; MCP and chat render it outside the editor thread.

use kurbo::{Affine, Rect, Shape as _, Size};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::application::editor::tools::review::ReviewMode;
use crate::application::view::canvas::proof::{
    comparison_bounds, comparison_transforms, references,
};
use crate::application::workspace::Workspace;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    source: usize,
    glyph: String,
    expected_document_epoch: String,
    expected_document_revision: u64,
    #[serde(default)]
    reference_offset: usize,
    #[serde(default = "annotations_default")]
    include_annotations: bool,
}

fn annotations_default() -> bool {
    true
}

fn failure(message: impl ToString) -> Value {
    json!({"ok":false,"error":message.to_string(),"root_changed":false})
}

impl Workspace {
    pub(crate) fn capture_review_proof(&mut self, args: &Value) -> Value {
        let request: Request = match serde_json::from_value(args.clone()) {
            Ok(request) => request,
            Err(error) => return failure(error),
        };
        if self
            .live
            .as_ref()
            .is_none_or(|server| server.document_epoch() != request.expected_document_epoch)
            || self.font.project.document_revision() != request.expected_document_revision
        {
            return failure("Document changed; read editor_context again before capturing review");
        }
        if self
            .font
            .project
            .source_id(self.font.active())
            .map(|id| id.0)
            != Some(request.source)
            || self.session.glyph_name != request.glyph
        {
            return failure(
                "The requested source/glyph is not the active review; read editor_context",
            );
        }
        if self.session.gesture_in_progress() {
            return failure("Finish the canvas gesture before capturing review");
        }
        if !self.on_active_master()
            || !matches!(self.review.mode, ReviewMode::Compare | ReviewMode::Toggle)
        {
            return failure(
                "Review images require Compare or Toggle at an editable master; use proof_start for shaped text",
            );
        }
        if !self.untitled {
            self.review
                .reading
                .references
                .refresh(self.font.document_source());
        }
        if self.review.annotations.lock().unwrap().draft.is_some() {
            return failure("Finish the pencil stroke before capturing review");
        }
        let comparison = self.review_comparison();
        let character = self
            .font
            .glyphs
            .iter()
            .find(|glyph| glyph.name == request.glyph)
            .and_then(|glyph| glyph.codepoint);
        let cards = references::cards(self, character);
        // Include every reference in the fit so paging never changes the em scale.
        let bounds = cards.iter().filter_map(|card| card.drawing.as_ref()).fold(
            comparison_bounds(&comparison.working, comparison.before.as_ref()),
            |bounds, drawing| bounds.union(comparison_bounds(drawing, None)),
        );
        let total = cards.len();
        if request.reference_offset > total {
            return failure("reference_offset exceeds the reference count");
        }
        let weight = self.review_reference_weight();
        let identities: Vec<_> = self
            .review
            .reading
            .references
            .selected(weight)
            .into_iter()
            .skip(request.reference_offset)
            .take(4)
            .map(|face| {
                json!({"label":face.label,"weight":face.weight(weight),
                "sha256":format!("{:x}",Sha256::digest(&*face.bytes))})
            })
            .collect();
        let references: Vec<_> = cards
            .into_iter()
            .skip(request.reference_offset)
            .take(4)
            .collect();
        let next = request.reference_offset + references.len();
        let count = references.len();
        let mut cards = vec![
            references::Card {
                label: comparison.left_caption.into(),
                drawing: comparison.before,
                error: None,
            },
            references::Card {
                label: comparison.right_caption.into(),
                drawing: Some(comparison.working),
                error: None,
            },
        ];
        if cards[0].drawing.is_none() {
            cards[0].error = Some("No pinned Before for this glyph/source".into());
        }
        cards.extend(references);
        let height = 64 + u32::try_from(cards.len().div_ceil(2)).expect("six bounded cards") * 320;
        let width = 1024;
        let appearance = self.palette.proof_page(self.review.dark);
        let paper = appearance.panel.to_rgba8();
        let ink = appearance.proof_ink().to_rgba8();
        let mut paths = vec![
            json!({"d":Rect::new(0.0,0.0,f64::from(width),f64::from(height)).to_path(0.1).to_svg(),
            "color":[paper.r,paper.g,paper.b]}),
        ];
        let mut labels = vec![
            json!({"text":format!("{} · {}",request.glyph, self.font.font_info().names.family_name.as_deref().unwrap_or("Untitled")),
            "x":24,"y":height-32,"size":18}),
        ];
        let mut captions = Vec::new();
        let mut annotation_data = Vec::new();
        for (index, card) in cards.iter().enumerate() {
            let top = 64.0 + f64::from(u32::try_from(index / 2).expect("bounded row")) * 320.0;
            let (left, right) = comparison_transforms(bounds, Size::new(976.0, 256.0), true);
            let transform = if index % 2 == 0 { left } else { right };
            let transform = Affine::translate((24.0, top + 40.0)) * transform;
            // The native canvas is y-down; Designbot's data-only scenes are y-up.
            let flip = Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, f64::from(height)]);
            if let Some(drawing) = &card.drawing {
                paths.push(json!({"d":(flip * transform * &*drawing.outline).to_svg(), "color":[ink.r,ink.g,ink.b]}));
            }
            if request.include_annotations
                && let Some(drawing) = &card.drawing
            {
                let key =
                    crate::application::widgets::proof_pencil::card_key(self, drawing, &card.label);
                let ink = self.review.annotations.lock().unwrap();
                for stroke in ink.strokes.get(&key).into_iter().flatten() {
                    annotation_data.push(json!({"card":index,"drawing_key":key,"width_font_units":stroke.width,"points":stroke.points.iter().map(|p|[p.x,p.y]).collect::<Vec<_>>()}));
                    let field_x = if index % 2 == 0 { 24.0 } else { 528.0 };
                    let field = Rect::new(field_x, top + 40.0, field_x + 472.0, top + 296.0);
                    let path = flip * stroke.clipped_path(transform, field);
                    // Raster scenes carry filled paths; expand the red pencil's round stroke.
                    let outline = kurbo::stroke(
                        path.elements().iter().copied(),
                        &kurbo::Stroke::new(stroke.width * transform.as_coeffs()[0].abs()),
                        &kurbo::StrokeOpts::default(),
                        0.1,
                    );
                    paths.push(json!({"d":outline.to_svg(),"color":[230,40,45]}));
                }
            }
            let x = if index % 2 == 0 { 24.0 } else { 528.0 };
            labels.push(json!({"text":card.label,"x":x,"y":f64::from(height)-top-20.0,"size":14}));
            if let Some(error) = &card.error {
                labels.push(json!({"text":error,"x":x,"y":f64::from(height)-top-48.0,"size":12}));
            }
            let rule = Rect::new(
                x,
                f64::from(height) - top,
                x + 472.0,
                f64::from(height) - top + 1.0,
            );
            paths.push(json!({"d":rule.to_path(0.1).to_svg(),"color":[120,120,120]}));
            captions.push(json!({"label":card.label,"error":card.error}));
        }
        let scene =
            json!({"version":1,"width":width,"height":height,"paths":paths,"labels":labels});
        let scene_bytes = serde_json::to_vec(&scene).expect("data-only scene serializes");
        if scene_bytes.len() > 6 * 1024 * 1024 {
            return failure("Review geometry exceeds the live proof size limit");
        }
        json!({"ok":true,"root_changed":false,"source_id":request.source,"glyph":request.glyph,
            "document_epoch":request.expected_document_epoch,
            "captured_document_revision":request.expected_document_revision,
            "proposal_task":comparison.proposal_task,"message":comparison.message,"annotations":annotation_data,
            "reference_total":total,"reference_offset":request.reference_offset,"reference_count":count,
            "next_reference_offset":(next<total).then_some(next),"references":identities,"captions":captions,
            "scene_sha256":format!("{:x}",Sha256::digest(scene_bytes)),"scene":scene})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotated_capture_and_clean_capture_preserve_font_and_versions() {
        use crate::application::widgets::proof_pencil::{self, Stroke};
        let mut app = Workspace::untitled().unwrap();
        app.open_glyph(app.font.index_of("A").unwrap());
        app.pin_review_before();
        let revision = app.font.project.document_revision();
        let comparison = app.review_comparison();
        let key = proof_pencil::card_key(&app, &comparison.working, comparison.right_caption);
        app.review.annotations.lock().unwrap().strokes.insert(
            key,
            vec![Stroke {
                points: vec![(20.0, 30.0).into(), (300.0, 250.0).into()],
                width: 5.0,
            }],
        );
        let args = json!({"source":0,"glyph":"A","expected_document_revision":revision,"expected_document_epoch":app.live.as_ref().unwrap().document_epoch()});
        let annotated = app.capture_review_proof(&args);
        assert_eq!(annotated["ok"], true, "{annotated}");
        assert!(!annotated["annotations"].as_array().unwrap().is_empty());
        assert!(
            annotated["scene"]["paths"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["color"] == json!([230, 40, 45]))
        );
        let mut clean = args.clone();
        clean["include_annotations"] = json!(false);
        let clean = app.capture_review_proof(&clean);
        assert_eq!(clean["annotations"], json!([]));
        assert_ne!(clean["scene_sha256"], annotated["scene_sha256"]);
        if let Some(path) = std::env::var_os("RUNEBENDER_ANNOTATED_MCP_RENDER") {
            std::fs::write(
                path,
                runebender::formats::designbot::render(&annotated["scene"], false)
                    .expect("MCP scene raster"),
            )
            .unwrap();
        }
        assert_eq!(app.font.project.document_revision(), revision);
        app.review.annotations.lock().unwrap().draft = Some((
            "test".into(),
            Stroke {
                points: vec![],
                width: 5.0,
            },
        ));
        assert_eq!(app.capture_review_proof(&args)["ok"], false);
    }

    #[test]
    fn review_capture_shares_drawings_and_rejects_stale_or_wrong_context() {
        let mut app = Workspace::untitled().unwrap();
        app.open_glyph(app.font.index_of("A").unwrap());
        app.pin_review_before();
        app.review.reading.references.faces.push(
            runebender::font::reference::ReferenceFace::parse(std::sync::Arc::new(
                include_bytes!("../../../assets/fonts/VirtuaGrotesk-Regular.ttf").to_vec(),
            ))
            .unwrap(),
        );
        let revision = app.font.project.document_revision();
        let args = json!({"source":0,"glyph":"A","expected_document_revision":revision,
            "expected_document_epoch":app.live.as_ref().unwrap().document_epoch()});
        let before = app.review.show_before;
        let proof = app.capture_review_proof(&args);
        assert_eq!(proof["ok"], true, "{proof}");
        assert_eq!(proof["root_changed"], false);
        assert_eq!(proof["reference_total"], 1);
        assert_eq!(proof["captions"][0]["error"], Value::Null);
        assert_eq!(proof["captions"][1]["label"], "Working");
        assert_eq!(app.font.project.document_revision(), revision);
        assert_eq!(app.review.show_before, before);
        app.review.mode = ReviewMode::Toggle;
        app.toggle_review_revision();
        assert_eq!(
            app.capture_review_proof(&args)["scene_sha256"],
            proof["scene_sha256"]
        );
        let mut stale = args.clone();
        stale["expected_document_revision"] = json!(revision + 1);
        assert_eq!(app.capture_review_proof(&stale)["ok"], false);
        stale = args.clone();
        stale["glyph"] = json!("B");
        assert_eq!(app.capture_review_proof(&stale)["ok"], false);
        stale = args;
        stale["expected_document_epoch"] = json!("wrong");
        assert_eq!(app.capture_review_proof(&stale)["ok"], false);
    }
}
