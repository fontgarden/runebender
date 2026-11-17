// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded regular-file reads and revision-guarded replacement writes.
//!
//! A revision is the SHA-256 of the exact file bytes.
//! A guarded write detects an observed external edit before staging and immediately before
//! the rename.
//! It does not claim operating-system locking or an atomic compare-and-swap against an
//! arbitrary writer that races the final same-directory rename.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use runebender::workflows::digest;

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

/// Failure of a bounded read or a guarded write.
#[derive(Debug)]
pub(crate) enum GuardedFileError {
    /// The entry is missing, not a regular file, or is a symbolic link.
    NotFound,
    /// The file is larger than the caller's limit.
    TooLarge,
    /// The destination exists when a new file was requested.
    AlreadyExists,
    /// The on-disk revision differs from the revision observed by the caller.
    Conflict {
        /// Revision currently observed on disk.
        actual_revision: String,
    },
    /// A filesystem operation failed.
    Io(io::Error),
}

impl From<io::Error> for GuardedFileError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Revision of exact file bytes.
pub(crate) fn revision(bytes: &[u8]) -> String {
    digest::sha256_hex(bytes)
}

/// Read a regular, non-symlink file of at most `limit` bytes, with its metadata.
pub(crate) fn read_regular(
    path: &Path,
    limit: usize,
) -> Result<(Vec<u8>, fs::Metadata), GuardedFileError> {
    let path_metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            GuardedFileError::NotFound
        } else {
            GuardedFileError::Io(error)
        }
    })?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
        return Err(GuardedFileError::NotFound);
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(GuardedFileError::NotFound);
    }
    if metadata.len() > limit as u64 {
        return Err(GuardedFileError::TooLarge);
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(limit));
    file.take(limit.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(GuardedFileError::TooLarge);
    }
    Ok((bytes, metadata))
}

/// Check that `path` is absent (`None`) or holds the `expected` revision.
pub(crate) fn check_expected(
    path: &Path,
    expected_revision: Option<&str>,
    limit: usize,
) -> Result<(), GuardedFileError> {
    match (expected_revision, fs::symlink_metadata(path)) {
        (None, Err(error)) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        (None, Ok(_)) => Err(GuardedFileError::AlreadyExists),
        (None, Err(error)) => Err(error.into()),
        (Some(_), Err(error)) if error.kind() == io::ErrorKind::NotFound => {
            Err(GuardedFileError::NotFound)
        }
        (Some(expected), Ok(_)) => {
            let actual = revision(&read_regular(path, limit)?.0);
            if actual == expected {
                Ok(())
            } else {
                Err(GuardedFileError::Conflict {
                    actual_revision: actual,
                })
            }
        }
        (Some(_), Err(error)) => Err(error.into()),
    }
}

/// Replace `path` in `parent` with `bytes` through a synced temporary file and a rename.
///
/// `None` requires the destination to remain absent.
/// `Some` requires the destination to hold that revision before staging and before the rename.
/// `temporary_prefix` names the hidden staging file, such as `.runebender-graph`.
pub(crate) fn write_guarded(
    parent: &Path,
    path: &Path,
    bytes: &[u8],
    expected_revision: Option<&str>,
    limit: usize,
    temporary_prefix: &str,
) -> Result<(), GuardedFileError> {
    check_expected(path, expected_revision, limit)?;
    let (temporary, mut file) = create_temporary(parent, temporary_prefix)?;
    let staged = (|| -> Result<(), GuardedFileError> {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        check_expected(path, expected_revision, limit)?;
        fs::rename(&temporary, path)?;
        sync_directory(parent)?;
        Ok(())
    })();
    if staged.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    staged
}

/// Flush a directory entry change where the platform supports it.
pub(crate) fn sync_directory(path: &Path) -> Result<(), GuardedFileError> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    // Windows cannot open a directory as a file to flush it.
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn create_temporary(parent: &Path, prefix: &str) -> Result<(PathBuf, File), GuardedFileError> {
    for _ in 0..32 {
        let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let name = format!("{prefix}-{}-{sequence}.tmp", std::process::id());
        let path = parent.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(GuardedFileError::Io(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary file",
    )))
}
