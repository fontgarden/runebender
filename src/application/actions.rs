// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A native menu bar, and one action list behind both it and the keymap.
//!
//! This is the piece Xilem cannot supply. Xilem is built on winit, which
//! has no menu API of any kind, so Masonry never inherited menus, and a
//! font editor with no File menu is not a font editor anyone will use.
//!
//! An action is a plain value, a table binds a key and a menu position to
//! it, and both paths run the same code. This file adds the plumbing Xilem
//! does not own:
//!
//! - The menu bar is built with `muda`, on the main thread, the first
//!   time the view function runs. On macOS `init_for_nsapp` attaches to
//!   the application rather than a window, so no window handle is needed,
//!   which matters because Xilem does not hand one out.
//! - Menu clicks do not arrive through winit's event loop. They land on
//!   muda's own global channel, so a `task` view drains that channel on
//!   the runtime and posts each one back into the application.
//!
//! Linux and Windows use the in-window menu bar drawn with Masonry's layer
//! system. They do not initialize `muda` or its global event channel.

use crate::application::workspace::{AppState, Mode, Sel, Sort, Tool};
use masonry::core::keyboard::{Key, Modifiers, NamedKey};

/// Workspace-level actions a shortcut or a menu item can fire.
// Some variants are only constructed by the menu table, which the
// native menu bar reads, and that exists on macOS.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the menu table is read by the native menu bar, which is macOS only"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppAction {
    Quit,
    OpenFont,
    SaveAs,
    RevertToSaved,
    ExportFont,
    Save,
    Undo,
    Redo,
    Overview,
    Tool(Tool),
    /// Hold Space to pan in a chrome-free filled preview.
    BeginSpacePan,
    /// Restore the selected tool and editing chrome when Space is released.
    EndSpacePan,
    FlipHorizontal,
    FlipVertical,
    Rotate90,
    RotateRight,
    Rotate180,
    RemoveOverlap,
    BooleanUnion,
    BooleanSubtract,
    BooleanIntersect,
    BooleanExclude,
    Decompose,
    Duplicate,
    DuplicateRepeat,
    ReverseContours,
    SetStartPoint,
    TidyPaths,
    AddExtremes,
    RoundCoordinates,
    CorrectPathDirection,
    HyperToCubic,
    MetaballsToCubic,
    MetaballsToHyperbezier,
    FontMetaballsToCubic,
    QuadsToCubics,
    CubicsToQuads,
    RoundCorners,
    Harmonize,
    Balance,
    Optimize,
    FilterOffset,
    FilterExtrude,
    FilterRoughen,
    FilterSlant,
    Copy,
    Paste,
    CopySelectedGlyphs,
    SelectAll,
    DeselectAll,
    InvertSelection,
    NewFont,
    CycleTheme,
    Theme(&'static str),
    /// Flip the native window blur the current theme asks for, without restarting.
    ToggleTranslucency,
    ZoomToFit,
    ShowAllMasters,
    NextMaster,
    PreviousMaster,
    NextSampleString,
    PreviousSampleString,
    /// Go to the next glyph with the purple mark, a draft the designer has not graded.
    NextDraft,
    PreviousDraft,
    /// Give the open glyph a grade, then go to the next draft.
    GradeRed,
    GradeOrange,
    GradeYellow,
    GradeGreen,
    GridDots,
    GridLines,
    MeasureColorize,
    MeasureHandles,
    MeasureSegments,
    MeasureSizes,
    MeasureSpans,
    MeasureSideBearings,
    MeasurePopcount,
    MeasureAllOn,
    MeasureAllOff,
    /// The Graph menu: the canvas, a new file, save it, run it.
    GraphTab,
    GraphNew,
    GraphOpen,
    GraphSave,
    GraphRun,
    /// Add every glyph the selected coverage filter is missing.
    GenerateMissing,
    NewGlyph,
    DuplicateGlyph,
    RemoveGlyph,
    UpdateMetrics,
    Reinterpolate,
    CheckJoining,
    ComposeFromAnchors,
    BakeMasks,
    ExportGlyphSvg,
    TraceImage,
    BoldenWithModel,
    PlaceImage,
    ImportSvg,
    RemoveImage,
    /// The grid's order in the View menu.
    SortByName,
    SortByUnicode,
}

/// One row of the application's action table.
///
/// The same row supplies the menu item's title, its accelerator label,
/// and the action both the menu and the keymap fire. They cannot drift,
/// because there is one of them.
// Only the native menu bar reads the table, and that exists on macOS.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the menu table is read by the native menu bar, which is macOS only"
    )
)]
#[derive(Clone, Copy)]
pub(crate) struct Entry {
    /// Which menu it belongs under.
    pub menu: &'static str,
    /// The item's title.
    pub title: &'static str,
    /// An accelerator in muda's syntax, if it has one.
    pub accelerator: Option<&'static str>,
    /// What it does.
    pub action: AppAction,
}

/// One row of `ACTIONS`.
const fn item(
    menu: &'static str,
    title: &'static str,
    accelerator: Option<&'static str>,
    action: AppAction,
) -> Entry {
    Entry {
        menu,
        title,
        accelerator,
        action,
    }
}

impl Entry {
    /// Whether a visual group begins before this row.
    pub(crate) fn separator_before(&self) -> bool {
        use AppAction as A;
        (self.menu == "Filter" && self.action == A::AddExtremes)
            || matches!(
                self.action,
                A::Save
                    | A::ExportFont
                    | A::GraphRun
                    | A::Copy
                    | A::SelectAll
                    | A::UpdateMetrics
                    | A::CheckJoining
                    | A::TraceImage
                    | A::Decompose
                    | A::CorrectPathDirection
                    | A::RemoveOverlap
                    | A::FlipHorizontal
                    | A::Harmonize
                    | A::HyperToCubic
                    | A::SortByName
                    | A::NextMaster
                    | A::ShowAllMasters
                    | A::NextSampleString
                    | A::NextDraft
                    | A::GridDots
                    | A::MeasureColorize
                    | A::Theme(_)
                    | A::ToggleTranslucency
                    | A::MeasurePopcount
                    | A::MeasureAllOn
            )
    }

    /// Nested menu shared by native and in-window renderers.
    pub(crate) fn submenu(&self) -> Option<&'static str> {
        use AppAction as A;
        match self.action {
            A::MetaballsToCubic | A::MetaballsToHyperbezier | A::FontMetaballsToCubic => {
                Some("Metaballs")
            }
            A::GridDots | A::GridLines => Some("Grid"),
            A::Theme(_) | A::ToggleTranslucency => Some("Theme"),
            A::MeasureColorize
            | A::MeasureHandles
            | A::MeasureSegments
            | A::MeasureSizes
            | A::MeasureSpans
            | A::MeasureSideBearings
            | A::MeasurePopcount
            | A::MeasureAllOn
            | A::MeasureAllOff => Some("Measure"),
            _ => None,
        }
    }

    /// Whether the command can change the current workspace.
    pub(crate) fn enabled(&self, app: &AppState) -> bool {
        use AppAction as A;
        let Some(app) = app.workspace.as_ref() else {
            return matches!(
                self.action,
                A::Quit | A::NewFont | A::OpenFont | A::Theme(_) | A::ToggleTranslucency
            );
        };
        let editor = matches!(app.mode, Mode::Editor(_));
        if editor && app.interpolation_is_active() {
            if !crate::application::editor::tools::interpolation::command_allowed(self.action) {
                return false;
            }
            if matches!(self.action, A::DeselectAll | A::InvertSelection) {
                return !app.interpolation.selected.is_empty();
            }
        }
        match self.action {
            A::Save => app.untitled || (app.modified && app.font.is_writable()),
            A::RevertToSaved => !app.untitled,
            A::ExportFont => app.export_job.is_none(),
            A::Undo if app.proof_view && app.review.pencil => app
                .review
                .annotations
                .lock()
                .unwrap()
                .can_undo(&crate::application::widgets::proof_pencil::prefix(app)),
            A::Redo if app.proof_view && app.review.pencil => false,
            A::Undo => match app.mode {
                Mode::Editor(_) => app.can_editor_history_step(false),
                Mode::Overview => app.can_overview_history_step(false),
                Mode::Graph => {
                    crate::application::editor::tools::graph::controls::live_graph_can_step(
                        app, false,
                    ) || app.can_overview_history_step(false)
                }
            },
            A::Redo => match app.mode {
                Mode::Editor(_) => app.can_editor_history_step(true),
                Mode::Overview => app.can_overview_history_step(true),
                Mode::Graph => {
                    crate::application::editor::tools::graph::controls::live_graph_can_step(
                        app, true,
                    ) || app.can_overview_history_step(true)
                }
            },
            A::MetaballsToCubic | A::MetaballsToHyperbezier => editor,
            A::FontMetaballsToCubic => matches!(app.mode, Mode::Overview),
            A::Copy | A::SelectAll => editor,
            A::Paste => editor && !app.clipboard.is_empty(),
            A::CopySelectedGlyphs => app.selected.is_some() || !app.multi_selected.is_empty(),
            A::DeselectAll
            | A::InvertSelection
            | A::SetStartPoint
            | A::RoundCorners
            | A::Harmonize
            | A::Balance
            | A::Optimize => editor && app.selected_points() > 0,
            A::FlipHorizontal
            | A::FlipVertical
            | A::Rotate90
            | A::RotateRight
            | A::Rotate180
            | A::RemoveOverlap
            | A::BooleanUnion
            | A::BooleanSubtract
            | A::BooleanIntersect
            | A::BooleanExclude
            | A::Decompose
            | A::Duplicate
            | A::DuplicateRepeat
            | A::ReverseContours
            | A::TidyPaths
            | A::AddExtremes
            | A::RoundCoordinates
            | A::CorrectPathDirection
            | A::HyperToCubic
            | A::QuadsToCubics
            | A::CubicsToQuads
            | A::ZoomToFit => editor,
            A::FilterOffset | A::FilterExtrude | A::FilterRoughen | A::FilterSlant => editor,
            A::GenerateMissing => matches!(app.sel, Sel::Filter(_)),
            A::DuplicateGlyph | A::RemoveGlyph | A::BakeMasks | A::ExportGlyphSvg => {
                app.selected.is_some()
            }
            A::TraceImage | A::PlaceImage | A::ImportSvg => editor,
            A::BoldenWithModel => editor && app.ai.job.is_none(),
            A::RemoveImage => editor && app.session.has_image(),
            A::Reinterpolate => app.selected.is_some() && app.font.master_count() > 1,
            A::NextMaster | A::PreviousMaster => app.font.master_count() > 1,
            A::ShowAllMasters | A::NextSampleString | A::PreviousSampleString => editor,
            // The plain keys 1 to 4 and the brackets grade only on the proof page.
            A::NextDraft
            | A::PreviousDraft
            | A::GradeRed
            | A::GradeOrange
            | A::GradeYellow
            | A::GradeGreen => editor && app.proof_view,
            A::GraphSave => app.graph.open.is_some(),
            // Always available while a font is open.
            // The match has no wildcard, so a new action must choose its arm.
            A::Quit
            | A::OpenFont
            | A::SaveAs
            | A::Overview
            | A::Tool(_)
            | A::BeginSpacePan
            | A::EndSpacePan
            | A::NewFont
            | A::CycleTheme
            | A::Theme(_)
            | A::ToggleTranslucency
            | A::GridDots
            | A::GridLines
            | A::MeasureColorize
            | A::MeasureHandles
            | A::MeasureSegments
            | A::MeasureSizes
            | A::MeasureSpans
            | A::MeasureSideBearings
            | A::MeasurePopcount
            | A::MeasureAllOn
            | A::MeasureAllOff
            | A::GraphTab
            | A::GraphNew
            | A::GraphOpen
            | A::GraphRun
            | A::NewGlyph
            | A::UpdateMetrics
            | A::CheckJoining
            | A::ComposeFromAnchors
            | A::SortByName
            | A::SortByUnicode => true,
        }
    }

    /// Checked state for choice and toggle commands; `None` for ordinary rows.
    pub(crate) fn checked(&self, app: &AppState) -> Option<bool> {
        use AppAction as A;
        let workspace = app.workspace.as_ref();
        match self.action {
            A::SortByName => workspace.map(|app| app.sort == Sort::Name),
            A::SortByUnicode => workspace.map(|app| app.sort == Sort::Unicode),
            A::Theme(id) => Some(app.theme_id == id),
            A::ToggleTranslucency => Some(app.palette.blur_background),
            A::ShowAllMasters => workspace.map(|app| app.show_all_masters),
            A::GridDots => workspace.map(|app| !app.view.grid_lines),
            A::GridLines => workspace.map(|app| app.view.grid_lines),
            A::MeasureColorize => workspace.map(|app| app.view.colorize),
            A::MeasureHandles => workspace.map(|app| app.view.handles),
            A::MeasureSegments => workspace.map(|app| app.view.segments),
            A::MeasureSizes => workspace.map(|app| app.view.sizes),
            A::MeasureSpans => workspace.map(|app| app.view.spans),
            A::MeasureSideBearings => workspace.map(|app| app.view.bearings),
            A::MeasurePopcount => workspace.map(|app| app.view.popcount),
            A::Tool(tool) => workspace.map(|app| app.tool == tool),
            _ => None,
        }
    }
}

/// Every action the application exposes, in menu order.
///
/// Each row is one `item`: menu, title, accelerator and action.
/// The rows stay one per line, so the table is not formatted.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the menu table is read by the native menu bar, which is macOS only"
    )
)]
#[rustfmt::skip]
pub(crate) const ACTIONS: &[Entry] = {
    use AppAction as A;
    &[
        item("Runebender", "Quit Runebender", Some("CmdOrCtrl+Q"), A::Quit),
        item("File", "New Font", Some("CmdOrCtrl+N"), A::NewFont),
        item("File", "Open…", Some("CmdOrCtrl+O"), A::OpenFont),
        item("File", "Save", Some("CmdOrCtrl+S"), A::Save),
        item("File", "Save As…", Some("CmdOrCtrl+Shift+S"), A::SaveAs),
        item("File", "Revert to Saved…", None, A::RevertToSaved),
        item("File", "Export…", Some("CmdOrCtrl+Alt+E"), A::ExportFont),
        item("Graph", "New Graph", None, A::GraphNew),
        item("Graph", "Open Graph…", None, A::GraphOpen),
        item("Graph", "Save Graph", None, A::GraphSave),
        item("Graph", "Run Graph", Some("CmdOrCtrl+Enter"), A::GraphRun),
        item("Edit", "Undo", Some("CmdOrCtrl+Z"), A::Undo),
        item("Edit", "Redo", Some("CmdOrCtrl+Shift+Z"), A::Redo),
        item("Edit", "Copy", Some("CmdOrCtrl+C"), A::Copy),
        item("Edit", "Paste", Some("CmdOrCtrl+V"), A::Paste),
        item("Edit", "Copy Selected Glyphs as Text", None, A::CopySelectedGlyphs),
        item("Edit", "Select All", Some("CmdOrCtrl+A"), A::SelectAll),
        item("Edit", "Deselect All", Some("CmdOrCtrl+Alt+A"), A::DeselectAll),
        item("Edit", "Invert Selection", Some("CmdOrCtrl+Alt+Shift+I"), A::InvertSelection),
        item("Glyph", "New Glyph", None, A::NewGlyph),
        item("Glyph", "Duplicate Glyph", None, A::DuplicateGlyph),
        item("Glyph", "Remove Glyph", None, A::RemoveGlyph),
        item("Glyph", "Update Metrics", None, A::UpdateMetrics),
        item("Glyph", "Reinterpolate", None, A::Reinterpolate),
        item("Glyph", "Generate Missing Glyphs", None, A::GenerateMissing),
        item("Glyph", "Decompose Components", None, A::Decompose),
        item("Glyph", "Check Joining", None, A::CheckJoining),
        item("Glyph", "Compose from Anchors", None, A::ComposeFromAnchors),
        item("Glyph", "Bake Masks", None, A::BakeMasks),
        item("Glyph", "Export Glyph as SVG", None, A::ExportGlyphSvg),
        item("Glyph", "Trace Image…", None, A::TraceImage),
        item("Glyph", "Bolden With Model…", None, A::BoldenWithModel),
        item("Glyph", "Place Image…", None, A::PlaceImage),
        item("Glyph", "Import SVG…", None, A::ImportSvg),
        item("Glyph", "Remove Image", None, A::RemoveImage),
        item("Glyph", "Next Draft", Some("]"), A::NextDraft),
        item("Glyph", "Previous Draft", Some("["), A::PreviousDraft),
        item("Glyph", "Grade Red", Some("1"), A::GradeRed),
        item("Glyph", "Grade Orange", Some("2"), A::GradeOrange),
        item("Glyph", "Grade Yellow", Some("3"), A::GradeYellow),
        item("Glyph", "Grade Green", Some("4"), A::GradeGreen),
        item("Path", "Tidy Up Paths", Some("CmdOrCtrl+Shift+T"), A::TidyPaths),
        item("Path", "Add Extremes", None, A::AddExtremes),
        item("Path", "Round Coordinates", None, A::RoundCoordinates),
        item("Path", "Correct Path Direction", Some("CmdOrCtrl+Shift+R"), A::CorrectPathDirection),
        item("Path", "Reverse Contours", Some("CmdOrCtrl+Alt+Shift+R"), A::ReverseContours),
        item("Path", "Set Start Point", None, A::SetStartPoint),
        item("Path", "Remove Overlap", Some("CmdOrCtrl+Shift+O"), A::RemoveOverlap),
        item("Path", "Union", None, A::BooleanUnion),
        item("Path", "Subtract", None, A::BooleanSubtract),
        item("Path", "Intersect", None, A::BooleanIntersect),
        item("Path", "Exclude", None, A::BooleanExclude),
        item("Path", "Flip Horizontal", Some("CmdOrCtrl+Shift+H"), A::FlipHorizontal),
        item("Path", "Flip Vertical", Some("CmdOrCtrl+Shift+V"), A::FlipVertical),
        item("Path", "Rotate 90° Left", None, A::Rotate90),
        item("Path", "Rotate 90° Right", None, A::RotateRight),
        item("Path", "Rotate 180°", None, A::Rotate180),
        item("Path", "Duplicate Selection", Some("CmdOrCtrl+D"), A::Duplicate),
        item("Path", "Duplicate + Repeat", Some("CmdOrCtrl+Shift+D"), A::DuplicateRepeat),
        item("Path", "Harmonize", None, A::Harmonize),
        item("Path", "Balance", None, A::Balance),
        item("Path", "Optimize", None, A::Optimize),
        item("Path", "Metaballs to Hyperbezier", None, A::MetaballsToHyperbezier),
        item("Path", "Metaballs to Cubic", None, A::MetaballsToCubic),
        item("Path", "Font to Cubic (Current Master, Overview)", None, A::FontMetaballsToCubic),
        item("", "Metaball", None, A::Tool(Tool::Metaball)),
        item("Path", "Hyperbezier to Cubic", None, A::HyperToCubic),
        item("Path", "Quadratic to Cubic", None, A::QuadsToCubics),
        item("Path", "Cubic to Quadratic", None, A::CubicsToQuads),
        item("Filter", "Offset Curve", None, A::FilterOffset),
        item("Filter", "Extrude", None, A::FilterExtrude),
        item("Filter", "Roughen", None, A::FilterRoughen),
        item("Filter", "Slanter", None, A::FilterSlant),
        item("Filter", "Round Corners", None, A::RoundCorners),
        item("Filter", "Add Extremes", None, A::AddExtremes),
        item("Filter", "Remove Overlap", None, A::RemoveOverlap),
        item("View", "Zoom to Fit", Some("CmdOrCtrl+0"), A::ZoomToFit),
        item("View", "Show All Masters", None, A::ShowAllMasters),
        item("View", "Sort Glyphs by Name", None, A::SortByName),
        item("View", "Sort Glyphs by Unicode", None, A::SortByUnicode),
        item("View", "Next Master", None, A::NextMaster),
        item("View", "Previous Master", None, A::PreviousMaster),
        item("View", "Next Sample String", None, A::NextSampleString),
        item("View", "Previous Sample String", None, A::PreviousSampleString),
        item("View", "Dots", None, A::GridDots),
        item("View", "Lines", None, A::GridLines),
        item("View", "Colorize Outline", None, A::MeasureColorize),
        item("View", "Handle Lengths", None, A::MeasureHandles),
        item("View", "Segment Lengths", None, A::MeasureSegments),
        item("View", "Segment Sizes", None, A::MeasureSizes),
        item("View", "Stems & Counters", None, A::MeasureSpans),
        item("View", "Side Bearings", None, A::MeasureSideBearings),
        item("View", "Popcount Sums", None, A::MeasurePopcount),
        item("View", "All On", None, A::MeasureAllOn),
        item("View", "All Off", None, A::MeasureAllOff),
    // Shortcut-only commands. GPUI exposes tools in the chrome rather than a
    // top-level Tools menu, and Escape returns to the overview without a row.
        item("", "Overview", Some("Escape"), A::Overview),
        item("", "Cycle Theme", Some("CmdOrCtrl+T"), A::CycleTheme),
        item("", "Show Graph", None, A::GraphTab),
        item("", "Select", Some("V"), A::Tool(Tool::Select)),
        item("", "Hand", None, A::Tool(Tool::Hand)),
        item("", "Pen", Some("P"), A::Tool(Tool::Pen)),
        item("", "Hyper Pen", Some("B"), A::Tool(Tool::HyperPen)),
        item("", "Rectangle", Some("U"), A::Tool(Tool::Rect)),
        item("", "Ellipse", Some("O"), A::Tool(Tool::Ellipse)),
        item("", "Local AI Shapes", None, A::Tool(Tool::AiShape)),
        item("", "Knife", Some("E"), A::Tool(Tool::Knife)),
        item("", "Measure", Some("M"), A::Tool(Tool::Measure)),
        item("", "Edit Interpolation", Some("I"), A::Tool(Tool::Interpolation)),
        item("", "Label", None, A::Tool(Tool::Label)),
    ]
};

/// The order menus appear in the bar.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the menu table is read by the native menu bar, which is macOS only"
    )
)]
/// The menu table with built-in and local theme rows supplied by the catalog.
pub(crate) fn actions() -> &'static [Entry] {
    static ALL: std::sync::OnceLock<Vec<Entry>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut entries = ACTIONS.to_vec();
        let catalog = crate::application::platform::themes::catalog();
        for id in catalog.ids() {
            let theme = catalog.get(id).expect("catalog id has a theme");
            entries.push(Entry {
                menu: "View",
                title: &theme.name,
                accelerator: None,
                action: AppAction::Theme(id),
            });
        }
        entries.push(Entry {
            menu: "View",
            title: "Translucent Window",
            accelerator: None,
            action: AppAction::ToggleTranslucency,
        });
        entries
    })
}

pub(crate) const MENUS: &[&str] = &[
    "Runebender",
    "File",
    "Graph",
    "Edit",
    "Glyph",
    "Path",
    "Filter",
    "View",
];

/// Resolve a key press from the same accelerator metadata used by both menu bars.
pub(crate) fn action_for_key(key: &Key, modifiers: Modifiers) -> Option<AppAction> {
    action_for_key_impl(key, modifiers, false)
}

/// Resolve a key for the in-window menu, including commands owned by the native
/// application menu when the same code is exercised on macOS.
pub(crate) fn action_for_key_in_window(key: &Key, modifiers: Modifiers) -> Option<AppAction> {
    action_for_key_impl(key, modifiers, true)
}

fn action_for_key_impl(
    key: &Key,
    modifiers: Modifiers,
    _include_platform_commands: bool,
) -> Option<AppAction> {
    actions().iter().find_map(|entry| {
        #[cfg(target_os = "macos")]
        if entry.action == AppAction::Quit && !_include_platform_commands {
            // The native application menu owns Cmd-Q on macOS.
            return None;
        }
        let accelerator = entry.accelerator?;
        let command = modifiers.meta() || modifiers.ctrl();
        let wants_command = accelerator.contains("CmdOrCtrl+");
        let wants_shift = accelerator.contains("Shift+");
        let wants_alt = accelerator.contains("Alt+");
        if command != wants_command
            || modifiers.shift() != wants_shift
            || modifiers.alt() != wants_alt
        {
            return None;
        }
        let expected = accelerator.rsplit('+').next()?;
        let matches = if expected == "Escape" {
            matches!(key, Key::Named(NamedKey::Escape))
        } else if expected == "Enter" {
            matches!(key, Key::Named(NamedKey::Enter))
        } else {
            matches!(key, Key::Character(c) if c.eq_ignore_ascii_case(expected))
        };
        matches.then_some(entry.action)
    })
}

/// Whether `action` is currently available, using its shared menu predicate.
pub(crate) fn action_enabled(action: AppAction, app: &AppState) -> bool {
    if action == AppAction::BeginSpacePan {
        return app
            .workspace
            .as_ref()
            .is_some_and(|workspace| matches!(workspace.mode, Mode::Editor(_)));
    }
    if action == AppAction::EndSpacePan {
        return app.workspace.is_some();
    }
    actions()
        .iter()
        .find(|entry| entry.action == action)
        .is_some_and(|entry| entry.enabled(app))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn accelerator_metadata_is_the_shortcut_map() {
        assert_eq!(
            action_for_key(&Key::Character("s".into()), Modifiers::META),
            Some(AppAction::Save)
        );
        assert_eq!(
            action_for_key(&Key::Character("p".into()), Modifiers::empty()),
            Some(AppAction::Tool(Tool::Pen))
        );
        assert_eq!(
            action_for_key(&Key::Named(NamedKey::Escape), Modifiers::empty()),
            Some(AppAction::Overview)
        );
    }

    #[test]
    fn accelerators_are_unique_and_every_row_has_a_known_home() {
        let mut accelerators = HashSet::new();
        for entry in actions() {
            assert!(
                entry.menu.is_empty() || MENUS.contains(&entry.menu),
                "unknown menu for {}: {}",
                entry.title,
                entry.menu
            );
            if let Some(accelerator) = entry.accelerator {
                assert!(
                    accelerators.insert(accelerator),
                    "duplicate accelerator: {accelerator}"
                );
            }
        }
    }

    #[test]
    fn theme_menu_matches_the_catalog_once_in_catalog_order() {
        let catalog = crate::application::platform::themes::catalog();
        let rows: Vec<_> = actions()
            .iter()
            .filter_map(|entry| match entry.action {
                AppAction::Theme(id) => Some((id, entry)),
                _ => None,
            })
            .collect();
        assert_eq!(rows.len(), catalog.ids().len());
        let mut seen = HashSet::new();
        for (&(id, entry), expected) in rows.iter().zip(catalog.ids()) {
            assert_eq!(id, expected.as_str());
            assert!(seen.insert(id), "duplicate theme menu row: {id}");
            let theme = catalog.get(id).expect("menu theme is installed");
            assert_eq!(entry.title, theme.name);
            assert_eq!(entry.menu, "View");
            assert_eq!(entry.submenu(), Some("Theme"));
        }
        let builtin_ids = runebender::ui::theme::BUILTIN_THEME_IDS;
        assert_eq!(
            catalog.ids()[..builtin_ids.len()]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            builtin_ids,
            "built-in theme order must precede locally installed themes"
        );
        let (_, dark_gray) = rows
            .iter()
            .find(|(id, _)| *id == "dark-gray")
            .expect("Dark Gray is listed");
        assert_eq!(dark_gray.title, "Dark Gray");
    }

    #[test]
    fn pristine_starter_can_be_saved_but_cannot_revert_to_a_nonexistent_file() {
        let app = AppState::open(None);
        let enabled = |action| {
            actions()
                .iter()
                .find(|entry| entry.action == action)
                .unwrap()
                .enabled(&app)
        };
        assert!(enabled(AppAction::Save));
        assert!(enabled(AppAction::OpenFont));
        assert!(!enabled(AppAction::RevertToSaved));
    }

    #[test]
    fn welcome_state_keeps_application_commands_and_disables_document_commands() {
        let mut app = AppState::open(None);
        app.workspace = None;
        let entry = |action| {
            actions()
                .iter()
                .find(|entry| entry.action == action)
                .expect("the action is in the command table")
        };

        assert!(entry(AppAction::Quit).enabled(&app));
        assert!(entry(AppAction::NewFont).enabled(&app));
        assert!(entry(AppAction::OpenFont).enabled(&app));
        assert!(entry(AppAction::Theme("gray")).enabled(&app));
        assert!(entry(AppAction::Theme("gray")).checked(&app).unwrap());
        assert!(!entry(AppAction::Save).enabled(&app));
        assert!(!entry(AppAction::Undo).enabled(&app));
        assert!(!entry(AppAction::ZoomToFit).enabled(&app));
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::cell::RefCell;
    use std::sync::OnceLock;

    use muda::accelerator::Accelerator;
    use muda::{CheckMenuItem, Menu, MenuId, MenuItem, MenuItemKind, Submenu};

    use super::{AppAction, MENUS, actions};

    /// A native menu bar with its items and their last applied states, in action order.
    type InstalledMenu = (Menu, Vec<MenuItemKind>, Vec<(bool, bool)>);

    thread_local! {
        /// The menu is built once and held for the life of the process:
        /// dropping it would take the menu bar with it. It is not `Send`,
        /// which is fine, because it is only ever touched from the main
        /// thread, and it is also why this cannot be a `static`.
        /// The bar, its items in action order, and the enabled and checked state each
        /// item was last given, so a rebuild only calls into AppKit for what changed.
        static MENU: RefCell<Option<InstalledMenu>> = const { RefCell::new(None) };
        static UPDATE_ITEM: RefCell<Option<MenuItem>> = const { RefCell::new(None) };
    }
    /// Menu item ids, in the same order as [`actions()`]. These are plain
    /// strings, so the event pump on another thread can read them.
    static IDS: OnceLock<Vec<MenuId>> = OnceLock::new();
    /// Custom Quit item id; predefined `AppKit` Quit would bypass dirty-state checks.
    static QUIT_ID: OnceLock<MenuId> = OnceLock::new();
    static UPDATE_ID: OnceLock<MenuId> = OnceLock::new();

    /// Text-editing accelerators stay in Masonry's event path so the focused
    /// `TextArea` gets first refusal. Installing the same equivalents on the
    /// native items would make `AppKit` consume them before `winit` sees the key.
    fn accelerator(entry: &super::Entry) -> Option<Accelerator> {
        if matches!(
            entry.action,
            AppAction::Undo
                | AppAction::Redo
                | AppAction::Copy
                | AppAction::Paste
                | AppAction::SelectAll
                // Digits and brackets are typed into fields, so a focused field
                // gets them first.
                | AppAction::NextDraft
                | AppAction::PreviousDraft
                | AppAction::GradeRed
                | AppAction::GradeOrange
                | AppAction::GradeYellow
                | AppAction::GradeGreen
        ) {
            None
        } else {
            entry
                .accelerator
                .and_then(|text| text.parse::<Accelerator>().ok())
        }
    }

    /// Builds the menu bar and attaches it to the application.
    ///
    /// Must run on the main thread, which is where the view function runs.
    /// Later calls update enabled and checked state without rebuilding the bar.
    pub(crate) fn install(app: &crate::application::workspace::AppState) {
        UPDATE_ITEM.with(|slot| {
            if let Some(item) = slot.borrow().as_ref() {
                item.set_enabled(crate::application::platform::updates::can_check());
            }
        });
        if IDS.get().is_some() {
            MENU.with(|slot| {
                if let Some((_, items, states)) = slot.borrow_mut().as_mut() {
                    for ((entry, item), state) in actions()
                        .iter()
                        .filter(|entry| entry.menu != "Runebender" && MENUS.contains(&entry.menu))
                        .zip(items)
                        .zip(states)
                    {
                        let wanted = (entry.enabled(app), entry.checked(app).unwrap_or(false));
                        if *state == wanted {
                            continue;
                        }
                        match item {
                            MenuItemKind::MenuItem(item) => item.set_enabled(wanted.0),
                            MenuItemKind::Check(item) => {
                                item.set_enabled(wanted.0);
                                item.set_checked(wanted.1);
                            }
                            _ => {}
                        }
                        *state = wanted;
                    }
                }
            });
            return;
        }
        let bar = Menu::new();
        let mut ids = Vec::with_capacity(actions().len());
        let mut items = Vec::with_capacity(actions().len());
        let mut states = Vec::with_capacity(actions().len());
        // The first submenu on macOS is the application menu, and it is
        // where the platform expects Quit to live.
        let app_menu = Submenu::new("Runebender", true);
        let _ = app_menu.append(&muda::PredefinedMenuItem::about(
            None,
            Some(muda::AboutMetadata {
                name: Some("Runebender".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                ..Default::default()
            }),
        ));
        let updates = MenuItem::new("Check for Updates…", false, None);
        let _ = UPDATE_ID.set(updates.id().clone());
        let _ = app_menu.append(&updates);
        UPDATE_ITEM.with(|slot| *slot.borrow_mut() = Some(updates));
        let _ = app_menu.append(&muda::PredefinedMenuItem::separator());
        let _ = app_menu.append(&muda::PredefinedMenuItem::hide(None));
        let _ = app_menu.append(&muda::PredefinedMenuItem::separator());
        let quit_entry = actions()
            .iter()
            .find(|entry| entry.action == AppAction::Quit)
            .expect("the action table has Quit");
        let quit = MenuItem::new(quit_entry.title, true, accelerator(quit_entry));
        let _ = QUIT_ID.set(quit.id().clone());
        let _ = app_menu.append(&quit);
        let _ = bar.append(&app_menu);

        for name in MENUS.iter().filter(|name| **name != "Runebender") {
            let submenu = Submenu::new(*name, true);
            let menu_entries: Vec<_> = actions()
                .iter()
                .filter(|entry| entry.menu == *name)
                .collect();
            let mut index = 0;
            while index < menu_entries.len() {
                let entry = menu_entries[index];
                if let Some(group) = entry.submenu() {
                    if entry.separator_before() {
                        let _ = submenu.append(&muda::PredefinedMenuItem::separator());
                    }
                    let nested = Submenu::new(group, true);
                    let mut first = true;
                    while index < menu_entries.len() && menu_entries[index].submenu() == Some(group)
                    {
                        let entry = menu_entries[index];
                        if !first && entry.separator_before() {
                            let _ = nested.append(&muda::PredefinedMenuItem::separator());
                        }
                        first = false;
                        let accelerator = accelerator(entry);
                        let state = (entry.enabled(app), entry.checked(app).unwrap_or(false));
                        let item = CheckMenuItem::new(entry.title, state.0, state.1, accelerator);
                        states.push(state);
                        ids.push(item.id().clone());
                        let _ = nested.append(&item);
                        items.push(MenuItemKind::Check(item));
                        index += 1;
                    }
                    let _ = submenu.append(&nested);
                    continue;
                }
                if entry.separator_before() {
                    let _ = submenu.append(&muda::PredefinedMenuItem::separator());
                }
                let accelerator = accelerator(entry);
                let state = (entry.enabled(app), entry.checked(app).unwrap_or(false));
                let item = if entry.checked(app).is_some() {
                    MenuItemKind::Check(CheckMenuItem::new(
                        entry.title,
                        state.0,
                        state.1,
                        accelerator,
                    ))
                } else {
                    MenuItemKind::MenuItem(MenuItem::new(entry.title, state.0, accelerator))
                };
                states.push(state);
                ids.push(item.id().clone());
                match &item {
                    MenuItemKind::MenuItem(item) => {
                        let _ = submenu.append(item);
                    }
                    MenuItemKind::Check(item) => {
                        let _ = submenu.append(item);
                    }
                    _ => unreachable!("Runebender creates only normal and check menu items"),
                }
                items.push(item);
                index += 1;
            }
            let _ = bar.append(&submenu);
        }
        // On macOS the bar belongs to the application, not to a window,
        // so this needs no window handle. Xilem does not hand one out.
        bar.init_for_nsapp();
        MENU.with(|slot| *slot.borrow_mut() = Some((bar, items, states)));
        let _ = IDS.set(ids);
    }

    /// Update checks are an application operation rather than a document action.
    pub(super) fn is_update(id: &MenuId) -> bool {
        UPDATE_ID.get() == Some(id)
    }

    /// The action a menu id fires, if it is one of ours.
    pub(super) fn action_for(id: &MenuId) -> Option<AppAction> {
        if QUIT_ID.get() == Some(id) {
            return Some(AppAction::Quit);
        }
        let ids = IDS.get()?;
        // actions() and IDS are built together, in menu order.
        let mut index = 0;
        for name in MENUS.iter().filter(|name| **name != "Runebender") {
            for entry in actions().iter().filter(|e| e.menu == *name) {
                if ids.get(index) == Some(id) {
                    return Some(entry.action);
                }
                index += 1;
            }
        }
        None
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn focused_editing_shortcuts_stay_out_of_the_native_accelerator_scope() {
            let entry = |action| {
                actions()
                    .iter()
                    .find(|entry| entry.action == action)
                    .expect("the action has a menu row")
            };
            for action in [
                AppAction::Undo,
                AppAction::Redo,
                AppAction::Copy,
                AppAction::Paste,
                AppAction::SelectAll,
            ] {
                assert!(accelerator(entry(action)).is_none());
            }
            assert!(accelerator(entry(AppAction::Save)).is_some());
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    /// The native menu is macOS-only; [`crate::application::widgets::menu_shell`] owns the
    /// in-window menu on these platforms.
    pub(crate) fn install(_app: &crate::application::workspace::AppState) {}
}

pub(crate) use platform::install;

/// Runs `view`, and alongside it drains muda's global menu channel and
/// posts each click back into the application.
///
/// Menu events do not travel through winit's event loop, so without this
/// they never reach the widget tree at all. The pump produces no widget,
/// which is why it is forked alongside the tree rather than placed in it.
#[cfg(target_os = "macos")]
pub(crate) fn with_menu_events<V: xilem::WidgetView<AppState>>(
    view: V,
) -> impl xilem::WidgetView<AppState> + use<V> {
    use xilem::core::fork;
    use xilem::view::task;

    fork(
        view,
        task(
            |proxy: xilem::core::MessageProxy<Option<muda::MenuId>>, _: &mut AppState| async move {
                // muda calls this handler for each menu click, and the task sleeps until one
                // arrives. It used to poll muda's channel every 16 ms.
                let (clicks, mut clicked) = tokio::sync::mpsc::unbounded_channel();
                muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
                    let _ = clicks.send(event.id);
                }));
                // The updater is polled once a second. Without it there is nothing to poll,
                // and a tick would only rebuild an idle interface every second.
                let updater = cfg!(feature = "macos-updater");
                if updater && proxy.message(None).is_err() {
                    return;
                }
                loop {
                    let id = if updater {
                        let tick = std::time::Duration::from_secs(1);
                        match tokio::time::timeout(tick, clicked.recv()).await {
                            Ok(id) => id,
                            Err(_) => {
                                if proxy.message(None).is_err() {
                                    return;
                                }
                                continue;
                            }
                        }
                    } else {
                        clicked.recv().await
                    };
                    let Some(id) = id else {
                        return;
                    };
                    if proxy.message(Some(id)).is_err() {
                        return;
                    }
                }
            },
            |app: &mut AppState, id: Option<muda::MenuId>| {
                let Some(id) = id else {
                    crate::application::platform::updates::poll(app);
                    return;
                };
                if platform::is_update(&id) {
                    crate::application::platform::updates::check(app);
                } else if let Some(action) = platform::action_for(&id) {
                    app.dispatch(action);
                }
            },
        ),
    )
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn with_menu_events<V: xilem::WidgetView<AppState>>(view: V) -> V {
    view
}
