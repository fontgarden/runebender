// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Geometry regressions run against committed canonical drafts, not the historical UFO editor.
//!
//! `tests/variable_project.rs` covers identity preservation, atomic failures and persistence.
//! These small fixtures retain the historical geometric expectations independently of that suite.

use std::collections::HashSet;

use kurbo::{Affine, BezPath, Point, Rect, Shape, Vec2};
use norad::{Font, Glyph};

use crate::font::canonical_metadata::{CanonicalFontMetadata, KerningParticipant, KerningSide};
use crate::font::model::glyph_metadata::parse_codepoints;
use crate::font::project::{Project, SourceInput};
use crate::font::variable::{GlyphLayerAddress, LayerId, SourceId};
use crate::font::{LayerEditDraft, LayerPointType, LayerView, PointId};
use crate::outline::{component_ops, glyph_paths};

struct Fixture {
    project: Project,
    layer: LayerId,
}

impl Fixture {
    fn new() -> Self {
        Self::with_glyphs(&["test"])
    }

    fn with_glyphs(names: &[&str]) -> Self {
        let mut font = Font::new();
        for name in names {
            font.default_layer_mut().insert_glyph(Glyph::new(name));
        }
        let project = Project::from_source(SourceInput::from_font(font, "Geometry.ufo".into()))
            .expect("valid fixture");
        let layer = project
            .document_source(SourceId(0))
            .expect("fixture source")
            .default_layer();
        Self { project, layer }
    }

    fn edit<R>(&mut self, edit: impl FnOnce(&mut LayerEditDraft) -> R) -> R {
        self.edit_glyph("test", edit)
    }

    fn edit_glyph<R>(&mut self, glyph: &str, edit: impl FnOnce(&mut LayerEditDraft) -> R) -> R {
        let address = GlyphLayerAddress {
            glyph: glyph.into(),
            layer: self.layer.clone(),
        };
        let mut transaction = self
            .project
            .begin_document_layer_transaction(&address)
            .expect("fixture transaction");
        transaction.draft_mut().set_grid_spacing(2.0);
        let result = edit(transaction.draft_mut());
        self.project
            .commit_document_layer_transaction(transaction)
            .expect("valid geometry edit");
        result
    }

    fn view(&self) -> LayerView<'_> {
        self.project
            .document_layer("test", &self.layer)
            .expect("fixture glyph")
    }

    fn path(&self) -> BezPath {
        glyph_paths::ordinary_layer_contours_to_bezpath(self.view())
    }

    fn points(&self) -> Vec<(PointId, Point)> {
        self.view()
            .contours()
            .flat_map(|contour| contour.points().map(|point| (point.id(), point.position())))
            .collect()
    }

    fn edit_metadata(&mut self, edit: impl FnOnce(&mut CanonicalFontMetadata)) {
        let mut metadata = self.metadata().clone();
        edit(&mut metadata);
        self.project
            .edit_document_source_metadata(SourceId(0), |draft| {
                draft.set_font_metadata(metadata);
                Ok(())
            })
            .expect("valid metadata edit");
    }

    fn metadata(&self) -> &CanonicalFontMetadata {
        self.project
            .document_font_metadata(SourceId(0))
            .expect("fixture metadata")
    }
}

fn rectangle(draft: &mut LayerEditDraft, rect: Rect) -> Vec<PointId> {
    draft.add_shape_contour(rect, false).expect("rectangle").1
}

fn curved_contour(draft: &mut LayerEditDraft) -> crate::font::ContourId {
    let (contour, _) = draft.start_contour(Point::ORIGIN).expect("start");
    draft
        .append_contour_segment(contour, None, Point::new(100.0, 0.0), false)
        .expect("line");
    draft
        .append_contour_segment(
            contour,
            Some([Point::new(130.0, 40.0), Point::new(130.0, 80.0)]),
            Point::new(100.0, 120.0),
            true,
        )
        .expect("curve");
    contour
}

#[test]
fn flip_and_rotate_preserve_bbox_center() {
    let mut fixture = Fixture::new();
    fixture.edit(|draft| rectangle(draft, Rect::new(0.0, 0.0, 100.0, 60.0)));
    fixture.edit(|draft| {
        assert!(
            draft
                .transform_points(&[], Affine::scale_non_uniform(-1.0, 1.0))
                .unwrap()
        );
    });
    assert_eq!(
        fixture.path().bounding_box(),
        Rect::new(0.0, 0.0, 100.0, 60.0)
    );
    assert_eq!(fixture.points()[0].1, Point::new(100.0, 0.0));
    let before = fixture.points();
    for _ in 0..2 {
        fixture.edit(|draft| {
            assert!(
                draft
                    .transform_points(&[], Affine::rotate(std::f64::consts::FRAC_PI_2))
                    .unwrap()
            );
        });
    }
    let rounded = |points: Vec<(PointId, Point)>| {
        let mut points: Vec<_> = points
            .into_iter()
            .map(|(_, point)| glyph_paths::point_key(point.x, point.y))
            .collect();
        points.sort();
        points
    };
    assert_eq!(rounded(before), rounded(fixture.points()));

    let mut empty = Fixture::new();
    let revision = empty.project.document_revision();
    empty.edit(|draft| assert!(!draft.transform_points(&[], Affine::IDENTITY).unwrap()));
    assert_eq!(empty.project.document_revision(), revision);
}

#[test]
fn transform_only_selected_points() {
    let mut fixture = Fixture::new();
    let selected = fixture.edit(|draft| {
        let (contour, _) = draft.start_contour(Point::ORIGIN).unwrap();
        draft
            .append_contour_segment(contour, None, Point::new(100.0, 0.0), false)
            .unwrap();
        let points = draft
            .append_contour_segment(contour, None, Point::new(50.0, 80.0), false)
            .unwrap();
        draft.close_contour(contour, None).unwrap();
        points[0]
    });
    let before = fixture.points();
    fixture.edit(|draft| {
        assert!(
            draft
                .transform_points(&[selected], Affine::translate((10.0, 5.0)))
                .unwrap()
        );
    });
    for ((id, original), (after_id, after)) in before.into_iter().zip(fixture.points()) {
        assert_eq!(after_id, id);
        assert_eq!(
            after,
            if id == selected {
                Point::new(60.0, 85.0)
            } else {
                original
            }
        );
    }
}

#[test]
fn reverse_flips_winding_and_keeps_shape() {
    let mut fixture = Fixture::new();
    fixture.edit(|draft| {
        let contour = curved_contour(draft);
        draft
            .append_contour_segment(contour, None, Point::new(0.0, 120.0), false)
            .unwrap();
        draft.close_contour(contour, None).unwrap();
    });
    let before = fixture.path();
    let identities: HashSet<_> = fixture.points().into_iter().map(|(id, _)| id).collect();
    let mut oracle = fixture
        .project
        .encode_ufo_layer("test", &fixture.layer)
        .unwrap();
    assert!(super::reverse_contours(&mut oracle, &HashSet::new()));
    fixture.edit(|draft| assert!(draft.reverse_contours(&[]).unwrap()));
    let after = fixture.path();
    assert!(
        (before.area() + after.area()).abs() < 1.0,
        "winding must flip"
    );
    assert_eq!(before.bounding_box(), after.bounding_box());
    assert_eq!(
        fixture
            .points()
            .into_iter()
            .map(|(id, _)| id)
            .collect::<HashSet<_>>(),
        identities
    );
    assert!(
        fixture
            .view()
            .contours()
            .next()
            .unwrap()
            .points()
            .any(|point| point.is_smooth())
    );

    // Storage starts may differ; compare oriented segments modulo a cyclic rotation.
    let expected: Vec<_> = glyph_paths::contours_to_bezpath(&oracle)
        .segments()
        .collect();
    let actual: Vec<_> = after.segments().collect();
    assert_eq!(actual.len(), expected.len());
    assert!((0..actual.len()).any(|offset| {
        actual
            .iter()
            .enumerate()
            .all(|(index, segment)| *segment == expected[(index + offset) % expected.len()])
    }));
}

#[test]
fn boolean_subtract_cuts_a_hole_disjoint_union_merges() {
    let mut fixture = Fixture::new();
    fixture.edit(|draft| {
        rectangle(draft, Rect::new(0.0, 0.0, 100.0, 100.0));
        rectangle(draft, Rect::new(25.0, 25.0, 75.0, 75.0));
    });
    fixture.edit(|draft| {
        assert!(
            draft
                .boolean_contours(linesweeper::BinaryOp::Difference)
                .unwrap()
        );
    });
    assert_eq!(fixture.view().contours().count(), 2);
    assert!((fixture.path().area().abs() - 7_500.0).abs() < 1.0);
    assert_eq!(fixture.path().winding(Point::new(50.0, 50.0)), 0);
    assert_ne!(fixture.path().winding(Point::new(10.0, 10.0)), 0);

    let mut union = Fixture::new();
    union.edit(|draft| {
        rectangle(draft, Rect::new(0.0, 0.0, 100.0, 100.0));
        rectangle(draft, Rect::new(50.0, 0.0, 150.0, 100.0));
    });
    union.edit(|draft| {
        assert!(
            draft
                .boolean_contours(linesweeper::BinaryOp::Union)
                .unwrap()
        );
    });
    assert_eq!(union.view().contours().count(), 1);
    assert!((union.path().area().abs() - 15_000.0).abs() < 1.0);
}

#[test]
fn set_contour_start_rotates_closed_contours() {
    let mut fixture = Fixture::new();
    let ids = fixture.edit(|draft| rectangle(draft, Rect::new(0.0, 0.0, 100.0, 100.0)));
    let before = fixture.points();
    fixture.edit(|draft| assert!(draft.set_contour_start(ids[2]).unwrap()));
    let after = fixture.points();
    assert_eq!(after[0], before[2]);
    assert_eq!(after.len(), 4);
    assert!(after.contains(&before[0]));
    let revision = fixture.project.document_revision();
    fixture.edit(|draft| assert!(!draft.set_contour_start(ids[2]).unwrap()));
    assert_eq!(fixture.project.document_revision(), revision);
}

#[test]
fn round_corner_replaces_with_fillet() {
    let mut fixture = Fixture::new();
    fixture.edit(|draft| rectangle(draft, Rect::new(0.0, 0.0, 200.0, 200.0)));
    let corner = fixture
        .points()
        .into_iter()
        .find(|(_, point)| *point == Point::ORIGIN)
        .unwrap()
        .0;
    let selection = fixture.edit(|draft| {
        draft
            .round_selected_corners(&[corner])
            .unwrap()
            .expect("fillet")
    });
    let contour = fixture.view().contours().next().unwrap();
    assert_eq!(contour.points().count(), 7);
    assert_eq!(selection.len(), 2);
    let smooth: Vec<_> = contour.points().filter(|point| point.is_smooth()).collect();
    assert_eq!(smooth.len(), 2);
    for point in smooth {
        assert_ne!(point.point_type(), LayerPointType::OffCurve);
        assert!(selection.contains(&point.id()));
        let along = point.position().x.abs().max(point.position().y.abs());
        assert!((along - 32.0).abs() < 1e-6, "offset {along}");
    }
    assert_eq!(
        contour
            .points()
            .filter(|point| point.point_type() == LayerPointType::OffCurve)
            .count(),
        2
    );
    let revision = fixture.project.document_revision();
    fixture.edit(|draft| assert!(draft.round_selected_corners(&selection).unwrap().is_none()));
    assert_eq!(fixture.project.document_revision(), revision);
}

#[test]
fn duplicate_clones_selected_contours_offset() {
    let mut fixture = Fixture::new();
    let ids = fixture.edit(|draft| {
        let ids = rectangle(draft, Rect::new(0.0, 0.0, 100.0, 100.0));
        rectangle(draft, Rect::new(200.0, 0.0, 300.0, 100.0));
        ids
    });
    let before = fixture.points();
    let pasted = fixture.edit(|draft| {
        draft
            .duplicate_contours(&[ids[0]], Vec2::new(20.0, 20.0))
            .unwrap()
    });
    let contours: Vec<_> = fixture.view().contours().collect();
    assert_eq!(contours.len(), 3);
    assert_eq!(pasted.contours, [contours[2].id()]);
    assert_eq!(pasted.points.len(), 4);
    for (original, duplicate) in contours[0].points().zip(contours[2].points()) {
        assert_eq!(
            duplicate.position(),
            original.position() + Vec2::new(20.0, 20.0)
        );
        assert!(pasted.points.contains(&duplicate.id()));
        assert!(!before.iter().any(|(id, _)| *id == duplicate.id()));
    }
    assert_eq!(&fixture.points()[..before.len()], before.as_slice());
    assert!(
        fixture
            .project
            .encode_ufo_layer("test", &fixture.layer)
            .unwrap()
            .contours[2]
            .identifier()
            .is_none()
    );
    let revision = fixture.project.document_revision();
    fixture.edit(|draft| {
        assert!(
            draft
                .duplicate_contours(&[], Vec2::new(20.0, 20.0))
                .unwrap()
                .points
                .is_empty()
        );
    });
    assert_eq!(fixture.project.document_revision(), revision);
}

#[test]
fn kern_group_membership_moves_and_prunes() {
    let mut fixture = Fixture::with_glyphs(&["A"]);
    fixture.edit_metadata(|metadata| {
        assert!(
            metadata
                .set_kerning_group("A", KerningSide::First, Some("ROUND"))
                .unwrap()
        );
    });
    assert_eq!(
        fixture.metadata().kerning_group("A", KerningSide::First),
        Some("public.kern1.ROUND")
    );
    fixture.edit_metadata(|metadata| {
        assert!(
            metadata
                .set_kerning_group("A", KerningSide::First, Some("FLAT"))
                .unwrap()
        );
    });
    assert_eq!(
        fixture.metadata().kerning_group("A", KerningSide::First),
        Some("public.kern1.FLAT")
    );
    assert!(
        !fixture
            .metadata()
            .groups()
            .contains_key("public.kern1.ROUND")
    );
    fixture.edit_metadata(|metadata| {
        assert!(
            metadata
                .set_kerning_group("A", KerningSide::Second, Some("LEFTY"))
                .unwrap()
        );
        assert!(
            metadata
                .set_kerning_group("A", KerningSide::First, Some(""))
                .unwrap()
        );
    });
    assert_eq!(
        fixture.metadata().kerning_group("A", KerningSide::First),
        None
    );
    assert_eq!(
        fixture.metadata().kerning_group("A", KerningSide::Second),
        Some("public.kern2.LEFTY")
    );
}

#[test]
fn unicode_parses_the_web_forms() {
    let mut fixture = Fixture::new();
    for (text, expected) in [
        ("0041", Some('A')),
        ("U+0042", Some('B')),
        ("0x43", Some('C')),
        ("", None),
    ] {
        let parsed = parse_codepoints(text).expect("valid Unicode form");
        fixture.edit(|draft| assert!(draft.set_codepoints(parsed)));
        assert_eq!(
            fixture.view().codepoints().collect::<Vec<_>>(),
            expected.into_iter().collect::<Vec<_>>()
        );
    }
    let before = fixture.project.document_snapshot();
    assert!(parse_codepoints("zzz").is_err());
    assert_eq!(fixture.project.document_snapshot(), before);
}

#[test]
fn rename_updates_components_groups_and_kerning() {
    let mut fixture = Fixture::with_glyphs(&["A", "B", "Agrave"]);
    fixture.edit_glyph("Agrave", |draft| {
        draft.add_component("A".into(), Affine::IDENTITY).unwrap();
    });
    fixture.edit_metadata(|metadata| {
        metadata
            .set_kerning_group("A", KerningSide::First, Some("ROUND"))
            .unwrap();
        metadata
            .set_kerning_pair(
                KerningParticipant::glyph("A").unwrap(),
                KerningParticipant::glyph("B").unwrap(),
                Some(-30.0),
            )
            .unwrap();
    });
    fixture
        .project
        .rename_document_glyph("A", "A.new")
        .expect("rename");
    assert!(fixture.project.document_glyph("A").is_none());
    assert!(fixture.project.document_glyph("A.new").is_some());
    assert_eq!(
        fixture
            .project
            .document_layer("Agrave", &fixture.layer)
            .unwrap()
            .components()
            .next()
            .unwrap()
            .reference(),
        "A.new"
    );
    assert_eq!(
        fixture
            .metadata()
            .kerning_group("A.new", KerningSide::First),
        Some("public.kern1.ROUND")
    );
    assert_eq!(
        fixture.metadata().resolved_kerning("A.new", "B"),
        Some(-30.0)
    );
    let before = fixture.project.document_snapshot();
    assert!(fixture.project.rename_document_glyph("A.new", "B").is_err());
    assert!(fixture.project.rename_document_glyph("A.new", "").is_err());
    assert_eq!(fixture.project.document_snapshot(), before);
}

#[test]
fn components_hit_translate_delete() {
    let mut fixture = Fixture::with_glyphs(&["base", "comp"]);
    fixture.edit_glyph("base", |draft| {
        rectangle(draft, Rect::new(0.0, 0.0, 100.0, 100.0))
    });
    let id = fixture.edit_glyph("comp", |draft| {
        draft
            .add_component("base".into(), Affine::translate((200.0, 0.0)))
            .unwrap()
    });
    let root = fixture
        .project
        .document_layer("comp", &fixture.layer)
        .unwrap();
    let resolved = component_ops::resolved_document_components(
        root,
        |name| fixture.project.document_layer(name, &fixture.layer),
        |name| {
            fixture
                .project
                .document_layer(name, &fixture.layer)
                .into_iter()
                .collect()
        },
    )
    .unwrap();
    let hit = |point| {
        resolved
            .iter()
            .rev()
            .find(|component| component.path.winding(point) != 0)
            .map(|component| component.id)
    };
    assert_eq!(hit(Point::new(250.0, 50.0)), Some(id));
    assert_eq!(hit(Point::new(50.0, 50.0)), None);
    fixture.edit_glyph("comp", |draft| {
        let transform = draft.view().components().next().unwrap().transform();
        assert!(
            draft
                .set_component_transform(id, Affine::translate((10.0, 5.0)) * transform)
                .unwrap()
        );
    });
    let moved = fixture
        .project
        .document_layer("comp", &fixture.layer)
        .unwrap()
        .components()
        .next()
        .unwrap();
    assert_eq!(moved.id(), id);
    assert_eq!(moved.transform(), Affine::translate((210.0, 5.0)));
    fixture.edit_glyph("comp", |draft| assert!(draft.remove_component(id).unwrap()));
    assert_eq!(
        fixture
            .project
            .document_layer("comp", &fixture.layer)
            .unwrap()
            .components()
            .count(),
        0
    );
}

#[test]
fn hyper_pen_builds_closed_solver_contour() {
    let mut fixture = Fixture::new();
    fixture.edit(|draft| {
        let (contour, _) = draft.start_hyper_contour(Point::ORIGIN).unwrap();
        draft
            .append_hyper_point(contour, Point::new(200.0, 0.0), false)
            .unwrap();
        draft
            .append_hyper_point(contour, Point::new(200.0, 200.0), true)
            .unwrap();
        draft
            .append_hyper_point(contour, Point::new(0.0, 200.0), false)
            .unwrap();
        draft.close_hyper_contour(contour).unwrap();
    });
    let contour = fixture.view().contours().next().unwrap();
    assert!(contour.is_hyper());
    assert!(contour.is_closed());
    assert_eq!(contour.points().count(), 4);
    assert_eq!(
        contour.points().nth(2).unwrap().point_type(),
        LayerPointType::Line
    );
    assert_eq!(
        contour.points().nth(1).unwrap().point_type(),
        LayerPointType::Curve
    );
    let path = fixture
        .project
        .document_layer_path(&GlyphLayerAddress {
            glyph: "test".into(),
            layer: fixture.layer.clone(),
        })
        .unwrap();
    let curves = path
        .elements()
        .iter()
        .filter(|element| matches!(element, kurbo::PathEl::CurveTo(..)))
        .count();
    assert!(curves >= 2, "solver should emit curves, got {curves}");

    let projected = fixture
        .project
        .encode_ufo_layer("test", &fixture.layer)
        .unwrap();
    let workspace = crate::outline::path::hyper_model::Contour::from_norad(&projected.contours[0]);
    assert!(crate::outline::path::hyper_model::norad_contour_is_hyper(
        &workspace.to_norad()
    ));
}

#[test]
fn pen_builds_closed_contour() {
    let mut fixture = Fixture::new();
    fixture.edit(|draft| {
        let contour = curved_contour(draft);
        draft.close_contour(contour, None).unwrap();
    });
    let contour = fixture.view().contours().next().unwrap();
    assert!(contour.is_closed());
    assert_eq!(contour.points().count(), 5);
    assert!(contour.points().nth(4).unwrap().is_smooth());
    assert_eq!(
        contour.points().nth(4).unwrap().position(),
        Point::new(100.0, 120.0)
    );
}

#[test]
fn delete_offcurve_makes_line_delete_oncurve_drops_segment() {
    let mut fixture = Fixture::new();
    fixture.edit(|draft| {
        let (contour, _) = draft.start_contour(Point::ORIGIN).unwrap();
        draft
            .append_contour_segment(contour, None, Point::new(100.0, 0.0), false)
            .unwrap();
        draft
            .append_contour_segment(contour, None, Point::new(100.0, 100.0), false)
            .unwrap();
        draft
            .append_contour_segment(
                contour,
                Some([Point::new(80.0, 130.0), Point::new(20.0, 130.0)]),
                Point::new(0.0, 100.0),
                true,
            )
            .unwrap();
        draft.close_contour(contour, None).unwrap();
    });
    let contour = fixture.view().contours().next().unwrap();
    assert_eq!(contour.points().count(), 6);
    let offcurve = contour
        .points()
        .find(|point| point.point_type() == LayerPointType::OffCurve)
        .unwrap()
        .id();
    fixture.edit(|draft| assert!(draft.delete_points(&[offcurve]).unwrap()));
    let contour = fixture.view().contours().next().unwrap();
    assert_eq!(contour.points().count(), 4);
    assert!(contour.is_closed());
    assert!(
        contour
            .points()
            .all(|point| point.point_type() == LayerPointType::Line)
    );
    let corner = contour
        .points()
        .find(|point| point.position() == Point::new(100.0, 0.0))
        .unwrap()
        .id();
    fixture.edit(|draft| assert!(draft.delete_points(&[corner]).unwrap()));
    let contour = fixture.view().contours().next().unwrap();
    assert_eq!(contour.points().count(), 3);
    assert!(contour.is_closed());
    assert!(contour.points().all(|point| point.id() != corner));
}

#[test]
fn overlap_union_two_squares() {
    let mut fixture = Fixture::new();
    fixture.edit(|draft| {
        rectangle(draft, Rect::new(0.0, 0.0, 100.0, 100.0));
        rectangle(draft, Rect::new(50.0, 50.0, 150.0, 150.0));
    });
    fixture.edit(|draft| assert!(draft.remove_overlap().unwrap()));
    assert_eq!(fixture.view().contours().count(), 1);
    let area = fixture.path().area().abs();
    assert!((area - 17_500.0).abs() < 100.0, "area {area}");
}

#[test]
fn smooth_constraint_rotates_sibling() {
    let mut fixture = Fixture::new();
    let (incoming, outgoing) = fixture.edit(|draft| {
        let (contour, _) = draft.start_contour(Point::ORIGIN).unwrap();
        let incoming = draft
            .append_contour_segment(
                contour,
                Some([Point::new(40.0, 60.0), Point::new(60.0, 100.0)]),
                Point::new(100.0, 100.0),
                true,
            )
            .unwrap()[1];
        let outgoing = draft
            .append_contour_segment(
                contour,
                Some([Point::new(140.0, 100.0), Point::new(180.0, 60.0)]),
                Point::new(200.0, 0.0),
                false,
            )
            .unwrap()[0];
        draft.close_contour(contour, None).unwrap();
        (incoming, outgoing)
    });
    fixture.edit(|draft| {
        assert!(
            draft
                .translate_points(&[incoming], &[], Vec2::new(0.0, -20.0), false)
                .unwrap()
        );
    });
    let points = fixture.points();
    assert_eq!(
        points.iter().find(|(id, _)| *id == incoming).unwrap().1,
        Point::new(60.0, 80.0)
    );
    let out = points.iter().find(|(id, _)| *id == outgoing).unwrap().1;
    let cross = 40.0 * (out.y - 100.0) - 20.0 * (out.x - 100.0);
    assert!(cross.abs() <= 60.0, "not collinear: {cross}");
    let length = out.distance(Point::new(100.0, 100.0));
    assert!((length - 40.0).abs() < 2.0, "length changed: {length}");
}

#[test]
fn shapes_and_curve_ops() {
    let mut fixture = Fixture::new();
    fixture.edit(|draft| {
        draft
            .add_shape_contour(Rect::new(10.0, 20.0, 110.0, 220.0), true)
            .unwrap();
    });
    let contour = fixture.view().contours().next().unwrap();
    assert_eq!(contour.points().count(), 12);
    assert!(contour.is_closed());
    let oncurves: Vec<_> = contour
        .points()
        .filter(|point| point.point_type() != LayerPointType::OffCurve)
        .map(|point| (point.id(), point.position()))
        .collect();
    fixture.edit(|draft| {
        draft.balance_handles(&[]).unwrap();
    });
    let after = fixture.points();
    for point in oncurves {
        assert!(
            after.contains(&point),
            "balancing moved an ellipse endpoint"
        );
    }
}

#[test]
fn independent_edit_contexts_snap_without_changing_document_policy() {
    let mut fixture = Fixture::new();
    let points = fixture.edit(|draft| rectangle(draft, Rect::new(0.0, 0.0, 100.0, 100.0)));
    let address = GlyphLayerAddress {
        glyph: "test".into(),
        layer: fixture.layer.clone(),
    };
    let mut fine = fixture
        .project
        .begin_document_layer_transaction(&address)
        .unwrap();
    let mut coarse = fixture
        .project
        .begin_document_layer_transaction(&address)
        .unwrap();
    fine.draft_mut().set_grid_spacing(1.0);
    coarse.draft_mut().set_grid_spacing(8.0);
    for invalid in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(!coarse.draft_mut().set_grid_spacing(invalid));
    }
    fine.draft_mut()
        .translate_points(&points[..1], &[], Vec2::new(6.0, 6.0), false)
        .unwrap();
    coarse
        .draft_mut()
        .translate_points(&points[..1], &[], Vec2::new(6.0, 6.0), false)
        .unwrap();
    assert_eq!(
        coarse
            .draft_mut()
            .view()
            .contours()
            .next()
            .unwrap()
            .points()
            .next()
            .unwrap()
            .position(),
        Point::new(8.0, 8.0),
    );
    fixture
        .project
        .commit_document_layer_transaction(fine)
        .unwrap();
    assert_eq!(fixture.points()[0].1, Point::new(6.0, 6.0));
    // The other context has its own policy and geometry, but must not overwrite a newer edit.
    assert!(
        fixture
            .project
            .commit_document_layer_transaction(coarse)
            .is_err()
    );
    let revision = fixture.project.document_revision();
    fixture.edit(|draft| {
        draft.set_grid_spacing(8.0);
    });
    assert_eq!(fixture.project.document_revision(), revision);
}
