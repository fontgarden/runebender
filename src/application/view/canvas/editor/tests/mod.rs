// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Tests of the editor widget, grouped by subject, and the fixtures they share.

mod frame_budget;
mod metrics_card;
mod painting;
mod pen;
mod selection;
mod text;
mod tools;
mod workspace_sync;

use super::marks::{grid_dot_centers, push_grid_dot};
use super::pointer::constrain_to_45;
use super::view::dispatch_editor_event;
use super::*;
use masonry::dpi::PhysicalPosition;
use masonry::theme::default_property_set;
use masonry::ui_events::pointer::PointerState;
use masonry_testing::{PRIMARY_MOUSE, TestHarness};
use std::collections::HashSet;

fn projected_glyph(session: &Session) -> norad::Glyph {
    session
        .projected_glyph()
        .expect("an editor session has a canonical layer")
}

fn session() -> Session {
    let mut font = norad::Font::new();
    let mut glyph = norad::Glyph::new("A");
    glyph.width = 500.0;
    let mut contour = norad::Contour::default();
    for (x, y) in [(0.0, 0.0), (400.0, 0.0), (400.0, 700.0), (0.0, 700.0)] {
        contour.points.push(norad::ContourPoint::new(
            x,
            y,
            norad::PointType::Line,
            false,
            None,
            None,
        ));
    }
    glyph.contours.push(contour);
    font.default_layer_mut().insert_glyph(glyph);
    Session::new(&font, "A").expect("the glyph is there")
}

fn empty_session() -> Session {
    let mut font = norad::Font::new();
    let mut glyph = norad::Glyph::new("A");
    glyph.width = 500.0;
    font.default_layer_mut().insert_glyph(glyph);
    Session::new(&font, "A").expect("the glyph is there")
}

fn widget() -> EditorWidget {
    EditorWidget {
        hoi: None,
        hoi_drag: None,
        hoi_control: None,
        session: session(),
        palette: Arc::new(Palette::load("dark")),
        groups: (String::new(), String::new()),
        mark: None,
        tool: Tool::Select,
        sketch: Arc::new(Mutex::new(SketchLayer::new("A".into(), 0, 500.0))),
        sketch_source: 0,
        sketch_image: None,
        thickness_image: None,
        grid_dots: std::cell::RefCell::default(),
        preview_mode: false,
        ghosts: Arc::new(Vec::new()),
        interp: None,
        underlay: Underlay::default(),
        image_preview: None,
        text: None,
        text_inputs: None,
        size: Size::ZERO,
        drag: Drag::None,
        hover: None,
        hover_vertical: false,
        ruler: None,
        insert_preview: None,
        label_hover: None,
        menu: None,
        view: ViewOptions::default(),
        field: None,
        field_buf: String::new(),
        ai_box: None,
        ai_hover: None,
        ai_ghost: None,
        cursor_blink_elapsed_ns: 0,
        cursor_visible: true,
    }
}

fn composite_editor() -> EditorWidget {
    let mut font = norad::Font::new();
    font.default_layer_mut()
        .insert_glyph(projected_glyph(&session()));
    let mut composite = norad::Glyph::new("composite");
    composite.width = 500.0;
    composite.components.push(norad::Component::new(
        norad::Name::new("A").expect("valid glyph name"),
        norad::AffineTransform::default(),
        None,
    ));
    font.default_layer_mut().insert_glyph(composite);
    let mut editor = widget();
    editor.session = Session::new(&font, "composite").expect("composite resolves");
    editor.tool = Tool::Select;
    editor
}

fn double_click(harness: &mut TestHarness<EditorWidget>, at: Point) {
    harness.process_pointer_event(PointerEvent::Down(PointerButtonEvent {
        pointer: PRIMARY_MOUSE,
        button: Some(PointerButton::Primary),
        state: PointerState {
            position: PhysicalPosition::new(at.x, at.y),
            count: 2,
            ..PointerState::default()
        },
    }));
}

/// The fixture square with a `top` anchor above it.
fn anchored_widget() -> EditorWidget {
    let mut font = norad::Font::new();
    let mut glyph = projected_square();
    glyph.anchors.push(norad::Anchor::new(
        200.0,
        600.0,
        norad::Name::new("top").ok(),
        None,
        None,
    ));
    font.default_layer_mut().insert_glyph(glyph);
    let mut editor = widget();
    editor.session = Session::new(&font, "A").expect("the glyph is there");
    editor
}

fn projected_square() -> norad::Glyph {
    let mut glyph = norad::Glyph::new("A");
    glyph.width = 500.0;
    let mut contour = norad::Contour::default();
    for (x, y) in [(0.0, 0.0), (400.0, 0.0), (400.0, 700.0), (0.0, 700.0)] {
        contour.points.push(norad::ContourPoint::new(
            x,
            y,
            norad::PointType::Line,
            false,
            None,
            None,
        ));
    }
    glyph.contours.push(contour);
    glyph
}

/// Two curves meeting at a smooth point, the fourth point, at (300, 300).
fn smooth_curve_session() -> Session {
    let mut font = norad::Font::new();
    let mut glyph = norad::Glyph::new("A");
    glyph.width = 500.0;
    let mut contour = norad::Contour::default();
    for (x, y, typ, smooth) in [
        (0.0, 0.0, norad::PointType::Line, false),
        (0.0, 200.0, norad::PointType::OffCurve, false),
        (200.0, 300.0, norad::PointType::OffCurve, false),
        (300.0, 300.0, norad::PointType::Curve, true),
        (400.0, 300.0, norad::PointType::OffCurve, false),
        (500.0, 200.0, norad::PointType::OffCurve, false),
        (500.0, 0.0, norad::PointType::Curve, false),
    ] {
        contour
            .points
            .push(norad::ContourPoint::new(x, y, typ, smooth, None, None));
    }
    glyph.contours.push(contour);
    font.default_layer_mut().insert_glyph(glyph);
    Session::new(&font, "A").expect("the glyph is there")
}

fn session_with(points: &[(f64, f64, norad::PointType)]) -> Session {
    let mut font = norad::Font::new();
    let mut glyph = norad::Glyph::new("A");
    glyph.width = 600.0;
    let mut contour = norad::Contour::default();
    for &(x, y, typ) in points {
        contour
            .points
            .push(norad::ContourPoint::new(x, y, typ, false, None, None));
    }
    glyph.contours.push(contour);
    font.default_layer_mut().insert_glyph(glyph);
    Session::new(&font, "A").expect("the glyph is there")
}
