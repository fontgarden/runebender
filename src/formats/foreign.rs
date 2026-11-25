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
//! - The editor watches the working copy. If the original changes, saving the project keeps
//!   the edited working copy and reports a conflict before publishing the original.
//! - Masters or sources added in Runebender cannot be written back; `write_back` refuses
//!   and only the UFO working copy is saved.
//! - Font-wide data edited in Runebender (names, font info, OpenType values, designspace
//!   instances and rules) is not written back. Only the items listed above are merged.
//! - Backup layers made in the Layers panel are written back: to Glyphs as backup layers of
//!   their master, and to Fontra as sub-layers of their source. Brace layers made in
//!   Runebender are not written back, and Glyphs files do not get the background layer.
//! - Save As writes UFOs; there is no "save as Glyphs" or "save as Fontra" for a font that
//!   started as UFO.
//! - These formats have not been tried in the browser host.

use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::font::persistence::{fresh_sibling, path_entry_exists};

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

/// The source revision observed on open and any unpublished working-copy save.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ForeignDocument {
    observed: Option<(ForeignSource, SourceRevision)>,
    pending: bool,
}

impl ForeignDocument {
    /// Read the conversion and the revision from the same source.
    pub(crate) fn open(
        path: &Path,
    ) -> Result<Option<(ForeignSource, Self, super::glyphs_import::ConversionResult)>, String> {
        let Some(format) = ForeignFormat::of(path) else {
            return Ok(None);
        };
        let (mut conversion, revision) = if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("glyphs"))
        {
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let revision = SourceRevision::from_file_bytes(text.as_bytes());
            (super::glyphs_import::glyphs_to_ufo_files(&text)?, revision)
        } else {
            let revision = source_revision(path)?;
            let conversion = match format {
                ForeignFormat::Glyphs => {
                    let entries = crate::font::project::read_glyphspackage(path)?;
                    super::glyphs_import::glyphs_package_to_ufo_files(&entries)?
                }
                ForeignFormat::Fontra => super::fontra::to_ufo_files(path)?,
            };
            check_revision(path, &revision)?;
            (conversion, revision)
        };
        if revision.linked_entries {
            conversion.warnings.push("The original package contains symbolic links. Save As keeps the edited working copy independent.".into());
        }
        let source = ForeignSource {
            path: path.to_owned(),
            format,
        };
        let document = Self {
            observed: Some((source.clone(), revision)),
            pending: false,
        };
        Ok(Some((source, document, conversion)))
    }

    pub(crate) fn needs_write_back(&self) -> bool {
        self.pending
    }

    pub(crate) fn save(
        &mut self,
        source: Option<&ForeignSource>,
        working: Option<&Path>,
        changed: bool,
    ) -> Result<(), String> {
        let Some(source) = source else {
            *self = Self::default();
            return Ok(());
        };
        self.pending |= changed;
        if !self.pending {
            return Ok(());
        }
        let working = working.ok_or("missing UFO working copy")?;
        let save_error = |error: String| {
            format!(
                "the UFO working copy saved at {}, but {} did not: {error}",
                working.display(),
                source.path.display()
            )
        };
        let Some((opened, revision)) = &mut self.observed else {
            return Err(save_error(
                "this original was never opened. Reopen it to establish a write-back revision."
                    .into(),
            ));
        };
        if opened != source {
            return Err(save_error(format!(
                "this working copy was opened from {}. Reopen the new original to establish its write-back revision.",
                opened.path.display()
            )));
        }
        *revision = write_back_revision(source, working, revision).map_err(save_error)?;
        self.pending = false;
        Ok(())
    }
}

/// Content of an imported file or package, independent of its root name.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceRevision {
    digest: [u8; 32],
    /// The package writers reconstruct files and cannot publish interior symbolic links.
    linked_entries: bool,
}

impl SourceRevision {
    fn from_file_bytes(bytes: &[u8]) -> Self {
        let mut hash = Sha256::new();
        hash.update([0]);
        hash.update(bytes);
        Self {
            digest: hash.finalize().into(),
            linked_entries: false,
        }
    }
}

fn source_revision(path: &Path) -> Result<SourceRevision, String> {
    fn read(
        path: &Path,
        directories: &mut Vec<PathBuf>,
        links: &mut bool,
    ) -> std::io::Result<[u8; 32]> {
        let kind = std::fs::symlink_metadata(path)?.file_type();
        let mut hash = Sha256::new();
        if kind.is_symlink() {
            *links = true;
            let target = path.canonicalize()?;
            if directories.contains(&target) {
                return Err(std::io::Error::other(format!(
                    "cyclic package link at {}",
                    path.display()
                )));
            }
            hash.update([2]);
            hash.update(std::fs::read_link(path)?.as_os_str().as_encoded_bytes());
            hash.update(read(&target, directories, links)?);
        } else if kind.is_file() {
            hash.update([0]);
            std::io::copy(&mut std::fs::File::open(path)?, &mut hash)?;
        } else if kind.is_dir() {
            directories.push(path.to_owned());
            hash.update([1]);
            let mut entries = std::fs::read_dir(path)?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort_unstable();
            for name in entries {
                let bytes = name.as_encoded_bytes();
                hash.update(bytes.len().to_le_bytes());
                hash.update(bytes);
                hash.update(read(&path.join(name), directories, links)?);
            }
            directories.pop();
        } else {
            return Err(std::io::Error::other(format!(
                "expected a regular file or directory at {}",
                path.display()
            )));
        }
        Ok(hash.finalize().into())
    }
    // Resolve the user-selected root once. Only links inside the package affect write-back.
    let mut linked_entries = false;
    let digest = path
        .canonicalize()
        .and_then(|root| read(&root, &mut Vec::new(), &mut linked_entries))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(SourceRevision {
        digest,
        linked_entries,
    })
}

fn check_revision(path: &Path, expected: &SourceRevision) -> Result<(), String> {
    if &source_revision(path)? == expected {
        return Ok(());
    }
    Err(format!(
        "{} changed outside Runebender. Reopen the original to review those changes, or use Save As to keep an independent copy.",
        path.display()
    ))
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
    let revision = source_revision(&foreign.path)?;
    write_back_revision(foreign, working, &revision).map(|_| ())
}

/// Stage one write against the original revision the editor actually read.
fn write_back_revision(
    foreign: &ForeignSource,
    working: &Path,
    expected: &SourceRevision,
) -> Result<SourceRevision, String> {
    if expected.linked_entries {
        return Err("the original package contains symbolic links. Use Save As to save the working copy independently.".into());
    }
    // Use a separate inode for the lock, because publication replaces the original inode.
    // The OS releases it if the process exits. Keeping its empty file in the temporary
    // directory lets waiting editors always lock the same inode without adding source files.
    let identity = foreign
        .path
        .canonicalize()
        .map_err(|error| format!("{}: {error}", foreign.path.display()))?;
    let key = Sha256::digest(identity.as_os_str().as_encoded_bytes());
    let lock_path = std::env::temp_dir().join(format!("runebender-foreign-{key:x}.lock"));
    let lock = std::fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| format!("{}: {error}", lock_path.display()))?;
    lock.try_lock().map_err(|error| {
        format!(
            "cannot lock {} for write-back: {error}",
            foreign.path.display()
        )
    })?;
    check_revision(&foreign.path, expected)?;
    let sources = read_working_copy(working)?;
    let stage_dir = fresh_sibling(&identity, "foreign")?;
    std::fs::create_dir(&stage_dir).map_err(|error| format!("{}: {error}", stage_dir.display()))?;
    let stage = stage_dir.join(match foreign.format {
        ForeignFormat::Glyphs if identity.is_dir() => "font.glyphspackage",
        ForeignFormat::Glyphs => "font.glyphs",
        ForeignFormat::Fontra => "font.fontra",
    });
    let result = (|| {
        match foreign.format {
            ForeignFormat::Glyphs => {
                super::glyphs_export::write_back(&foreign.path, &sources, &stage)
            }
            ForeignFormat::Fontra => super::fontra::write_back(&foreign.path, &sources, &stage),
        }?;
        let revision = source_revision(&stage)?;
        replace_keeping_first_original(&identity, &stage, expected)?;
        Ok(revision)
    })();
    let _ = remove_any(&stage_dir);
    result
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
fn replace_keeping_first_original(
    original: &Path,
    stage: &Path,
    expected: &SourceRevision,
) -> Result<(), String> {
    let backup = backup_path(original);
    let keep_first = !path_entry_exists(&backup)?;
    let aside = if keep_first {
        backup
    } else {
        fresh_sibling(original, "previous")?
    };
    move_to_vacant_path(original, &aside)?;
    // Verify the actual tree that was displaced. An editor may have replaced the original
    // during staging or in the interval between a pathname check and this move.
    let publish =
        check_revision(&aside, expected).and_then(|()| move_to_vacant_path(stage, original));
    if let Err(error) = publish {
        move_to_vacant_path(&aside, original).map_err(|rollback| {
            format!(
                "{error}. Restoring {} failed: {rollback}. The displaced original remains at {}",
                original.display(),
                aside.display()
            )
        })?;
        return Err(error);
    }
    if !keep_first {
        // Publication succeeded. A leftover belongs to this save, never to the next one.
        let _ = remove_any(&aside);
    }
    Ok(())
}

/// Move either kind of source without replacing a concurrently recreated destination.
fn move_to_vacant_path(from: &Path, to: &Path) -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use rustix::fs::{CWD, RenameFlags, renameat_with};

        renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE)
            .map_err(|error| format!("{} to {}: {error}", from.display(), to.display()))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        if path_entry_exists(to)? {
            return Err(format!("{} already exists", to.display()));
        }
        // std has no portable no-replace rename. Cooperating editors still share the lock,
        // but another process can create the destination between this check and the rename.
        std::fs::rename(from, to)
            .map_err(|error| format!("{} to {}: {error}", from.display(), to.display()))
    }
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
    close(a.width, b.width) && same_shapes(a, b) && same_anchors(a, b)
}

pub(super) fn close(x: f64, y: f64) -> bool {
    (x - y).abs() < 1e-6
}

pub(super) fn same_shapes(a: &norad::Glyph, b: &norad::Glyph) -> bool {
    a.contours.len() == b.contours.len()
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
}

pub(super) fn same_anchors(a: &norad::Glyph, b: &norad::Glyph) -> bool {
    a.anchors.len() == b.anchors.len()
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
        let stale = SourceRevision::from_file_bytes(b"an earlier revision");
        assert!(replace_keeping_first_original(&original, &stage, &stale).is_err());
        assert_eq!(std::fs::read_to_string(&original).unwrap(), "original");
        assert!(!backup_path(&original).exists());
        replace_keeping_first_original(&original, &stage, &source_revision(&original).unwrap())
            .unwrap();
        std::fs::write(&stage, "second").unwrap();
        replace_keeping_first_original(&original, &stage, &source_revision(&original).unwrap())
            .unwrap();
        assert_eq!(std::fs::read_to_string(&original).unwrap(), "second");
        assert_eq!(
            std::fs::read_to_string(dir.join("Font (before Runebender).glyphs")).unwrap(),
            "original"
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
