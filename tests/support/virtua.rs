// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Full-font integration data, never a fallback for core tests.

use std::path::PathBuf;

/// The revision used by CI and the documented integration setup.
pub(crate) const REVISION: &str = "018488ed9499b7f2c18a249f5406797fc6a20e1d";

/// Require the complete pinned source family; missing data is a test failure.
pub(crate) fn dir() -> PathBuf {
    let dir = std::env::var_os("RUNEBENDER_TEST_FONTS").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../virtua-grotesk/sources"),
        PathBuf::from,
    );
    for file in [
        "VirtuaGrotesk.designspace",
        "VirtuaGrotesk-Regular.ufo/metainfo.plist",
        "VirtuaGrotesk-Bold.ufo/metainfo.plist",
    ] {
        assert!(
            dir.join(file).is_file(),
            "missing integration fixture {}: check out eliheuer/virtua-grotesk at \
             {REVISION} and set RUNEBENDER_TEST_FONTS to its sources directory",
            dir.join(file).display()
        );
    }
    dir
}
