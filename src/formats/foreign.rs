// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Sources edited through a UFO working copy: a Glyphs file or a Fontra folder.
//!
//! Opening one converts it to UFOs and a designspace beside it, and the editor works on those.
//! Save writes the working copy first, then merges its edits back into the original file.
//! The merge replaces only glyph drawings, Unicode values, glyph sets and kerning that differ
//! from the original, so data Runebender does not read survives untouched.
//! The first write-back moves the original aside as `<name> (before Runebender).<ext>`.
//!
//! # Status and known gaps
//!
//! This is basic alpha support, so every major format opens and saves.
//! It was round-tripped on real Glyphs 2 and 3 files, Glyphs packages and Fontra's test fonts,
//! but nobody has yet opened a written file in Glyphs or Fontra themselves.
//! Each format module lists its own gaps: [`super::glyphs_import`], [`super::glyphs_export`]
//! and [`super::fontra`].
//! Gaps shared by both formats:
//!
//! - Every open converts again into a new sibling folder (`Name-ufo`, then
//!   `Name-ufo-import-1`, and so on), because `publish_converted_import` never replaces output.
//!   Reusing one working copy, or keeping it in a cache folder, would stop the clutter.
//! - The editor watches the working copy, not the original, so a change made to the
//!   original by another app is not reloaded.
//! - Masters or sources added in Runebender cannot be written back; `write_back` refuses
//!   and only the UFO working copy is saved.
//! - Font-wide data edited in Runebender (names, font info, OpenType values, designspace
//!   instances and rules) is not written back. Only the items listed above are merged.
//! - Non-default UFO layers (backgrounds, brace layers made in Runebender) are not written back.
//! - Save As writes UFOs; there is no "save as Glyphs" or "save as Fontra" for a font that
//!   started as UFO.
//! - These formats have not been tried in the browser host.

use std::path::{Path, PathBuf};

/// A format Runebender opens through a UFO working copy and saves back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForeignFormat {
    /// A `.glyphs` file or `.glyphspackage` folder.
    Glyphs,
    /// A `.fontra` folder.
    Fontra,
}

impl ForeignFormat {
    /// The format a path names by its extension, if it is one of these.
    pub fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "glyphs" | "glyphspackage" => Some(Self::Glyphs),
            "fontra" => Some(Self::Fontra),
            _ => None,
        }
    }
}

/// The original file behind a UFO working copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignSource {
    /// The file or folder the user opened.
    pub path: PathBuf,
    /// Its format.
    pub format: ForeignFormat,
}

/// One UFO of a working copy, as saved.
#[derive(Clone, Debug)]
pub struct WorkingSource {
    /// The UFO's file name, such as `Family-Regular.ufo`.
    pub filename: String,
    /// The designspace source name, when there is a designspace.
    pub name: Option<String>,
    /// The saved UFO.
    pub font: norad::Font,
}

/// Read the saved working copy: every full source of a designspace, or a single UFO.
pub fn read_working_copy(path: &Path) -> Result<Vec<WorkingSource>, String> {
    let is_designspace = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("designspace"));
    if !is_designspace {
        let font =
            norad::Font::load(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Ok(vec![WorkingSource {
            filename,
            name: None,
            font,
        }]);
    }
    let document = norad::designspace::DesignSpaceDocument::load(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let directory = path.parent().unwrap_or(Path::new("."));
    document
        .sources
        .iter()
        .filter(|source| source.layer.is_none())
        .map(|source| {
            let ufo = directory.join(&source.filename);
            let font =
                norad::Font::load(&ufo).map_err(|error| format!("{}: {error}", ufo.display()))?;
            Ok(WorkingSource {
                filename: source.filename.clone(),
                name: source.name.clone(),
                font,
            })
        })
        .collect()
}

/// Merge the saved working copy at `working` back into the original file.
pub fn write_back(foreign: &ForeignSource, working: &Path) -> Result<(), String> {
    let sources = read_working_copy(working)?;
    let stage = sibling(&foreign.path, |name| format!(".runebender-save-{name}"));
    remove_any(&stage)?;
    let written = match foreign.format {
        ForeignFormat::Glyphs => super::glyphs_export::write_back(&foreign.path, &sources, &stage),
        ForeignFormat::Fontra => super::fontra::write_back(&foreign.path, &sources, &stage),
    };
    if let Err(error) = written {
        let _ = remove_any(&stage);
        return Err(error);
    }
    replace_keeping_first_original(&foreign.path, &stage)
}

/// Where the first write-back keeps the original: `Font (before Runebender).glyphs`.
pub fn backup_path(original: &Path) -> PathBuf {
    sibling(original, |name| {
        let path = Path::new(name);
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        match path.extension() {
            Some(extension) => {
                format!("{stem} (before Runebender).{}", extension.to_string_lossy())
            }
            None => format!("{stem} (before Runebender)"),
        }
    })
}

fn sibling(path: &Path, name: impl FnOnce(&str) -> String) -> PathBuf {
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(name(&file_name))
}

/// Put `stage` where `original` is. The first time, the original moves to its backup path;
/// after that the previous write-back is replaced.
fn replace_keeping_first_original(original: &Path, stage: &Path) -> Result<(), String> {
    let backup = backup_path(original);
    let mut moved_aside = None;
    if original.exists() {
        if backup.exists() {
            let previous = sibling(original, |name| format!(".runebender-previous-{name}"));
            remove_any(&previous)?;
            std::fs::rename(original, &previous)
                .map_err(|error| format!("{}: {error}", original.display()))?;
            moved_aside = Some((previous, true));
        } else {
            std::fs::rename(original, &backup)
                .map_err(|error| format!("{}: {error}", original.display()))?;
            moved_aside = Some((backup, false));
        }
    }
    if let Err(error) = std::fs::rename(stage, original) {
        if let Some((aside, _)) = &moved_aside {
            let _ = std::fs::rename(aside, original);
        }
        return Err(format!("{}: {error}", original.display()));
    }
    if let Some((previous, true)) = moved_aside {
        remove_any(&previous)?;
    }
    Ok(())
}

fn remove_any(path: &Path) -> Result<(), String> {
    let result = match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    };
    result.map_err(|error| format!("{}: {error}", path.display()))
}

/// Whether two glyphs draw the same thing: width, contours, components and anchors.
///
/// Identifiers, libs and notes are ignored, so a glyph that went through the editor
/// unchanged compares equal to the one the import produced.
pub fn same_drawing(a: &norad::Glyph, b: &norad::Glyph) -> bool {
    let close = |x: f64, y: f64| (x - y).abs() < 1e-6;
    close(a.width, b.width)
        && a.contours.len() == b.contours.len()
        && a.contours.iter().zip(&b.contours).all(|(p, q)| {
            p.points.len() == q.points.len()
                && p.points.iter().zip(&q.points).all(|(m, n)| {
                    close(m.x, n.x) && close(m.y, n.y) && m.typ == n.typ && m.smooth == n.smooth
                })
        })
        && a.components.len() == b.components.len()
        && a.components.iter().zip(&b.components).all(|(p, q)| {
            let (s, t) = (p.transform, q.transform);
            p.base == q.base
                && close(s.x_scale, t.x_scale)
                && close(s.xy_scale, t.xy_scale)
                && close(s.yx_scale, t.yx_scale)
                && close(s.y_scale, t.y_scale)
                && close(s.x_offset, t.x_offset)
                && close(s.y_offset, t.y_offset)
        })
        && a.anchors.len() == b.anchors.len()
        && a.anchors
            .iter()
            .zip(&b.anchors)
            .all(|(p, q)| p.name == q.name && close(p.x, q.x) && close(p.y, q.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_named_by_extension() {
        assert_eq!(
            ForeignFormat::of(Path::new("a/Font.glyphspackage")),
            Some(ForeignFormat::Glyphs)
        );
        assert_eq!(
            ForeignFormat::of(Path::new("Font.Fontra")),
            Some(ForeignFormat::Fontra)
        );
        assert_eq!(ForeignFormat::of(Path::new("Font.ufo")), None);
    }

    #[test]
    fn the_first_write_back_keeps_the_original_and_later_ones_replace() {
        let dir = std::env::temp_dir().join(format!("rb-foreign-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let original = dir.join("Font.glyphs");
        std::fs::write(&original, "original").unwrap();
        let stage = dir.join("stage");
        std::fs::write(&stage, "first").unwrap();
        replace_keeping_first_original(&original, &stage).unwrap();
        std::fs::write(&stage, "second").unwrap();
        replace_keeping_first_original(&original, &stage).unwrap();
        assert_eq!(std::fs::read_to_string(&original).unwrap(), "second");
        assert_eq!(
            std::fs::read_to_string(dir.join("Font (before Runebender).glyphs")).unwrap(),
            "original"
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
