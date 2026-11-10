// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Repository-owned core fixtures and explicitly enabled full-font integration fixtures.
//!
//! `tests/fixtures/core/generate.py` defines the original synthetic core font.
//! Core tests never inspect `RUNEBENDER_TEST_FONTS` or an adjacent checkout.

use std::path::PathBuf;

#[cfg(feature = "integration-fonts")]
#[path = "../../tests/support/virtua.rs"]
mod virtua;

pub(crate) fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core")
}

pub(crate) fn regular_ufo() -> PathBuf {
    dir().join("Core-Regular.ufo")
}

pub(crate) fn designspace() -> PathBuf {
    dir().join("Core.designspace")
}

#[cfg(feature = "integration-fonts")]
pub(crate) fn virtua_regular_ufo() -> PathBuf {
    virtua::dir().join("VirtuaGrotesk-Regular.ufo")
}

#[cfg(feature = "integration-fonts")]
pub(crate) fn virtua_designspace() -> PathBuf {
    virtua::dir().join("VirtuaGrotesk.designspace")
}
