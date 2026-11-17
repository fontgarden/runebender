// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A small helper to draw a shaped text label into a scene, reusing
//! Masonry's `render_text` and Parley. Used for measure labels and
//! grid cell labels.
//!
//! This stays local while Masonry has no one-call equivalent for shaped
//! canvas labels.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use masonry::core::render_text;
use masonry::kurbo::{Affine, Point};
use masonry::parley::{FontContext, Layout, LayoutContext, StyleProperty};
use masonry::peniko::Brush;
use xilem::Color;

use masonry::core::BrushIndex;

/// Shaped labels by text and the bits of their size.
type LayoutCache = HashMap<(String, u32), Rc<Layout<BrushIndex>>>;

thread_local! {
    static FONT_CX: RefCell<FontContext> = RefCell::new({
        let mut cx = FontContext::new();
        cx.collection
            .register_fonts(xilem::Blob::new(std::sync::Arc::new(crate::application::view::UI_FONT)), None);
        cx
    });
    static LAYOUT_CX: RefCell<LayoutContext<BrushIndex>> = RefCell::new(LayoutContext::new());
    /// Shaped labels by text and size. Paint draws the same labels every frame, and shaping one
    /// costs far more than looking it up.
    static LAYOUTS: RefCell<LayoutCache> = RefCell::new(HashMap::new());
}

/// Entries kept before the cache is emptied. Labels with changing numbers, such as
/// coordinates during a drag, add entries that are never asked for again.
const MAX_CACHED_LAYOUTS: usize = 4096;

/// The shaped, single-line layout of `text` at `size`, shared between frames.
fn layout(text: &str, size: f32) -> Rc<Layout<BrushIndex>> {
    let key = (text.to_owned(), size.to_bits());
    if let Some(found) = LAYOUTS.with(|layouts| layouts.borrow().get(&key).cloned()) {
        return found;
    }
    let shaped = FONT_CX.with(|font_cx| {
        LAYOUT_CX.with(|layout_cx| {
            let mut font_cx = font_cx.borrow_mut();
            let mut layout_cx = layout_cx.borrow_mut();
            let mut builder = layout_cx.ranged_builder(&mut font_cx, text, 1.0, true);
            builder.push_default(StyleProperty::FontSize(size));
            builder.push_default(masonry::parley::FontFamily::named(
                crate::application::view::UI_FONT_FAMILY,
            ));
            builder.push_default(StyleProperty::Brush(BrushIndex(0)));
            let mut layout: Layout<BrushIndex> = builder.build(text);
            layout.break_all_lines(None);
            Rc::new(layout)
        })
    });
    LAYOUTS.with(|layouts| {
        let mut layouts = layouts.borrow_mut();
        if layouts.len() >= MAX_CACHED_LAYOUTS {
            layouts.clear();
        }
        layouts.insert(key, shaped.clone());
    });
    shaped
}

/// Horizontal anchor for a drawn label.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Anchor {
    Start,
    Middle,
    End,
}

/// Draw `text` at screen point `at`, `size` px, in `color`.
/// `anchor` positions the text horizontally; it is vertically centered.
pub(crate) fn draw(
    painter: &mut masonry::imaging::Painter<'_>,
    at: Point,
    text: &str,
    size: f32,
    color: Color,
    anchor: Anchor,
) {
    let layout = layout(text, size);
    let w = f64::from(layout.width());
    let h = f64::from(layout.height());
    let x = match anchor {
        Anchor::Start => at.x,
        Anchor::Middle => at.x - w / 2.0,
        Anchor::End => at.x - w,
    };
    let y = at.y - h / 2.0;
    let brush: Brush = color.into();
    render_text(painter, Affine::translate((x, y)), &layout, &[brush], false);
}

/// Measure a shaped UI label in logical pixels.
pub(crate) fn width(text: &str, size: f32) -> f64 {
    f64::from(layout(text, size).width())
}

/// Draw a single-line label, with an ellipsis if its measured width exceeds `available`.
pub(crate) fn draw_elided(
    painter: &mut masonry::imaging::Painter<'_>,
    at: Point,
    text: &str,
    size: f32,
    color: Color,
    anchor: Anchor,
    available: f64,
) {
    if width(text, size) <= available {
        return draw(painter, at, text, size, color, anchor);
    }
    if width("…", size) > available {
        return;
    }
    let mut end = text.len();
    loop {
        let candidate = format!("{}…", &text[..end]);
        if width(&candidate, size) <= available {
            return draw(painter, at, &candidate, size, color, anchor);
        }
        end = text[..end]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index);
    }
}
