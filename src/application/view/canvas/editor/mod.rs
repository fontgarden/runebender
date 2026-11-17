// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The glyph editor island: a canvas widget that owns the edit session
//! and gesture state, and the view that hosts it.
//!
//! This file holds the widget, its gesture state and its `Widget` implementation.
//! Each sibling file holds one concern, such as painting, pointer handling or the Xilem view.

mod ai_shapes;
mod geometry;
mod interpolation;
mod keyboard;
mod labels;
mod marks;
mod measure;
mod menu;
mod metrics_card;
mod paint;
mod pointer;
mod tools;
mod view;

pub(crate) use self::labels::label_colors;
pub(crate) use self::view::{PlacedImage, Underlay, ViewOptions, editor};

use self::marks::{
    GridDotsKey, direction_marker_shape, grid_alphas, grid_dots_field, grid_dots_offset,
    grid_index, horizontal_rule_rect, paint_metric_crosses, point_grid_marks, point_marker_shape,
    text_sort_metric_ys, vertical_rule_rect,
};
use crate::application::editor::tools::interpolation::InterpolationCanvas;

use std::sync::Arc;
use std::sync::Mutex;

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, CursorIcon, EventCtx, LayerType, LayoutCtx, MeasureCtx, NewWidget,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PointerScrollEvent, PointerUpdate,
    PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, ScrollDelta, TextEvent, UpdateCtx, Widget,
    WidgetId,
};
use masonry::imaging::Painter;
use masonry::kurbo;
use masonry::kurbo::{Affine, Axis, Circle, Line, Point, Rect, Shape as _, Size, Stroke};
use masonry::layout::{LenReq, Length};
use masonry::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use runebender::font::{ComponentId, ContourId, PointId};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::application::design::theme::Palette;
use crate::application::editor::component_host::PartOwner;
use crate::application::editor::session::{
    LabelModifiers, LabelPress, SelectionShape, SelectionTarget, Session, SessionSyncOutcome,
};
use crate::application::editor::tools::ai_shapes::ShapeGhost;
use crate::application::editor::tools::label::LABEL_GRID;
use crate::application::editor::tools::sketch::SketchLayer;
use crate::application::widgets::context_menu::{ContextMenu, MenuAction, MenuRow, MenuTarget};
use crate::application::widgets::text_label::{self, Anchor};
use crate::application::workspace::Tool;
use crate::application::workspace::Workspace;

use crate::application::design::{
    ANCHOR_DIAMOND_SCALE, METRICS_CARD_BOTTOM, METRICS_CARD_HEADER as PANEL_HEADER,
    METRICS_CARD_HEIGHT as PANEL_HEIGHT, METRICS_CARD_INSET as PANEL_PAD,
    METRICS_CARD_WIDTH as PANEL_WIDTH, METRICS_FIELD_GAP, METRICS_FIELD_HEIGHT as PANEL_ROW,
    METRICS_FIELD_START, METRICS_FIELD_WIDTH, POINT_CORNER_RADIUS, POINT_CURVE_RADIUS,
    POINT_GRID_COARSE_LINE_WIDTH, POINT_GRID_FINE_LINE_WIDTH, POINT_HALO_EXTRA, POINT_RING_WIDTH,
    POINT_SELECTED_GROW, START_MARKER_BACK, START_MARKER_HALF_WIDTH, START_MARKER_SCALE,
    START_MARKER_SMOOTH_CUT, START_MARKER_TIP, Stroke as DesignStroke, TEXT_CURSOR_CAP_FRACTION,
    TEXT_CURSOR_CAP_MAX, TEXT_CURSOR_CAP_MIN, TextSize, point_grid_dot_sizes, point_marker_scale,
};

const HIT_RADIUS_PX: f64 = 8.0;
const CURSOR_BLINK_HALF_CYCLE_NS: u64 = 500_000_000;
const CURSOR_BLINK_CYCLE_NS: u64 = CURSOR_BLINK_HALF_CYCLE_NS * 2;

fn cursor_visible_at(elapsed_ns: u64) -> bool {
    elapsed_ns.rem_euclid(CURSOR_BLINK_CYCLE_NS) <= CURSOR_BLINK_HALF_CYCLE_NS
}

/// What the editor reports upward.
#[derive(Debug)]
pub(crate) enum EditorEvent {
    Interpolation(crate::application::editor::tools::interpolation::InterpolationEvent),
    /// The glyph changed; the app should refresh its cached preview.
    Edited,
    /// Selection changed; carries how many points are selected.
    Selection(usize),
    /// Change manual placement for the component selected by the context click.
    ToggleComponentAlignment,
    /// A composed sort was activated: open that glyph without losing the line,
    /// and keep the tool that established the interaction.
    EditGlyph {
        name: String,
        tool: Tool,
    },
    /// The committed logical text changed; park it with the active tab.
    TextChanged(String),
    /// Widget-owned selected shaped occurrence and its proof settings.
    TextProofSelection(Result<runebender::text::buffer::TextProofSelection, String>),
    /// Session-only brush ink changed; the canonical glyph did not.
    SketchChanged,
    /// The local AI shapes tool: a box was dragged; the old ghost is stale.
    AiShapeBox {
        rect: Rect,
    },
    /// The local AI shapes tool: draw `word` into `rect`; `seed` 0 is
    /// the most likely drawing, another seed another option.
    AiShapeDraw {
        rect: Rect,
        seed: u64,
    },
    /// The local AI shapes tool: the box was dismissed.
    AiShapeDrop,
    /// Ask the workspace to accept this exact, origin-guarded drawing preview.
    AiShapeKept {
        ghost: Arc<ShapeGhost>,
    },
    /// Open a component of the open glyph in place.
    EnterComponent(ComponentId),
    /// Move to another part of the composite, selecting one of its points.
    EnterPart {
        owner: PartOwner,
        point: Option<PointId>,
    },
    /// Leave the open component for the glyph that contains it.
    ExitComponent,
    /// Open a glyph in a tab of its own, outside this text line.
    OpenGlyphTab(String),
    /// Cmd+Z: the app undoes on the master's pile.
    Undo,
    /// Cmd+Shift+Z or Cmd+Y.
    Redo,
}

enum Drag {
    /// A press on another part's point is opening that part; the point drag starts when the
    /// new session arrives.
    Enter {
        start: Point,
    },
    Metaballs {
        last: Point,
        changed: bool,
    },
    None,
    Points {
        start: Point,
    },
    Pan {
        last: Point,
    },
    Sketch {
        last: Point,
        changed: bool,
    },
    /// Pen mouse-down at `origin` (design space); becomes handle-drag past a threshold.
    Pen {
        origin: Point,
        dragging: bool,
        point_count: usize,
        active_contour: Option<ContourId>,
    },
    /// The local AI shapes box being resized by an edge or corner:
    /// which sides move, as -1, 0, or 1 on each axis in design space.
    AiResize {
        handle: (i8, i8),
    },
    /// Rubber-band selection in screen space.
    Marquee {
        start: Point,
        current: Point,
        additive: bool,
    },
    /// Freehand selection boundary in screen space.
    Lasso {
        points: Vec<Point>,
        additive: bool,
    },
    /// Moving the layer's picture, or resizing it from a corner. `start` is in design space,
    /// `origin` is the picture's transform at the press, and `anchor` is the corner that stays
    /// put while the opposite one, `corner`, follows the pointer.
    Image {
        start: Point,
        origin: Affine,
        anchor: Option<Point>,
        corner: Point,
    },
    /// A drag of the label tool.
    Label(tools::label::LabelDrag),
    /// Measure tool: the ruler's fixed end in design space; the other end follows the pointer.
    Measure {
        start: Point,
    },
    /// Drawing a shape; endpoints in design space.
    Shape {
        start: Point,
        current: Point,
    },
    /// Dragging one top-level component; `last` is in design space.
    Component {
        last: Point,
    },
    /// Dragging the advance (right sidebearing) line.
    AdvanceLine,
    /// Dragging the left sidebearing line; carries the last cursor x (screen).
    LeftLine {
        last_x: f64,
    },
}

pub(crate) struct EditorWidget {
    hoi: Option<Arc<InterpolationCanvas>>,
    hoi_drag: Option<interpolation::InterpolationDrag>,
    hoi_control: Option<usize>,
    session: Session,
    palette: Arc<Palette>,
    /// The glyph's kerning groups, shown in the floating metrics card.
    groups: (String, String),
    /// A glyph mark colors the card header, as it does in GPUI.
    mark: Option<xilem::Color>,
    tool: Tool,
    sketch: Arc<Mutex<SketchLayer>>,
    sketch_source: usize,
    /// The sketch ink last rasterized for the canvas, by sketch revision and ink alpha,
    /// so a zoom or hover repaint does not rebuild and re-upload a 512 px image.
    sketch_image: Option<(u64, u32, ImageData)>,
    /// The thickness field last rasterized for the canvas, by outline and ramp
    /// colors, so a zoom or hover repaint does not recompute the field.
    thickness_image: Option<(u64, ImageData, Affine)>,
    /// The design grid's dots for the coarse and fine levels, kept between frames.
    /// Each holds the dots for one zoom and canvas size; panning only moves them.
    grid_dots: std::cell::RefCell<[Option<(GridDotsKey, kurbo::BezPath)>; 2]>,
    /// Space is held: pan while showing only the filled design.
    preview_mode: bool,
    ghosts: Arc<Vec<Arc<kurbo::BezPath>>>,
    /// Read-only interpolated instance overlay at the current axis location.
    interp: Option<Arc<kurbo::BezPath>>,
    /// Background layer and reference glyph, drawn under everything.
    underlay: Underlay,
    /// The tab's text composition, kept while outline tools edit one sort.
    text: Option<crate::application::editor::tools::text::TextState>,
    /// The master the buffer was built from.
    text_inputs: Option<crate::application::editor::tools::text::TextInputs>,
    size: Size,
    /// The picture's transform while it is dragged, drawn before it is committed.
    image_preview: Option<Affine>,
    drag: Drag,
    /// Last cursor position in design space, for the pen preview segment and the Measure
    /// tool's scan line.
    hover: Option<Point>,
    /// Option is held: the Measure tool's scan line runs vertically.
    hover_vertical: bool,
    /// The Measure tool's ruler in design space, kept after the drag until a click or Escape.
    ruler: Option<Line>,
    /// Where a pen click would add a point to an existing segment, and
    /// whether that point splits a curve.
    insert_preview: Option<(Point, bool)>,
    /// What the label tool's pointer is over, for the glow, the cursor and the caption.
    label_hover: Option<crate::application::editor::tools::label::LabelHit>,
    /// The open context-menu layer, if there is one.
    menu: Option<WidgetId>,
    view: ViewOptions,
    /// The metric box being typed into, if any, and what has been typed.
    ///
    /// This is a hand-written text field. The panel it lives in is
    /// painted rather than composed, so it cannot hold Xilem's
    /// `text_input`: there is no view for a floating panel, and the
    /// `zstack` that would have composed one never finished compiling.
    /// Three numbers over the drawing therefore cost an editing mode.
    field: Option<MetricField>,
    field_buf: String,
    /// The local AI shapes tool: the box dragged, the word typed into
    /// the field painted under it, and what the model drew.
    ai_box: Option<Rect>,
    /// The resize handle under the pointer, for its hover state.
    ai_hover: Option<(i8, i8)>,
    ai_ghost: Option<Arc<ShapeGhost>>,
    /// Text caret blink phase; reset whenever the person moves or edits it.
    cursor_blink_elapsed_ns: u64,
    cursor_visible: bool,
}

/// Which number in the metrics panel is being typed into.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricField {
    Lsb,
    Width,
    Rsb,
}

impl EditorWidget {
    fn emit(&self, ctx: &mut EventCtx<'_>, edited: bool) {
        if edited {
            ctx.submit_action::<EditorEvent>(EditorEvent::Edited);
        }
        ctx.submit_action::<EditorEvent>(EditorEvent::Selection(self.session.selection.len()));
        ctx.request_render();
    }

    fn emit_text_changed(&self, ctx: &mut EventCtx<'_>) {
        if let Some(text) = &self.text {
            ctx.submit_action::<EditorEvent>(EditorEvent::TextChanged(text.buffer.text()));
            self.emit_text_proof_selection(ctx);
        }
    }

    fn emit_text_proof_selection(&self, ctx: &mut EventCtx<'_>) {
        if let Some(text) = &self.text {
            ctx.submit_action::<EditorEvent>(EditorEvent::TextProofSelection(
                text.buffer.proof_selection(),
            ));
        }
    }
}

impl Widget for EditorWidget {
    type Action = EditorEvent;

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        true
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        match len_req {
            LenReq::FitContent(space) => space,
            _ => Length::px(200.0),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        if self.size != size {
            self.size = size;
            if !self.session.fitted {
                // An open composition frames the line even while Select edits
                // one sort within it.
                if self.text.is_some() {
                    self.fit_text();
                } else {
                    self.fit();
                }
                // Deterministic close-up evidence for zoom-dependent grid
                // rendering. This is deliberately gated to headless capture.
                if std::env::var("RUNEBENDER_SCREENSHOT").is_ok()
                    && let Ok(zoom) = std::env::var("RUNEBENDER_EDITOR_ZOOM")
                    && let Ok(zoom) = zoom.parse::<f64>()
                    && zoom.is_finite()
                    && zoom > 0.0
                {
                    let center = self.size.to_rect().center();
                    self.session.viewport.zoom_about(
                        center,
                        zoom / self.session.viewport.zoom,
                        zoom,
                        zoom,
                    );
                }
            }
        }
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        self.paint_canvas(painter);
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.handle_pointer_event(ctx, event);
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        self.handle_text_event(ctx, event);
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        if self.tool != Tool::Text || self.text.is_none() {
            return;
        }

        self.cursor_blink_elapsed_ns = self
            .cursor_blink_elapsed_ns
            .saturating_add(interval)
            .rem_euclid(CURSOR_BLINK_CYCLE_NS);
        // Keep the exact half-cycle boundary visible. Headless proof renders
        // after a 500 ms settle frame, while a real display crosses the
        // boundary on its following frame.
        let visible = cursor_visible_at(self.cursor_blink_elapsed_ns);
        if visible != self.cursor_visible {
            self.cursor_visible = visible;
            ctx.request_paint_only();
        }
        ctx.request_anim_frame();
    }

    fn accessibility_role(&self) -> Role {
        Role::Canvas
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, pos: Point) -> CursorIcon {
        if self.tool == Tool::Interpolation {
            return if self.hoi_drag.is_some() {
                CursorIcon::Grabbing
            } else if self.interpolation_hit(ctx.to_local(pos)).is_some() {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            };
        }
        let resizing = match self.drag {
            Drag::AiResize { handle } => Some(handle),
            _ => self.ai_hover,
        };
        if let Some(handle) = resizing {
            return match handle {
                (0, _) => CursorIcon::NsResize,
                (_, 0) => CursorIcon::EwResize,
                (-1, 1) | (1, -1) => CursorIcon::NwseResize,
                _ => CursorIcon::NeswResize,
            };
        }
        if self.tool == Tool::Hand {
            if matches!(self.drag, Drag::Pan { .. }) {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            }
        } else if self.tool == Tool::Measure {
            CursorIcon::Crosshair
        } else {
            CursorIcon::Default
        }
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_description(format!("Glyph editor: {}", self.session.glyph_name));
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

#[cfg(test)]
mod tests;
