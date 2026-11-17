// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Original GLIF bytes guarded by a digest of their decoded, deterministically encoded contents.
//!
//! This is preservation data, never a second editable glyph model.
//! Both filesystem and in-memory saves reuse raw bytes only when the entire serialized glyph is
//! unchanged.
//! A file whose bytes already equal the deterministic encoding is not retained, because an
//! ordinary save writes the same bytes.

use sha2::{Digest as _, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OriginalGlif {
    original: Vec<u8>,
    encoded_digest: [u8; 32],
}

impl OriginalGlif {
    /// Retain `original` when it differs from the deterministic encoding of `glyph`.
    ///
    /// Returns `None` when the bytes are equal, since an ordinary save reproduces them.
    pub(super) fn capture(original: Vec<u8>, glyph: &norad::Glyph) -> Result<Option<Self>, String> {
        let encoded = glyph.encode_xml().map_err(|error| error.to_string())?;
        if encoded == original {
            return Ok(None);
        }
        Ok(Some(Self {
            original,
            encoded_digest: Sha256::digest(&encoded).into(),
        }))
    }

    pub(super) fn unchanged_bytes(&self, encoded: &[u8]) -> Option<&[u8]> {
        (<[u8; 32]>::from(Sha256::digest(encoded)) == self.encoded_digest)
            .then_some(self.original.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::OriginalGlif;

    #[test]
    fn canonical_bytes_are_not_retained_and_formatted_bytes_are_guarded() {
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.0;
        let encoded = glyph.encode_xml().unwrap();
        assert_eq!(OriginalGlif::capture(encoded.clone(), &glyph), Ok(None));

        let mut formatted = encoded.clone();
        formatted.extend_from_slice(b"\n\n");
        let original = OriginalGlif::capture(formatted.clone(), &glyph)
            .unwrap()
            .expect("formatted bytes differ from the encoding");
        assert_eq!(
            original.unchanged_bytes(&encoded),
            Some(formatted.as_slice())
        );

        glyph.width = 600.0;
        let edited = glyph.encode_xml().unwrap();
        assert_eq!(original.unchanged_bytes(&edited), None);
    }
}
