// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Original GLIF bytes guarded by their decoded, deterministically encoded contents.
//!
//! This is preservation data, never a second editable glyph model. Both filesystem
//! and in-memory saves reuse raw bytes only when the entire serialized glyph is unchanged.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OriginalGlif {
    original: Vec<u8>,
    encoded: Vec<u8>,
}

impl OriginalGlif {
    pub(super) fn capture(original: Vec<u8>, glyph: &norad::Glyph) -> Result<Self, String> {
        Ok(Self {
            original,
            encoded: glyph.encode_xml().map_err(|error| error.to_string())?,
        })
    }

    pub(super) fn unchanged_bytes(&self, encoded: &[u8]) -> Option<&[u8]> {
        (encoded == self.encoded).then_some(self.original.as_slice())
    }
}
