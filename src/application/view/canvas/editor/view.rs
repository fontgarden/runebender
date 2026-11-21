// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Xilem view that hosts the editor widget.
//! The view options, underlay and picture values that it passes to the widget are also here.

use super::*;

pub(super) fn dispatch_editor_event(
    app: &mut Workspace,
    session: &mut Session,
    event: EditorEvent,
    on_event: impl FnOnce(&mut Workspace, EditorEvent),
) {
    if matches!(
        &event,
        EditorEvent::SketchChanged | EditorEvent::Interpolation(_)
    ) {
        on_event(app, event);
        return;
    }
    match app.sync_session_from(session) {
        SessionSyncOutcome::Changed => on_event(app, event),
        SessionSyncOutcome::Unchanged if !matches!(event, EditorEvent::Edited) => {
            on_event(app, event);
        }
        SessionSyncOutcome::Unchanged | SessionSyncOutcome::Rejected => {}
    }
}

fn interp_eq(a: &Option<Arc<kurbo::BezPath>>, b: &Option<Arc<kurbo::BezPath>>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => Arc::ptr_eq(x, y),
        (None, None) => true,
        _ => false,
    }
}

pub(crate) struct EditorView<F> {
    hoi: Option<Arc<InterpolationCanvas>>,
    session: Arc<Session>,
    palette: Arc<Palette>,
    groups: (String, String),
    mark: Option<xilem::Color>,
    tool: Tool,
    sketch: Arc<Mutex<SketchLayer>>,
    sketch_source: usize,
    sketch_revision: u64,
    preview_mode: bool,
    view: ViewOptions,
    ghosts: Arc<Vec<Arc<kurbo::BezPath>>>,
    interp: Option<Arc<kurbo::BezPath>>,
    underlay: Underlay,
    text: Option<crate::application::editor::tools::text::TextInputs>,
    ai_ghost: Option<Arc<ShapeGhost>>,
    focus_target: Arc<Mutex<Option<WidgetId>>>,
    on_event: F,
}

// The editor takes every input it draws from as its own argument, so the
// call site reads as a list of what the view depends on.
#[expect(
    clippy::too_many_arguments,
    reason = "one argument per layer this paint pass draws"
)]
pub(crate) fn editor<F: Fn(&mut Workspace, EditorEvent) + 'static>(
    session: Arc<Session>,
    palette: Arc<Palette>,
    groups: (String, String),
    mark: Option<xilem::Color>,
    tool: Tool,
    sketch: Arc<Mutex<SketchLayer>>,
    sketch_source: usize,
    preview_mode: bool,
    view: ViewOptions,
    ghosts: Arc<Vec<Arc<kurbo::BezPath>>>,
    interp: Option<Arc<kurbo::BezPath>>,
    underlay: Underlay,
    text: Option<crate::application::editor::tools::text::TextInputs>,
    ai_ghost: Option<Arc<ShapeGhost>>,
    focus_target: Arc<Mutex<Option<WidgetId>>>,
    on_event: F,
) -> EditorView<F> {
    let sketch_revision = sketch
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .revision();
    EditorView {
        hoi: None,
        session,
        palette,
        groups,
        mark,
        tool,
        sketch,
        sketch_source,
        sketch_revision,
        preview_mode,
        view,
        ghosts,
        interp,
        underlay,
        text,
        ai_ghost,
        focus_target,
        on_event,
    }
}

/// The analysis overlays: what the editor draws on top of the outline
/// besides the points.
///
/// They read from Runebender's analysis modules so results remain consistent across
/// interfaces.
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct ViewOptions {
    /// Draw design-grid lines instead of dots.
    pub grid_lines: bool,
    /// The curvature comb.
    pub comb: bool,
    /// Stroke thickness rungs across the ink, colored on a warm ramp.
    pub thickness: bool,
    /// A dot per on-curve node, colored by continuity level.
    pub continuity: bool,
    /// Tint the outline and handles by segment length.
    pub colorize: bool,
    /// Label handle lengths.
    pub handles: bool,
    /// Label straight segment lengths.
    pub segments: bool,
    /// Draw bounding boxes and width×height labels for every segment.
    pub sizes: bool,
    /// Draw horizontal and vertical stem/counter spans.
    pub spans: bool,
    /// Draw and label the side bearings.
    pub bearings: bool,
    /// Spell lengths as sums of powers of two: 96 = 64+32.
    pub popcount: bool,
    /// Show every component's points in place, ready to edit.
    pub components: bool,
}

impl ViewOptions {
    /// Whether anything in the measure group is on.
    pub(crate) fn measures(self) -> bool {
        self.colorize || self.handles || self.segments || self.sizes || self.spans || self.bearings
    }

    /// A length, spelled the way the options ask for.
    pub(super) fn label(self, value: i64) -> String {
        if self.popcount {
            runebender::analysis::measure::label(value)
        } else {
            value.to_string()
        }
    }
}

/// What is drawn under the outline: the UFO background layer, a
/// reference glyph, and an optional proposal comparison. All are
/// read-only; the warm proposal is stronger so it remains legible
/// against the editable foreground.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct Underlay {
    /// The glyph's contours in the UFO's background layer.
    pub background: Option<Arc<kurbo::BezPath>>,
    /// Another glyph, shown behind this one.
    pub reference: Option<Arc<kurbo::BezPath>>,
    /// Marks positioned by matching `_anchor`/`anchor` pairs.
    pub mark_cloud: Vec<Arc<kurbo::BezPath>>,
    /// A waiting proposal for this glyph, shown in warm amber.
    pub proposal: Option<Arc<kurbo::BezPath>>,
    /// A completed, unapplied Brush draft rendered without compiling the font.
    pub brush_candidate: Option<Arc<kurbo::BezPath>>,
    /// The picture placed behind the glyph, such as a page of calligraphy to trace.
    pub image: Option<PlacedImage>,
}

/// A decoded picture and where it sits on the canvas.
#[derive(Clone, PartialEq)]
pub(crate) struct PlacedImage {
    /// The pixels, already faded to sit behind the outline.
    pub data: ImageData,
    /// Maps pixel space (origin at the top left, y down) to font units.
    pub to_glyph: Affine,
}

impl<F> EditorView<F> {
    pub(crate) fn interpolation(mut self, data: Option<Arc<InterpolationCanvas>>) -> Self {
        self.hoi = data;
        self
    }
}

impl<F> ViewMarker for EditorView<F> {}
impl<F: Fn(&mut Workspace, EditorEvent) + 'static> View<Workspace, (), ViewCtx> for EditorView<F> {
    type Element = Pod<EditorWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut Workspace) -> (Self::Element, Self::ViewState) {
        let mut session = (*self.session).clone();
        if self.text.is_some() {
            // Framing is decided in layout, and the text line needs a
            // different frame than one glyph.
            session.fitted = false;
        }
        let widget = EditorWidget {
            hoi: self.hoi.clone(),
            hoi_drag: None,
            hoi_control: None,
            session,
            palette: self.palette.clone(),
            groups: self.groups.clone(),
            mark: self.mark,
            tool: self.tool,
            sketch: self.sketch.clone(),
            sketch_source: self.sketch_source,
            sketch_image: None,
            thickness_image: None,
            text_thickness: Default::default(),
            grid_dots: std::cell::RefCell::default(),
            preview_mode: self.preview_mode,
            ghosts: self.ghosts.clone(),
            interp: self.interp.clone(),
            underlay: self.underlay.clone(),
            image_preview: None,
            text: self
                .text
                .as_ref()
                .map(crate::application::editor::tools::text::TextState::new),
            text_inputs: self.text.clone(),
            size: Size::ZERO,
            drag: Drag::None,
            hover: None,
            hover_vertical: false,
            ruler: None,
            insert_preview: None,
            label_hover: None,
            menu: None,
            view: self.view,
            field: None,
            field_buf: String::new(),
            ai_box: None,
            ai_hover: None,
            ai_ghost: None,
            cursor_blink_elapsed_ns: 0,
            cursor_visible: true,
        };
        let pod = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        *self
            .focus_target
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(pod.new_widget.id());
        (pod, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut Workspace,
    ) {
        let mut dirty = false;
        if self.hoi != prev.hoi {
            let reframe = match (&self.hoi, &prev.hoi) {
                (Some(a), Some(b)) => a.glyph != b.glyph || a.interval != b.interval,
                (Some(_), None) => true,
                _ => false,
            };
            let obsolete = match (&self.hoi, &prev.hoi) {
                (Some(a), Some(b)) => a.revision != b.revision || a.preview_only != b.preview_only,
                _ => true,
            };
            element.widget.hoi = self.hoi.clone();
            if reframe || obsolete {
                element.widget.hoi_drag = None;
            }
            if reframe {
                element.widget.fit_interpolation();
            }
            dirty = true;
        }
        if !Arc::ptr_eq(&self.palette, &prev.palette) {
            element.widget.palette = self.palette.clone();
            dirty = true;
        }
        if !Arc::ptr_eq(&self.session, &prev.session) {
            if self.session.canonical_address() != prev.session.canonical_address() {
                element.widget.ai_box = None;
                element.widget.ai_hover = None;
                element.widget.ai_ghost = None;
            }
            let viewport = element.widget.session.viewport.clone();
            let fitted = element.widget.session.fitted;
            element.widget.session = (*self.session).clone();
            element.widget.session.viewport = viewport;
            element.widget.session.fitted = fitted;
            // A press on another part's point opened that part: keep the press going as a
            // drag of the point it selected.
            if let Drag::Enter { start } = element.widget.drag {
                element.widget.drag = if !element.widget.session.has_selection() {
                    Drag::None
                } else {
                    element.widget.session.begin_point_drag();
                    Drag::Points { start }
                };
            }
            dirty = true;
        }
        if self.tool != prev.tool {
            element.widget.hoi_drag = None;
            element.widget.tool = self.tool;
            element.widget.cursor_blink_elapsed_ns = 0;
            element.widget.cursor_visible = true;
            if self.tool != Tool::AiShape {
                element.widget.ai_box = None;
                element.widget.ai_ghost = None;
            }
            if self.tool == Tool::Text {
                element.ctx.request_anim_frame();
            }
            if self.tool != Tool::Pen {
                element.widget.session.pen_cancel();
            }
            dirty = true;
        }
        if self.sketch_source != prev.sketch_source {
            element.widget.sketch_source = self.sketch_source;
            dirty = true;
        }
        let ghost_same = match (&self.ai_ghost, &prev.ai_ghost) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !ghost_same {
            element.widget.ai_ghost = self.ai_ghost.clone();
            dirty = true;
        }
        if self.sketch_revision != prev.sketch_revision {
            dirty = true;
        }
        if self.preview_mode != prev.preview_mode {
            element.widget.preview_mode = self.preview_mode;
            dirty = true;
        }
        if self.groups != prev.groups {
            element.widget.groups = self.groups.clone();
            dirty = true;
        }
        if self.mark != prev.mark {
            element.widget.mark = self.mark;
            dirty = true;
        }
        if self.view != prev.view {
            element.widget.view = self.view;
            dirty = true;
        }
        // The pane allocates the ghost list on every rebuild; the paths inside it are shared
        // with the font, so the same set of them means the same ghosts.
        let same_ghosts = self.ghosts.len() == prev.ghosts.len()
            && self
                .ghosts
                .iter()
                .zip(prev.ghosts.iter())
                .all(|(a, b)| Arc::ptr_eq(a, b));
        if !same_ghosts {
            element.widget.ghosts = self.ghosts.clone();
            dirty = true;
        }
        if !interp_eq(&self.interp, &prev.interp) {
            element.widget.interp = self.interp.clone();
            dirty = true;
        }
        if self.underlay != prev.underlay {
            element.widget.underlay = self.underlay.clone();
            dirty = true;
        }
        // The buffer is the widget's while the tool is in hand, so this
        // only replaces it when the app supplies a different one: a new
        // master, a reopened glyph, or the tool being picked up.
        if self.text != element.widget.text_inputs {
            match (&self.text, element.widget.text.as_mut()) {
                // A document/tab switch restores that tab's parked text rather
                // than carrying the previous widget-owned buffer across.
                (Some(inputs), Some(_))
                    if element
                        .widget
                        .text_inputs
                        .as_ref()
                        .is_some_and(|old| !inputs.same_context(old)) =>
                {
                    element.widget.text = Some(
                        crate::application::editor::tools::text::TextState::new(inputs),
                    );
                    element.widget.fit_text();
                    element.ctx.request_layout();
                }
                // A live client can replace the line without a widget input event.
                // Ordinary typing already changed the widget buffer, so do not
                // reconstruct it (and lose its caret) when the app catches up.
                (Some(inputs), Some(state))
                    if element.widget.text_inputs.as_ref().is_some_and(|old| {
                        inputs.replaced_text(old) && state.buffer.text() != inputs.initial_text()
                    }) =>
                {
                    *state = crate::application::editor::tools::text::TextState::new(inputs);
                    element.widget.fit_text();
                    element.ctx.request_layout();
                }
                // Same tool, new master or edited glyph: keep what has
                // been typed and re-read the metrics.
                (Some(inputs), Some(state)) => state.refresh(inputs),
                (Some(inputs), None) => {
                    element.widget.text = Some(
                        crate::application::editor::tools::text::TextState::new(inputs),
                    );
                    element.widget.fit_text();
                    element.ctx.request_layout();
                }
                // Park the buffer between tool changes; only Text handles typing.
                (None, _) => {}
            }
            element.widget.text_inputs = self.text.clone();
            dirty = true;
        }
        if dirty {
            element.ctx.request_render();
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        app: &mut Workspace,
    ) -> MessageResult<()> {
        match message.take_message::<EditorEvent>() {
            Some(event) => {
                // The island is the live source of truth while editing. Pull its
                // session back into the app before the callback runs, so save and
                // the grid preview see the edits (the widget edits its own clone).
                dispatch_editor_event(app, &mut element.widget.session, *event, &self.on_event);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}
