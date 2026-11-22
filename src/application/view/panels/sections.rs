// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The info panel's sections: layers, axes, paths, coordinates, curves, measure, background, marks, font info.

use crate::application::design;
use crate::application::design::controls::icon_button;
use crate::application::design::recipes;
use crate::application::design::recipes::button;
use crate::application::design::theme::Palette;
use crate::application::design::{
    ControlKind, ControlSize, ControlStyle, Region, Space, TextSize, column as xcolumn, row as xrow,
};
use crate::application::editor::session::Session;
use crate::application::view::chrome::direction_chips;
use crate::application::view::panels::tabs::tab_chip;
use crate::application::view::{label, text_input};
use crate::application::widgets::icon_paint;
use crate::application::widgets::input_typography;
use crate::application::workspace::{Mode, Workspace};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::FlexExt as _;
use xilem::view::{FlexSpacer, sized_box};

/// The glyph's layers, as in the Glyphs Layers panel: the master's own layer, its backup layers
/// and the other masters. Clicking a layer of the active master edits it; an eye draws a
/// layer's outline behind the layer being edited; plus copies the edited layer into a new
/// backup layer, and minus deletes the edited backup layer.
pub(crate) fn layers_section(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    if app.font.master_names().len() < 2 && !app.can_manage_glyph_layers() {
        return None;
    }
    let pal = &app.palette;
    let editor = app.can_manage_glyph_layers();
    let mut rows: Vec<Box<xilem::AnyWidgetView<Workspace>>> = Vec::new();
    if editor {
        for row in app.glyph_layer_rows() {
            let (eye_layer, edit_layer) = (row.layer.clone(), row.layer);
            rows.push(layer_row(
                pal,
                row.name,
                row.editing,
                row.visible,
                Box::new(move |app| app.toggle_glyph_layer_visible(&eye_layer)),
                Box::new(move |app| app.edit_glyph_layer(&edit_layer)),
            ));
        }
    }
    let active = app.font.active();
    for (index, name) in app.font.short_master_names().into_iter().enumerate() {
        if index == active {
            continue;
        }
        // Another master: its eye shows it behind the edited layer, its name switches to it.
        rows.push(layer_row(
            pal,
            name,
            false,
            app.reference_layers.contains(&index),
            Box::new(move |app| {
                if !app.reference_layers.remove(&index) {
                    app.reference_layers.insert(index);
                }
            }),
            Box::new(move |app| app.set_master(index)),
        ));
    }
    let backup = editor && app.editing_backup_layer();
    let footer = editor.then(|| {
        xrow(
            Region::Inline,
            (
                icon_button::icon_button(
                    ControlStyle::new(pal, ControlKind::Bare),
                    "plus",
                    "Copy this layer to a new layer",
                    |app: &mut Workspace| app.duplicate_glyph_layer(),
                )
                .icon_size(design::STATUS_ICON_SIZE)
                .tile_size(design::STATUS_ICON_SIZE),
                // Only a backup layer can be deleted or replace the master.
                backup.then(|| {
                    icon_button::icon_button(
                        ControlStyle::new(pal, ControlKind::Bare),
                        "minus",
                        "Delete this layer",
                        |app: &mut Workspace| app.remove_glyph_layer(),
                    )
                    .icon_size(design::STATUS_ICON_SIZE)
                    .tile_size(design::STATUS_ICON_SIZE)
                }),
                FlexSpacer::Flex(1.0),
                backup.then(|| {
                    recipes::action(pal, "Use as master".into(), |app: &mut Workspace| {
                        app.use_glyph_layer_as_master();
                    })
                }),
            ),
        )
    });
    // The edited backup layer's name; Enter renames it.
    let rename = backup.then(|| {
        recipes::field_bare(
            pal,
            "Layer name",
            app.layer_name_buf.clone(),
            |app: &mut Workspace, value| app.layer_name_buf = value,
            |app: &mut Workspace, value| app.rename_glyph_layer(&value),
        )
    });
    Some(recipes::section(
        app,
        "Layers",
        "Layers",
        xcolumn(Region::List, (xcolumn(Region::List, rows), rename, footer)),
    ))
}

type LayerClick = Box<dyn Fn(&mut Workspace) + Send + Sync + 'static>;

/// One layer: its eye, then its name. `editing` marks the layer the canvas edits.
fn layer_row(
    pal: &Palette,
    name: String,
    editing: bool,
    visible: bool,
    on_eye: LayerClick,
    on_name: LayerClick,
) -> Box<xilem::AnyWidgetView<Workspace>> {
    let style = ControlStyle::new(pal, ControlKind::Row).selected(pal, editing);
    sized_box(xrow(
        Region::Inline,
        (
            icon_button::icon_button(
                ControlStyle::new(pal, ControlKind::Bare),
                if visible { "eye-open" } else { "eye-closed" },
                "Show this layer behind the edited layer",
                move |app: &mut Workspace| on_eye(app),
            )
            .icon_size(design::STATUS_ICON_SIZE)
            .tile_size(design::STATUS_ICON_SIZE),
            sized_box(
                button(
                    style,
                    // A row keeps its name at the left edge, as sidebar rows do.
                    xrow(
                        Region::Inline,
                        (
                            label(name)
                                .text_size(TextSize::Body.px())
                                .color(style.ink()),
                            FlexSpacer::Flex(1.0),
                        ),
                    ),
                    move |app: &mut Workspace| on_name(app),
                )
                // The name sits inside the row's face, not against its edge.
                .padding(masonry::properties::Padding::horizontal(Length::px(
                    design::SIDEBAR_ROW_INSET,
                ))),
            )
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::from(ControlSize::Control),
            ))
            .flex(1.0),
        ),
    ))
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::from(ControlSize::Control),
    ))
    .boxed()
}

fn master_rows(app: &Workspace) -> Vec<impl WidgetView<Workspace> + use<>> {
    let pal = &app.palette;
    app.font
        .short_master_names()
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let active = i == app.font.active();
            let style = ControlStyle::new(pal, ControlKind::Row)
                .selected(pal, active)
                .hover_outline(pal.outline);
            sized_box(
                button(
                    style,
                    xrow(
                        Region::Inline,
                        (
                            label(name)
                                .text_size(TextSize::Body.px())
                                .color(style.ink()),
                            FlexSpacer::Flex(1.0),
                        ),
                    ),
                    move |app: &mut Workspace| app.set_master(i),
                )
                .padding(masonry::properties::Padding::horizontal(Length::px(
                    design::SIDEBAR_ROW_INSET,
                ))),
            )
            .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Icon)))
        })
        .collect()
}

/// Masters: one compact row per designspace source. This is deliberately
/// separate from reference-layer controls: GPUI presents master switching as
/// a plain list, while underlays belong to the editor's Layers section.
pub(crate) fn masters_section(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    app.font.project.document_designspace()?;
    Some(recipes::section(
        app,
        "Masters",
        "Masters",
        xcolumn(Region::List, master_rows(app)),
    ))
}

/// Axes: one labeled slider per designspace axis, in the inspector.
pub(crate) fn axes_section(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    if app.font.axes.is_empty() {
        return None;
    }
    let pal = &app.palette;
    let (muted, text) = (pal.text_muted, pal.text);
    let rows: Vec<_> = app
        .font
        .axes
        .iter()
        .enumerate()
        .map(|(i, ax)| {
            let value = app.axis_values.get(i).copied().unwrap_or(ax.default);
            xcolumn(
                Region::List,
                (
                    xrow(
                        Region::Inline,
                        (
                            label(ax.tag.clone())
                                .text_size(TextSize::Body.px())
                                .color(muted),
                            FlexSpacer::Flex(1.0),
                            label(format!("{value:.0}"))
                                .text_size(TextSize::Body.px())
                                .color(text),
                        ),
                    ),
                    crate::application::widgets::gesture_slider::gesture_slider(
                        pal,
                        ax.min,
                        ax.max,
                        value,
                        move |app, v, _| app.set_axis(i, v),
                        |_, _| {},
                    )
                    .accessibility_name(ax.name.clone())
                    .track_color(
                        if (value - app.font.master_axis_values(app.font.active())[i]).abs() > 0.001
                        {
                            pal.text
                        } else {
                            pal.slider_track()
                        },
                    )
                    .dims(Dimensions::new(Dim::Stretch, Dim::Auto)),
                ),
            )
        })
        .collect();
    // Distinguish a valid instance from the active-outline fallback used when
    // this glyph's masters cannot interpolate.
    let hint = app.interpolation_status().map(|status| {
        let (summary, detail) = status
            .split_once(": ")
            .map_or((status.clone(), None), |(summary, detail)| {
                (summary.to_string(), Some(detail.to_string()))
            });
        xcolumn(
            Region::List,
            (
                label(if summary == "interpolated" {
                    "Interpolated preview".to_string()
                } else {
                    summary
                })
                .text_size(TextSize::Caption.px())
                .color(pal.text),
                detail.map(|detail| {
                    label(detail)
                        .text_size(TextSize::Caption.px())
                        .color(pal.text_muted)
                }),
            ),
        )
    });
    Some(recipes::section(
        app,
        "Axes",
        "Axes",
        xcolumn(Region::Form, (xcolumn(Region::Form, rows), hint)),
    ))
}

/// Shaping choices for the editor inspector, in the same right-panel position
/// as GPUI. These mutate the shared editor/preview text state.
pub(crate) fn shaping_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let feature = |tag: &'static str| {
        tab_chip(
            pal,
            tag.into(),
            !app.text_features_disabled.contains(tag),
            false,
            move |app: &mut Workspace| {
                if !app.text_features_disabled.remove(tag) {
                    app.text_features_disabled.insert(tag.into());
                }
            },
        )
    };
    let locale =
        |label_text: &'static str, script: Option<&'static str>, language: Option<&'static str>| {
            let active =
                app.text_script.as_deref() == script && app.text_language.as_deref() == language;
            tab_chip(
                pal,
                label_text.into(),
                active,
                false,
                move |app: &mut Workspace| {
                    app.text_script = script.map(str::to_string);
                    app.text_language = language.map(str::to_string);
                },
            )
        };
    recipes::section(
        app,
        "Shaping",
        "Shaping",
        xcolumn(
            Region::Form,
            (
                recipes::field(
                    pal,
                    "Preview text",
                    app.preview_text.clone(),
                    |app: &mut Workspace, value| app.preview_text = value,
                ),
                direction_chips(app),
                xcolumn(
                    Region::List,
                    (
                        xrow(
                            Region::Inline,
                            (feature("liga"), feature("rlig"), feature("kern")),
                        ),
                        xrow(Region::Inline, (feature("mark"), feature("mkmk"))),
                    ),
                ),
                xrow(
                    Region::Inline,
                    (
                        locale("Auto", None, None),
                        locale("Arabic", Some("arab"), Some("ar")),
                        locale("Urdu", Some("arab"), Some("ur")),
                    ),
                ),
            ),
        ),
    )
}

pub(crate) fn transformations_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use crate::application::editor::session::BoolOp;
    use icon_button::icon_button;
    let pal = &app.palette;
    let style = ControlStyle::new(pal, ControlKind::Bare);
    let op = move |icon: &'static str, f: fn(&mut Session) -> bool| {
        icon_button(style, icon, icon, move |app: &mut Workspace| {
            app.apply_op(f);
        })
        .icon_size(design::TRANSFORM_ICON_SIZE)
        .tile_size(design::TRANSFORM_TILE_SIZE)
    };
    recipes::section(
        app,
        "Transformations",
        "Transformations",
        xcolumn(
            Region::Section,
            (
                xrow(
                    Region::List,
                    (
                        op("flip-h", |s| s.flip_horizontal()),
                        FlexSpacer::Flex(1.0),
                        op("flip-v", |s| s.flip_vertical()),
                        FlexSpacer::Flex(1.0),
                        op("rot-ccw", |s| s.rotate_90()),
                        FlexSpacer::Flex(1.0),
                        op("rot-cw", |s| s.rotate_90_clockwise()),
                        FlexSpacer::Flex(1.0),
                        op("duplicate", |s| s.duplicate()),
                        FlexSpacer::Flex(1.0),
                        // Trial sixth column: repeats the last action to test the fit.
                        op("duplicate", |s| s.duplicate()),
                    ),
                ),
                xrow(
                    Region::List,
                    (
                        op("duplicate-repeat", |s| s.duplicate_repeat()),
                        FlexSpacer::Flex(1.0),
                        op("union", |s| s.remove_overlap()),
                        FlexSpacer::Flex(1.0),
                        op("subtract", |s| s.boolean(BoolOp::Subtract)),
                        FlexSpacer::Flex(1.0),
                        op("intersect", |s| s.boolean(BoolOp::Intersect)),
                        FlexSpacer::Flex(1.0),
                        op("exclude", |s| s.boolean(BoolOp::Exclude)),
                        FlexSpacer::Flex(1.0),
                        op("exclude", |s| s.boolean(BoolOp::Exclude)),
                    ),
                ),
            ),
        )
        .gap(Space::Md),
    )
}

/// Curve and outline operations follow GPUI in their own disclosure below
/// geometric transformations.
pub(crate) fn path_operations_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    recipes::section(
        app,
        "Path Operations",
        "Path Operations",
        path_operations_controls(app),
    )
}

/// A transformation parameter shares one row with its label; Enter applies it.
fn transform_parameter(
    app: &Workspace,
    title: &'static str,
    placeholder: &'static str,
    value: String,
    on_change: fn(&mut Workspace, String),
    on_enter: fn(&mut Workspace, String),
) -> impl WidgetView<Workspace> + use<> {
    xrow(
        Region::Inline,
        (
            sized_box(label(title).color(app.palette.text_muted)).dims(Dimensions::new(
                Dim::Fixed(Length::px(design::TRANSFORM_LABEL_WIDTH)),
                Dim::Auto,
            )),
            recipes::field_bare(&app.palette, placeholder, value, on_change, on_enter).flex(1.0),
        ),
    )
}

/// Path operations belong to the same disclosure as the geometric tools.
fn path_operations_controls(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    xcolumn(
        Region::List,
        (
            xrow(
                Region::Inline,
                (
                    tbtn(pal, "Harmonize", |s| s.harmonize()).flex(1.0),
                    tbtn(pal, "Balance", |s| s.balance()).flex(1.0),
                ),
            ),
            xrow(
                Region::Inline,
                (
                    tbtn(pal, "Optimize", |s| s.optimize()).flex(1.0),
                    tbtn(pal, "Add Extremes", |s| s.add_extremes()).flex(1.0),
                ),
            ),
            xrow(
                Region::Inline,
                (
                    tbtn(pal, "Round Corners", |s| s.round_corners()).flex(1.0),
                    tbtn(pal, "Reverse", |s| s.reverse()).flex(1.0),
                ),
            ),
            xrow(
                Region::Inline,
                (tbtn(pal, "Snap to Grid", |s| s.snap_to_dot_grid()).flex(1.0),),
            ),
            transform_parameter(
                app,
                "Slant °",
                "",
                app.slant_buf.clone(),
                |app, value| app.slant_buf = value,
                |app, value| {
                    app.slant_buf = value;
                    app.command_filter_slant();
                },
            ),
            transform_parameter(
                app,
                "Stroke width",
                "",
                app.stroke_buf.clone(),
                |app, value| app.stroke_buf = value,
                |app, value| {
                    app.stroke_buf = value;
                    app.command_expand_stroke();
                },
            ),
            transform_parameter(
                app,
                "Offset ±",
                "",
                app.offset_buf.clone(),
                |app, value| app.offset_buf = value,
                |app, value| {
                    app.offset_buf = value;
                    app.command_filter_offset();
                },
            ),
            transform_parameter(
                app,
                "Extrude d,°",
                "15,30",
                app.extrude_buf.clone(),
                |app, value| app.extrude_buf = value,
                |app, value| {
                    app.extrude_buf = value;
                    app.command_filter_extrude();
                },
            ),
            transform_parameter(
                app,
                "Roughen s,h,v",
                "15,15,10",
                app.roughen_buf.clone(),
                |app, value| app.roughen_buf = value,
                |app, value| {
                    app.roughen_buf = value;
                    app.command_filter_roughen();
                },
            ),
        ),
    )
    .gap(Space::Sm)
}

/// The LSB/RSB text-buffer strings for a session.
pub(crate) fn metric_bufs(session: &Session) -> (String, String) {
    match session.side_bearings() {
        Some(sb) => (format!("{}", sb.lsb), format!("{}", sb.rsb)),
        None => (String::new(), String::new()),
    }
}

/// A labeled path-operation button.
pub(crate) fn tbtn(
    pal: &Palette,
    text: &'static str,
    f: fn(&mut Session) -> bool,
) -> impl WidgetView<Workspace> + use<> {
    recipes::action(pal, text.into(), move |app: &mut Workspace| {
        app.apply_op(f);
    })
}

/// Coordinates: the 9-point reference picker beside compact X/Y/W/H fields.
/// GPUI keeps this panel up whether or not anything is selected, so the
/// inspector does not jump.
pub(crate) fn coordinates_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use crate::application::design::{
        COORD_LABEL_WIDTH, COORD_PICKER_EDGE, COORD_PICKER_GAP, COORD_PICKER_INSET,
    };
    use runebender::outline::path::Quadrant;
    const QUADRANTS: [Quadrant; 9] = [
        Quadrant::TopLeft,
        Quadrant::Top,
        Quadrant::TopRight,
        Quadrant::Left,
        Quadrant::Center,
        Quadrant::Right,
        Quadrant::BottomLeft,
        Quadrant::Bottom,
        Quadrant::BottomRight,
    ];
    let pal = &app.palette;
    let dot = |q: Quadrant| {
        let active = app.coord_quadrant == q;
        let (icon, color) = if active {
            ("coordinate-dot-selected", pal.editor_control_ink())
        } else {
            ("coordinate-dot", pal.outline)
        };
        let label = match q {
            Quadrant::TopLeft => "Top left reference",
            Quadrant::Top => "Top reference",
            Quadrant::TopRight => "Top right reference",
            Quadrant::Left => "Left reference",
            Quadrant::Center => "Center reference",
            Quadrant::Right => "Right reference",
            Quadrant::BottomLeft => "Bottom left reference",
            Quadrant::Bottom => "Bottom reference",
            Quadrant::BottomRight => "Bottom right reference",
        };
        sized_box(
            // A swatch's opaque face hides the grid lines that run through each ring.
            button(
                ControlStyle::new(pal, ControlKind::Swatch),
                icon_paint::view(icon, label, color, ControlSize::Dot.px()),
                move |app: &mut Workspace| {
                    app.coord_quadrant = q;
                    app.refresh_coord_bufs();
                },
            )
            .padding(Space::None),
        )
        .dims(Dimensions::fixed(
            ControlSize::Dot.length(),
            ControlSize::Dot.length(),
        ))
    };
    let row = |a: usize| {
        xrow(
            Region::List,
            (
                dot(QUADRANTS[a]),
                dot(QUADRANTS[a + 1]),
                dot(QUADRANTS[a + 2]),
            ),
        )
        .gap(Length::px(COORD_PICKER_GAP))
    };
    let picker_buttons = sized_box(
        xcolumn(Region::List, (row(0), row(3), row(6))).gap(Length::px(COORD_PICKER_GAP)),
    )
    .padding(Length::px(COORD_PICKER_INSET));
    let picker = sized_box(xilem::view::zstack((
        icon_paint::view(
            "coordinate-grid",
            "Reference grid",
            pal.outline,
            COORD_PICKER_EDGE,
        ),
        picker_buttons,
    )))
    .dims(Dimensions::fixed(
        Length::px(COORD_PICKER_EDGE),
        Length::px(COORD_PICKER_EDGE),
    ));
    let field = |name: &'static str, value: String, axis: usize| {
        xrow(
            Region::Inline,
            (
                sized_box(
                    // Right-aligned, so every letter sits the same distance from its field.
                    label(name)
                        .text_size(TextSize::Body.px())
                        .text_alignment(masonry::TextAlign::End)
                        .color(pal.text_muted),
                )
                .dims(Dimensions::new(
                    Dim::Fixed(Length::px(COORD_LABEL_WIDTH)),
                    Dim::Auto,
                )),
                sized_box(input_typography::input_typography(
                    text_input(value, move |app: &mut Workspace, v| {
                        if axis < 2 {
                            app.set_coord(axis, v);
                        } else {
                            app.set_coord_size(axis == 2, v);
                        }
                    })
                    .text_color(pal.text),
                    pal.text,
                ))
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::from(ControlSize::Control),
                ))
                .flex(1.0),
            ),
        )
        .gap(Space::Sm)
    };
    recipes::section(
        app,
        "Coordinates",
        "Coordinates",
        xrow(
            Region::Inline,
            (
                picker,
                xcolumn(
                    Region::List,
                    (
                        xrow(
                            Region::Inline,
                            (
                                field("X", app.coord_x_buf.clone(), 0).flex(1.0),
                                field("W", app.coord_w_buf.clone(), 2).flex(1.0),
                            ),
                        )
                        .gap(Space::Md),
                        xrow(
                            Region::Inline,
                            (
                                field("Y", app.coord_y_buf.clone(), 1).flex(1.0),
                                field("H", app.coord_h_buf.clone(), 3).flex(1.0),
                            ),
                        )
                        .gap(Space::Md),
                    ),
                )
                .gap(Space::Sm)
                .flex(1.0),
            ),
        )
        .gap(Space::Md),
    )
}

pub(crate) fn curves_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let view = app.view;
    recipes::section(
        app,
        "Curves",
        "Curves",
        xrow(
            Region::Inline,
            (
                recipes::toggle(
                    pal,
                    "Curvature Comb".into(),
                    view.comb,
                    |app: &mut Workspace| {
                        app.view.comb = !app.view.comb;
                    },
                )
                .flex(1.0),
                recipes::toggle(
                    pal,
                    "Thickness".into(),
                    view.thickness,
                    |app: &mut Workspace| {
                        app.view.thickness = !app.view.thickness;
                    },
                )
                .flex(1.0),
                recipes::toggle(
                    pal,
                    "Continuity".into(),
                    view.continuity,
                    |app: &mut Workspace| {
                        app.view.continuity = !app.view.continuity;
                    },
                )
                .flex(1.0),
            ),
        ),
    )
}

/// Measure: labels drawn over the outline in any tool, for handle and segment lengths, spans
/// and side bearings. The Measure tool's own ruler needs none of them.
pub(crate) fn measure_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let view = app.view;
    recipes::section(
        app,
        "Measure",
        "Measure",
        xcolumn(
            Region::Form,
            (
                xrow(
                    Region::Inline,
                    (
                        recipes::toggle(
                            pal,
                            "Color".into(),
                            view.colorize,
                            |app: &mut Workspace| {
                                app.view.colorize = !app.view.colorize;
                            },
                        ),
                        recipes::toggle(
                            pal,
                            "Handles".into(),
                            view.handles,
                            |app: &mut Workspace| {
                                app.view.handles = !app.view.handles;
                            },
                        ),
                    ),
                ),
                xrow(
                    Region::Inline,
                    (
                        recipes::toggle(
                            pal,
                            "Segments".into(),
                            view.segments,
                            |app: &mut Workspace| {
                                app.view.segments = !app.view.segments;
                            },
                        ),
                        recipes::toggle(
                            pal,
                            "Bearings".into(),
                            view.bearings,
                            |app: &mut Workspace| {
                                app.view.bearings = !app.view.bearings;
                            },
                        ),
                    ),
                ),
                // Lengths as sums of powers of two: 96 reads as 64+32. The
                // web editor's habit, and the reason for the tier colors.
                recipes::toggle(
                    pal,
                    "Popcount sums".into(),
                    view.popcount,
                    |app: &mut Workspace| {
                        app.view.popcount = !app.view.popcount;
                    },
                ),
            ),
        ),
    )
}

/// Background: the UFO's background layer, and a reference glyph. Both
/// are things to trace against, so both draw quietly and neither can be
/// selected.
pub(crate) fn background_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    recipes::section(
        app,
        "Background",
        "Background",
        xcolumn(
            Region::Inline,
            (
                xrow(
                    Region::Inline,
                    (
                        recipes::toggle(
                            pal,
                            "Background".into(),
                            app.show_background,
                            |app: &mut Workspace| {
                                app.show_background = !app.show_background;
                            },
                        )
                        .flex(1.0),
                        recipes::toggle(
                            pal,
                            "Mark cloud".into(),
                            app.show_mark_cloud,
                            |app: &mut Workspace| {
                                app.show_mark_cloud = !app.show_mark_cloud;
                            },
                        )
                        .flex(1.0),
                    ),
                ),
                xrow(
                    Region::Inline,
                    (
                        recipes::action(pal, "Send to background".into(), |app: &mut Workspace| {
                            app.send_to_background();
                        })
                        .flex(1.0),
                    ),
                ),
                xrow(
                    Region::Inline,
                    (
                        recipes::action(pal, "Swap".into(), |app: &mut Workspace| {
                            app.swap_background();
                        })
                        .flex(1.0),
                        recipes::action(pal, "Clear".into(), |app: &mut Workspace| {
                            app.clear_background();
                        })
                        .flex(1.0),
                    ),
                ),
                xrow(
                    Region::Inline,
                    (
                        sized_box(label("Reference").color(pal.text_muted)).dims(Dimensions::new(
                            Dim::Fixed(Length::px(design::TRANSFORM_LABEL_WIDTH)),
                            Dim::Auto,
                        )),
                        recipes::field_bare(
                            pal,
                            "glyph name",
                            app.reference_buf.clone(),
                            |app, value| app.reference_buf = value,
                            |app, value| app.reference_buf = value,
                        )
                        .flex(1.0),
                    ),
                ),
            ),
        ),
    )
}

pub(crate) fn mark_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let current = app
        .selected
        .and_then(|i| app.font.glyphs.get(i))
        .and_then(|g| g.mark.clone());
    let swatch = |mark_label: Option<String>, color: xilem::Color| {
        let selected = mark_label == current;
        let clear = mark_label.is_none();
        sized_box(recipes::mark_swatch(
            pal,
            color,
            selected,
            clear,
            move |app: &mut Workspace| {
                app.set_mark(mark_label.clone());
            },
        ))
        .dims(Dimensions::fixed(
            ControlSize::Row.length(),
            ControlSize::Row.length(),
        ))
    };
    let marks: Vec<_> = app
        .palette
        .mark_list()
        .into_iter()
        .map(|(name, color)| swatch(Some(name), color))
        .collect();
    recipes::section(
        app,
        "Mark",
        "Color",
        xcolumn(
            Region::Form,
            (
                xrow(Region::Inline, (swatch(None, pal.panel),)),
                xrow(Region::List, marks),
            ),
        ),
    )
}

/// The font's metadata, shown when no glyph is picked.
pub(crate) fn font_info_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    // The names and the design metrics; the export and hinting numbers go to
    // Advanced below.
    let rows: Vec<_> = app
        .font
        .info_rows()
        .into_iter()
        .take(8)
        .map(|(name, value)| recipes::kv(pal, name.to_string(), value))
        .collect();
    recipes::section_with_header_height(
        app,
        "Font info",
        "Font info",
        xcolumn(Region::List, rows),
        if matches!(app.mode, Mode::Graph) && app.collapsed.contains("Font info") {
            design::NODE_VIEW_SECTION_HEADER_HEIGHT
        } else {
            ControlSize::Row.px()
        },
    )
}

/// The typo, hhea and win metrics: real, but not an overview of the
/// font, so the section starts collapsed.
pub(crate) fn font_advanced_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let rows: Vec<_> = app
        .font
        .info_rows()
        .into_iter()
        .skip(8)
        .map(|(name, value)| recipes::kv(pal, name.to_string(), value))
        .collect();
    recipes::section(app, "Advanced", "Advanced", xcolumn(Region::List, rows))
}
