// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Read the proof samples of the project of the open font, and read them again when they change.
//!
//! The file is `sources/proof-samples.toml` at the project root that `reference-fonts/` uses,
//! or `proof-samples.toml` next to the font, as the web proof of fgtools reads it.

use super::reference_fonts::project_root;
use runebender::text::proof_samples::{self, ProofSample};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// The name of the sample file of a project.
const FILE_NAME: &str = "proof-samples.toml";
/// A larger file is reported instead of read.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// The sample file's path, length and modification time; `None` without a file.
pub(crate) type Signature = Option<(PathBuf, u64, Option<SystemTime>)>;

/// What is on disk for a document's sample file now.
/// A background task compares this once a second, so the interface is woken only on a change.
pub(crate) fn disk_signature(document: &Path) -> Signature {
    file(document).map(|path| {
        let metadata = std::fs::metadata(&path).ok();
        let len = metadata.as_ref().map_or(0, std::fs::Metadata::len);
        let modified = metadata.and_then(|m| m.modified().ok());
        (path, len, modified)
    })
}

/// The samples of the open project: the defaults merged with the project file.
#[derive(Debug)]
pub(crate) struct ProofSamples {
    pub(crate) samples: Vec<ProofSample>,
    /// Why the project file was not used; the defaults stay in use.
    pub(crate) error: Option<String>,
    document: Option<PathBuf>,
    signature: Signature,
    checked: Option<Instant>,
}

impl Default for ProofSamples {
    fn default() -> Self {
        Self {
            samples: proof_samples::default_samples(),
            error: None,
            document: None,
            signature: None,
            checked: None,
        }
    }
}

/// The sample file of a document: `sources/proof-samples.toml` at the project root, else
/// `proof-samples.toml` in the folder of the document. `None` when neither file exists.
pub(crate) fn file(document: &Path) -> Option<PathBuf> {
    let root = project_root(document)?;
    let beside = document
        .canonicalize()
        .or_else(|_| std::path::absolute(document))
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join(FILE_NAME)));
    std::iter::once(root.join("sources").join(FILE_NAME))
        .chain(beside)
        .find(|path| path.is_file())
}

impl ProofSamples {
    /// Read the file again when it was added, changed or removed, at most once a second.
    /// Returns whether the samples or the error changed.
    pub(crate) fn refresh(&mut self, document: &Path) -> bool {
        let document = (!document.as_os_str().is_empty()).then(|| document.to_path_buf());
        if self.document == document
            && self
                .checked
                .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return false;
        }
        self.checked = Some(Instant::now());
        let signature: Signature = document.as_deref().and_then(disk_signature);
        if self.document == document && self.signature == signature {
            return false;
        }
        self.document = document;
        self.signature = signature;
        let (samples, error) = match &self.signature {
            None => (proof_samples::default_samples(), None),
            Some((path, len, _)) => match read(path, *len) {
                Ok(samples) => (samples, None),
                Err(error) => (
                    proof_samples::default_samples(),
                    Some(format!("{}: {error}", display_name(path))),
                ),
            },
        };
        let changed = samples != self.samples || error != self.error;
        self.samples = samples;
        self.error = error;
        changed
    }
}

fn read(path: &Path, len: u64) -> Result<Vec<ProofSample>, String> {
    if len > MAX_FILE_BYTES {
        return Err("the file exceeds 1 MiB".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file = proof_samples::parse(&text)?;
    Ok(proof_samples::merge(proof_samples::default_samples(), file))
}

/// The path from the folder that holds `sources/`, as the designer names the file.
fn display_name(path: &Path) -> String {
    let parent = path.parent().and_then(Path::file_name);
    match (parent, path.file_name()) {
        (Some(parent), Some(name)) => {
            format!("{}/{}", parent.to_string_lossy(), name.to_string_lossy())
        }
        _ => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "runebender-proof-samples-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        root.canonicalize().unwrap()
    }

    #[test]
    fn the_file_is_found_at_the_git_root_or_beside_the_font() {
        let root = temporary_root();
        std::fs::write(root.join(".git"), "gitdir: elsewhere").unwrap();
        let doc = root.join("sources/Font.designspace");
        std::fs::create_dir_all(root.join("sources")).unwrap();
        std::fs::write(&doc, "").unwrap();
        assert_eq!(file(&doc), None);
        let path = root.join("sources/proof-samples.toml");
        std::fs::write(&path, "").unwrap();
        assert_eq!(file(&doc), Some(path.clone()));
        // A font deeper in the repository still finds the file at the Git root.
        let nested = root.join("sources/masters/Font-Regular.ufo");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(file(&nested), Some(path.clone()));
        // Outside Git, the file is next to the font.
        std::fs::remove_file(root.join(".git")).unwrap();
        std::fs::remove_file(&path).unwrap();
        let beside = root.join("sources/masters/proof-samples.toml");
        std::fs::write(&beside, "").unwrap();
        assert_eq!(file(&nested), Some(beside));
        assert_eq!(file(Path::new("")), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn samples_follow_the_file_and_an_error_keeps_the_defaults() {
        let root = temporary_root();
        std::fs::write(root.join(".git"), "gitdir: elsewhere").unwrap();
        std::fs::create_dir_all(root.join("sources")).unwrap();
        let doc = root.join("sources/Font.designspace");
        std::fs::write(&doc, "").unwrap();
        let path = root.join("sources/proof-samples.toml");
        let defaults = proof_samples::default_samples();

        let mut samples = ProofSamples::default();
        assert!(!samples.refresh(&doc));
        assert_eq!(samples.samples, defaults);

        std::fs::write(
            &path,
            "defaults = false\n[sample.own]\nlabel = \"Own\"\ntext = \"o\"\n",
        )
        .unwrap();
        samples.checked = None;
        assert!(samples.refresh(&doc));
        assert_eq!(samples.samples.len(), 1);
        assert_eq!(samples.error, None);

        std::fs::write(&path, "[sample.broken\n").unwrap();
        samples.checked = None;
        assert!(samples.refresh(&doc));
        assert_eq!(samples.samples, defaults);
        let error = samples.error.clone().unwrap();
        assert!(error.starts_with("sources/proof-samples.toml: "), "{error}");

        std::fs::remove_file(&path).unwrap();
        samples.checked = None;
        assert!(samples.refresh(&doc));
        assert_eq!(samples.error, None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
