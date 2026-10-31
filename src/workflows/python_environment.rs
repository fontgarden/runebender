// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runebender's Python virtual environment, `~/Runebender/python`.
//!
//! It is an ordinary virtual environment beside `~/Runebender/models`, so the same tools work
//! outside the editor: `source ~/Runebender/python/bin/activate`.
//! Python recipes run its `python3`, and `DrawBot` specimen scripts run its `drawbot`, whenever it
//! has them and no environment variable or config setting names another.
//! The editor never creates or changes it; see `docs/LOCAL-AI.md` for how to set it up.

use std::path::PathBuf;

/// The environment's folder, whether or not it exists.
pub fn directory() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Runebender").join("python"))
}

/// A command installed in the environment's `bin/`, when it exists.
pub fn command(name: &str) -> Option<PathBuf> {
    directory()
        .map(|directory| directory.join("bin").join(name))
        .filter(|path| path.is_file())
}
