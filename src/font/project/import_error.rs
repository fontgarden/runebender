// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Validation errors at the transient UFO-to-document boundary.

use std::{error::Error, fmt};

use crate::font::canonical_metadata::CanonicalMetadataError;
use crate::font::model::font_info::CanonicalFontInfoError;
use crate::font::model::glyph_metadata::GlyphMetadataError;

/// Why decoded source data could not become a canonical project.
#[derive(Debug)]
#[non_exhaustive]
pub enum ProjectImportError {
    /// No source was supplied for canonical construction.
    MissingSource,
    /// Kerning or group metadata is invalid.
    FontMetadata(CanonicalMetadataError),
    /// Font information violates a canonical invariant.
    FontInfo(CanonicalFontInfoError),
    /// A glyph's standardized metadata is invalid.
    GlyphMetadata {
        /// Glyph whose metadata failed validation.
        glyph: String,
        /// The metadata validation failure.
        source: GlyphMetadataError,
    },
}

impl fmt::Display for ProjectImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSource => f.write_str("canonical import needs at least one UFO source"),
            Self::FontMetadata(error) => error.fmt(f),
            Self::FontInfo(error) => error.fmt(f),
            Self::GlyphMetadata { glyph, source } => write!(f, "glyph {glyph}: {source}"),
        }
    }
}

impl Error for ProjectImportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MissingSource => None,
            Self::FontMetadata(error) => Some(error),
            Self::FontInfo(error) => Some(error),
            Self::GlyphMetadata { source, .. } => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::font::persistence::memory::{open_memory_project, ufo_files_from_font};
    use crate::font::project::{Project, SourceInput};

    fn malformed_font() -> norad::Font {
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("a"));
        font.lib
            .insert("public.skipExportGlyphs".into(), "not-an-array".into());
        font
    }

    #[test]
    fn decoded_source_exposes_the_typed_validation_cause() {
        let error = Project::from_source(SourceInput::from_font(
            malformed_font(),
            PathBuf::from("Malformed.ufo"),
        ))
        .unwrap_err();
        assert!(matches!(
            &error,
            ProjectImportError::GlyphMetadata { glyph, .. } if glyph == "a"
        ));
        assert!(
            error
                .source()
                .unwrap()
                .downcast_ref::<GlyphMetadataError>()
                .is_some()
        );
    }

    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn malformed_metadata_is_an_error_for_disk_and_memory_imports() {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let scratch = Scratch(std::env::temp_dir().join(format!(
            "runebender-invalid-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        )));
        std::fs::create_dir(&scratch.0).unwrap();
        let ufo = scratch.0.join("Malformed.ufo");
        let font = malformed_font();
        font.save(&ufo).unwrap();
        assert!(Project::load(&ufo).is_err());

        let designspace = scratch.0.join("Malformed.designspace");
        std::fs::write(
            &designspace,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<designspace format="5.0"><sources><source filename="Malformed.ufo" name="Regular"><location/></source></sources></designspace>"#,
        )
        .unwrap();
        assert!(Project::load(&designspace).is_err());

        let encoded = ufo_files_from_font(&font, &HashMap::new(), &HashMap::new()).unwrap();
        assert!(open_memory_project(Some("Malformed.ufo"), &encoded.files).is_err());
        let folder_files = encoded
            .files
            .into_iter()
            .map(|(name, bytes)| (format!("Malformed.ufo/{name}"), bytes))
            .collect::<Vec<_>>();
        assert!(open_memory_project(None, &folder_files).is_err());
    }
}
