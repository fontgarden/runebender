// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Discover local references without triggering source reloads or document saves.

use runebender::font::reference::ReferenceFace;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Default)]
pub(crate) struct References {
    pub(crate) faces: Vec<ReferenceFace>,
    pub(crate) errors: Vec<String>,
    folder: Option<PathBuf>,
    signature: Vec<(PathBuf, u64, Option<SystemTime>)>,
    checked: Option<Instant>,
}
pub(crate) fn directory(document: &Path) -> Option<PathBuf> {
    if document.as_os_str().is_empty() {
        return None;
    }
    let absolute = document
        .canonicalize()
        .or_else(|_| std::path::absolute(document))
        .ok()?;
    let parent = absolute.parent()?;
    let root = parent
        .ancestors()
        .find(|p| p.join(".git").exists())
        .unwrap_or(parent);
    Some(root.join("reference-fonts"))
}
fn scan(folder: &Path, paths: &mut Vec<(PathBuf, u64, Option<SystemTime>)>) -> Result<(), String> {
    if !folder.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(folder).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') || path.is_symlink() {
            continue;
        }
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        if metadata.is_dir() {
            scan(&path, paths)?;
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"))
        {
            paths.push((path, metadata.len(), metadata.modified().ok()));
        }
    }
    Ok(())
}
impl References {
    pub(crate) fn refresh(&mut self, document: &Path) {
        let folder = directory(document);
        if self.folder == folder
            && self
                .checked
                .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.checked = Some(Instant::now());
        let mut signature = Vec::new();
        let result = folder
            .as_ref()
            .map_or(Ok(()), |folder| scan(folder, &mut signature));
        signature.sort();
        if result.is_ok() && folder == self.folder && signature == self.signature {
            return;
        }
        self.folder = folder;
        self.faces.clear();
        self.errors.clear();
        if let Err(error) = result {
            self.errors.push(error);
        }
        for (path, len, _) in &signature {
            let loaded = if *len > 32 * 1024 * 1024 {
                Err("Font exceeds 32 MiB".into())
            } else {
                std::fs::read(path)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| ReferenceFace::parse(Arc::new(bytes)))
            };
            match loaded {
                Ok(face) => {
                    if !self.faces.iter().any(|old| old.bytes == face.bytes) {
                        self.faces.push(face);
                    }
                }
                Err(error) => self.errors.push(format!(
                    "{}: {error}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )),
            }
        }
        self.signature = signature;
    }
    pub(crate) fn selected(&self, weight: f32) -> Vec<&ReferenceFace> {
        let mut groups = std::collections::BTreeMap::new();
        for face in &self.faces {
            let selected = groups.entry(&face.group).or_insert(face);
            if face.compare(selected, weight).is_lt() {
                *selected = face;
            }
        }
        groups.into_values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_root_discovery_and_live_removal() {
        let root = std::env::temp_dir().join(format!(
            "runebender-reference-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        std::fs::write(root.join(".git"), "gitdir: elsewhere").unwrap();
        let doc = root.join("sources/font.ufo");
        std::fs::create_dir_all(&doc).unwrap();
        let refs = root.join("reference-fonts");
        assert_eq!(directory(&doc), Some(refs.clone()));
        std::fs::create_dir(&refs).unwrap();
        let path = refs.join("Regular.TTF");
        std::fs::write(
            &path,
            include_bytes!("../../../assets/fonts/VirtuaGrotesk-Regular.ttf"),
        )
        .unwrap();
        std::fs::write(refs.join("broken.otf"), "invalid").unwrap();
        let mut collection = References::default();
        collection.refresh(&doc);
        assert_eq!(collection.faces.len(), 1);
        assert_eq!(collection.errors.len(), 1);
        std::fs::remove_file(path).unwrap();
        collection.checked = None;
        collection.refresh(&doc);
        assert!(collection.faces.is_empty());
        std::fs::remove_file(root.join(".git")).unwrap();
        assert_eq!(directory(&doc), Some(root.join("sources/reference-fonts")));
        std::fs::remove_dir_all(root).unwrap();
    }
}
