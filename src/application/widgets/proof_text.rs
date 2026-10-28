// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Selectable native specimens, each registered under a private font family.
//! Before, Working and references may share source family names without colliding.
//! Masonry owns shaping, bidi layout, wrapping, selection, accessibility and clipboard behavior.

use crate::application::workspace::Workspace;
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, PaintCtx, PropertiesMut,
    PropertiesRef, PropertySet, RegisterCtx, StyleProperty, Update, UpdateCtx, Widget, WidgetPod,
};
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use masonry::parley::fontique::{FamilyId, FontInfo, FontInfoOverride};
use masonry::parley::{FontStyle, FontWeight, FontWidth};
use masonry::properties::ContentColor;
use masonry::widgets::{Prose, TextArea};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

#[derive(Clone)]
pub(crate) struct ProofText {
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) text: String,
    pub(crate) size: f32,
    pub(crate) variations: String,
    pub(crate) color: Color,
}

pub(crate) struct ProofTextWidget {
    child: WidgetPod<Prose>,
    bytes: Arc<Vec<u8>>,
    family: String,
    registered: Vec<(FamilyId, Vec<FontInfo>)>,
}
impl ProofTextWidget {
    fn register(&mut self, fonts: &mut masonry::parley::FontContext) {
        self.unregister(fonts);
        self.registered = fonts.collection.register_fonts(
            xilem::Blob::new(self.bytes.clone()),
            Some(FontInfoOverride {
                family_name: Some(&self.family),
                // A private alias selects this exact face without synthetic styling.
                weight: Some(FontWeight::default()),
                width: Some(FontWidth::default()),
                style: Some(FontStyle::default()),
                ..Default::default()
            }),
        );
    }
    fn unregister(&mut self, fonts: &mut masonry::parley::FontContext) {
        for (family, infos) in self.registered.drain(..) {
            for info in infos {
                fonts
                    .collection
                    .unregister_font(family, info.width(), info.style(), info.weight());
            }
        }
    }
}
impl Widget for ProofTextWidget {
    type Action = ();
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded) {
            self.register(ctx.text_contexts().0);
        }
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        cross: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.child, axis, cross)
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
    }
    fn paint(
        &mut self,
        _: &mut PaintCtx<'_>,
        _: &PropertiesRef<'_>,
        _: &mut masonry::imaging::Painter<'_>,
    ) {
    }
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}
impl ViewMarker for ProofText {}
impl View<Workspace, (), ViewCtx> for ProofText {
    type Element = Pod<ProofTextWidget>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut Workspace) -> (Self::Element, ()) {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let family = format!(
            "Runebender specimen {}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let area = TextArea::new_immutable(&self.text)
            .with_style(StyleProperty::FontFamily(
                masonry::parley::FontFamily::named(&family).into_owned(),
            ))
            .with_style(StyleProperty::FontSize(self.size))
            .with_style(StyleProperty::FontVariations(
                masonry::parley::FontVariations::Source(std::borrow::Cow::Owned(
                    self.variations.clone(),
                )),
            ))
            .with_style(StyleProperty::LineHeight(
                masonry::parley::LineHeight::FontSizeRelative(1.5),
            ))
            .with_style(StyleProperty::OverflowWrap(
                masonry::parley::OverflowWrap::Anywhere,
            ))
            .with_word_wrap(true);
        let mut props = PropertySet::new();
        props.insert(ContentColor::new(self.color));
        let child = Prose::from_text_area(NewWidget::new(area).with_props(props)).with_clip(true);
        (
            ctx.create_pod(ProofTextWidget {
                child: NewWidget::new(child).to_pod(),
                bytes: self.bytes.clone(),
                family,
                registered: Vec::new(),
            }),
            (),
        )
    }
    fn rebuild(
        &self,
        prev: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut Workspace,
    ) {
        let changed_font = !Arc::ptr_eq(&self.bytes, &prev.bytes);
        if changed_font {
            element.widget.bytes = self.bytes.clone();
            let fonts = element.ctx.text_contexts().0;
            element.widget.register(fonts);
        }
        let family = element.widget.family.clone();
        let mut child = element.ctx.get_mut(&mut element.widget.child);
        let mut text = Prose::text_mut(&mut child);
        if changed_font || self.text != prev.text {
            TextArea::reset_text(&mut text, &self.text);
        }
        if changed_font {
            TextArea::insert_style(
                &mut text,
                StyleProperty::FontFamily(masonry::parley::FontFamily::named(&family).into_owned()),
            );
        }
        if self.size != prev.size {
            TextArea::insert_style(&mut text, StyleProperty::FontSize(self.size));
        }
        if self.variations != prev.variations {
            TextArea::insert_style(
                &mut text,
                StyleProperty::FontVariations(masonry::parley::FontVariations::Source(
                    std::borrow::Cow::Owned(self.variations.clone()),
                )),
            );
        }
        if self.color != prev.color {
            text.insert_prop(ContentColor::new(self.color));
        }
    }
    fn teardown(&self, _: &mut (), _: &mut ViewCtx, mut element: Mut<'_, Self::Element>) {
        let fonts = element.ctx.text_contexts().0;
        element.widget.unregister(fonts);
    }
    fn message(
        &self,
        _: &mut (),
        _: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        _: &mut Workspace,
    ) -> MessageResult<()> {
        MessageResult::Stale
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::design::stock;
    use crate::application::view::render::panel_resize_tests_support::context;
    use masonry::core::keyboard::{Code, Key, KeyState, KeyboardEvent, Modifiers};
    use masonry::core::{TextEvent, WidgetRef};
    use masonry_testing::TestHarness;

    #[test]
    fn specimens_wrap_and_copy_original_unicode() {
        let mut app = Workspace::untitled().unwrap();
        let original = "Hamburgefonts 0123456789 — العربية\nA second line of specimen text";
        let view = ProofText {
            bytes: Arc::new(
                include_bytes!("../../../assets/fonts/VirtuaGrotesk-Regular.ttf").to_vec(),
            ),
            text: original.into(),
            size: 32.0,
            variations: String::new(),
            color: Color::BLACK,
        };
        let mut ctx = context();
        let (pod, _) = view.build(&mut ctx, &mut app);
        let mut harness = TestHarness::create_with_size(
            stock::widget_properties(&app.palette),
            pod.new_widget,
            (220, 500),
        );
        fn text_id(widget: WidgetRef<'_, dyn Widget>) -> Option<masonry::core::WidgetId> {
            if widget.short_type_name() == "TextArea" {
                return Some(widget.id());
            }
            widget.children().into_iter().find_map(text_id)
        }
        let id = text_id(harness.root_widget().as_dyn()).expect("selectable text child");
        harness.focus_on(Some(id));
        let modifiers = if cfg!(target_os = "macos") {
            Modifiers::META
        } else {
            Modifiers::CONTROL
        };
        for key in ["a", "c"] {
            harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
                state: KeyState::Down,
                key: Key::Character(key.into()),
                code: Code::Unidentified,
                modifiers,
                ..KeyboardEvent::default()
            }));
        }
        assert_eq!(harness.clipboard_contents(), original);
        let text = harness.get_widget_with_id(id);
        assert!(text.ctx().border_box().width() <= 220.0);
        assert!(
            text.ctx().border_box().height() > 96.0,
            "sample should wrap into multiple lines"
        );
    }
}
