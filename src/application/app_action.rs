// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `AppAction`: the workspace-level actions that a shortcut or a menu item can fire.
//!
//! The action table in `actions.rs` binds keys and menu positions to these values.
//! The type is in its own module below `widgets`, because the shortcut and menu widgets name it.

use crate::application::workspace::Tool;

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
    #[cfg(target_arch = "wasm32")]
    DownloadSources,
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
    /// Show the open glyph in each joining context, in the Proof section and the reading proof.
    JoiningContexts,
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
