// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Source-format data that is not canonical editable font state.
//!
//! Canonical glyph geometry, metadata, features, groups and kerning live in the document model.
//! This record retains only glyph-free UFO structure and opaque resources needed to reconstruct a
//! transient Norad value at a persistence boundary.

use std::path::{Path, PathBuf};

/// Glyph-free UFO structure and opaque resources retained for exact persistence.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::font) struct SourceFormatData {
    meta: norad::MetaInfo,
    font_info: norad::FontInfo,
    layers: norad::LayerContents,
    lib: norad::Plist,
    data: norad::datastore::DataStore,
    images: norad::datastore::ImageStore,
}

/// The one new, glyph-free layer container owned by a background history group.
#[derive(Clone, Debug)]
pub(in crate::font) struct LayerContainerCreation {
    before: Vec<(String, PathBuf)>,
    name: String,
    created: Option<norad::Layer>,
}

impl Default for SourceFormatData {
    fn default() -> Self {
        Self::from_ufo(&norad::Font::new())
    }
}

impl SourceFormatData {
    /// Capture only the source-format fields not owned by the canonical document.
    pub(in crate::font) fn from_ufo(font: &norad::Font) -> Self {
        let mut layers = font.layers.clone();
        for layer in layers.iter_mut() {
            layer.clear();
        }
        let mut font_info = font.font_info.clone();
        super::super::model::font_info::clear_canonical_font_info_fields(&mut font_info);
        Self {
            meta: font.meta.clone(),
            font_info,
            layers,
            lib: font.lib.clone(),
            data: font.data.clone(),
            images: font.images.clone(),
        }
    }

    /// Materialize a glyph-free UFO codec value for staged export.
    pub(in crate::font) fn to_ufo_template(&self) -> norad::Font {
        let mut font = norad::Font::new();
        font.meta.clone_from(&self.meta);
        font.font_info.clone_from(&self.font_info);
        font.layers.clone_from(&self.layers);
        font.lib.clone_from(&self.lib);
        font.data.clone_from(&self.data);
        font.images.clone_from(&self.images);
        font
    }

    /// Update supported font-lib references without changing opaque source data.
    pub(in crate::font) fn rename_glyph_references(
        &mut self,
        old: &str,
        new: &str,
    ) -> Result<bool, String> {
        crate::formats::metadata::lib_keys::rename_glyph_lib_references(&mut self.lib, old, new)
    }

    /// Remove supported font-lib references to one glyph.
    pub(in crate::font) fn remove_glyph_references(&mut self, name: &str) -> Result<bool, String> {
        crate::formats::metadata::lib_keys::remove_glyph_lib_references(&mut self.lib, name)
    }

    pub(in crate::font) fn default_layer_name(&self) -> &str {
        self.layers.default_layer().name().as_str()
    }

    pub(in crate::font) fn contains_layer(&self, name: &str) -> bool {
        self.layers.get(name).is_some()
    }

    pub(in crate::font) fn layer_names(&self) -> impl Iterator<Item = &str> {
        self.layers.names().map(norad::Name::as_str)
    }

    pub(in crate::font) fn prepare_layer_creation(
        &self,
        name: &str,
    ) -> Result<Option<LayerContainerCreation>, String> {
        if self.contains_layer(name) {
            return Ok(None);
        }
        norad::Name::new(name).map_err(|error| error.to_string())?;
        Ok(Some(LayerContainerCreation {
            before: self
                .layers
                .iter()
                .map(|layer| (layer.name().to_string(), layer.path().to_owned()))
                .collect(),
            name: name.into(),
            created: None,
        }))
    }

    pub(in crate::font) fn matches_layer_creation(
        &self,
        creation: &LayerContainerCreation,
        forward: bool,
    ) -> bool {
        let mut current = self.layers.iter();
        for (name, path) in &creation.before {
            if !current
                .next()
                .is_some_and(|layer| layer.name().as_str() == name.as_str() && layer.path() == path)
            {
                return false;
            }
        }
        if !forward {
            let Some(created) = &creation.created else {
                return false;
            };
            if current.next() != Some(created) {
                return false;
            }
        }
        current.next().is_none()
    }

    pub(in crate::font) fn install_layer_creation(
        &mut self,
        creation: &mut LayerContainerCreation,
        forward: bool,
    ) -> Result<(), String> {
        if !self.matches_layer_creation(creation, forward) {
            return Err("background layer container changed after history capture".into());
        }
        // Norad validates before inserting. Do not clone all empty layers: their cleared glyph
        // filename hash tables can still retain capacity proportional to the imported font.
        if forward {
            let created = self
                .layers
                .new_layer(&creation.name)
                .map_err(|error| error.to_string())?;
            if let Some(expected) = &creation.created {
                if &*created != expected {
                    self.layers.remove(&creation.name);
                    return Err("background layer path changed after history capture".into());
                }
            } else {
                creation.created = Some(created.clone());
            }
        } else {
            self.layers
                .remove(&creation.name)
                .expect("guarded created container remains present");
        }
        Ok(())
    }

    pub(in crate::font) fn ensure_layer(&mut self, name: &str) -> Result<(), String> {
        self.layers
            .get_or_create_layer(name)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub(in crate::font) fn retain_default_layer(&mut self) {
        self.layers.retain(|_| false);
    }

    pub(in crate::font) fn remove_empty_layer(&mut self, name: &str) -> bool {
        if self.default_layer_name() == name
            || self.layers.get(name).is_none_or(|layer| !layer.is_empty())
        {
            return false;
        }
        self.layers.remove(name).is_some()
    }

    pub(in crate::font) fn image_bytes(
        &self,
        path: &Path,
    ) -> Result<Option<std::sync::Arc<[u8]>>, String> {
        self.images
            .get(path)
            .transpose()
            .map_err(|error| error.to_string())
    }

    pub(in crate::font) fn image_paths(&self) -> Vec<PathBuf> {
        self.images.iter().map(|(path, _)| path.clone()).collect()
    }

    pub(in crate::font) fn remove_image(&mut self, path: &Path) -> bool {
        let present = self.images.contains_key(path);
        self.images.remove(path);
        present
    }

    pub(in crate::font) fn install_image(
        &mut self,
        path: PathBuf,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        self.images
            .insert(path, bytes)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use norad::{Color, Font, Glyph};

    use super::SourceFormatData;
    use crate::formats::metadata::lib_keys::{GLYPH_ORDER_KEY, PSNAMES_KEY};

    #[test]
    fn created_container_metadata_blocks_removal_without_losing_the_change() {
        let mut format = SourceFormatData::default();
        let mut creation = format
            .prepare_layer_creation("public.background")
            .unwrap()
            .unwrap();
        format.install_layer_creation(&mut creation, true).unwrap();
        format
            .layers
            .get_mut("public.background")
            .unwrap()
            .lib
            .insert("owner".into(), plist::Value::String("later edit".into()));
        let before = format.clone();
        assert!(!format.matches_layer_creation(&creation, false));
        assert!(format.install_layer_creation(&mut creation, false).is_err());
        assert_eq!(format, before);
    }

    #[test]
    fn creation_replay_rolls_back_when_reserved_paths_changed() {
        let mut format = SourceFormatData::default();
        format.layers.new_layer("public.background").unwrap();
        // Norad retain leaves removed paths reserved even though the visible layout is empty.
        format.layers.retain(|_| false);
        let mut creation = format
            .prepare_layer_creation("public.background")
            .unwrap()
            .unwrap();
        format.install_layer_creation(&mut creation, true).unwrap();
        format.install_layer_creation(&mut creation, false).unwrap();
        format.layers = norad::LayerContents::default();
        let before = format.clone();
        assert!(format.install_layer_creation(&mut creation, true).is_err());
        assert_eq!(format, before);
    }

    fn format_with_references() -> SourceFormatData {
        let mut format = SourceFormatData::default();
        format.lib.insert(
            GLYPH_ORDER_KEY.into(),
            plist::Value::Array(vec![
                "B".into(),
                "A".into(),
                7.into(),
                "A".into(),
                "C".into(),
            ]),
        );
        let mut names = plist::Dictionary::new();
        names.insert("A".into(), "uni0041".into());
        names.insert("B".into(), "B.production".into());
        names.insert("opaque".into(), plist::Value::Array(vec![true.into()]));
        format
            .lib
            .insert(PSNAMES_KEY.into(), plist::Value::Dictionary(names));
        format.lib.insert(
            "com.example.opaque".into(),
            plist::Value::Data(vec![1, 2, 3]),
        );
        format
    }

    #[test]
    fn rename_preserves_order_production_value_and_opaque_metadata() {
        let mut format = format_with_references();
        let mut expected = format.clone();
        expected.lib.insert(
            GLYPH_ORDER_KEY.into(),
            plist::Value::Array(vec![
                "B".into(),
                "A.alt".into(),
                7.into(),
                "A.alt".into(),
                "C".into(),
            ]),
        );
        let names = expected
            .lib
            .get_mut(PSNAMES_KEY)
            .unwrap()
            .as_dictionary_mut()
            .unwrap();
        let production = names.remove("A").unwrap();
        names.insert("A.alt".into(), production);

        assert!(format.rename_glyph_references("A", "A.alt").unwrap());
        assert_eq!(format, expected);
        assert!(!format.rename_glyph_references("A", "A.alt").unwrap());
        assert!(!format.rename_glyph_references("A.alt", "A.alt").unwrap());
    }

    #[test]
    fn remove_deletes_only_affected_entries() {
        let mut format = format_with_references();
        let mut expected = format.clone();
        expected.lib.insert(
            GLYPH_ORDER_KEY.into(),
            plist::Value::Array(vec!["B".into(), 7.into(), "C".into()]),
        );
        expected
            .lib
            .get_mut(PSNAMES_KEY)
            .unwrap()
            .as_dictionary_mut()
            .unwrap()
            .remove("A");

        assert!(format.remove_glyph_references("A").unwrap());
        assert_eq!(format, expected);
        assert!(!format.remove_glyph_references("A").unwrap());
    }

    #[test]
    fn absent_and_last_entries_follow_lib_conventions() {
        let mut format = SourceFormatData::default();
        let original = format.clone();
        assert!(!format.rename_glyph_references("A", "B").unwrap());
        assert!(!format.remove_glyph_references("A").unwrap());
        assert_eq!(format, original);

        format.lib.insert(
            GLYPH_ORDER_KEY.into(),
            plist::Value::Array(vec!["A".into()]),
        );
        let mut names = plist::Dictionary::new();
        names.insert("A".into(), "uni0041".into());
        format
            .lib
            .insert(PSNAMES_KEY.into(), plist::Value::Dictionary(names));
        assert!(format.remove_glyph_references("A").unwrap());
        assert_eq!(
            format
                .lib
                .get(GLYPH_ORDER_KEY)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert!(!format.lib.contains_key(PSNAMES_KEY));
    }

    #[test]
    fn rename_collisions_reject_without_changing_either_key() {
        let original = format_with_references();
        // C exists only in glyphOrder; B exists in both supported keys.
        for destination in ["B", "C", "opaque"] {
            let mut format = original.clone();
            assert!(format.rename_glyph_references("A", destination).is_err());
            assert_eq!(format, original);
        }
        let mut format = original.clone();
        let names = format
            .lib
            .get_mut(PSNAMES_KEY)
            .unwrap()
            .as_dictionary_mut()
            .unwrap();
        names.insert("A.alt".into(), "another.production".into());
        let original = format.clone();
        assert!(format.rename_glyph_references("A", "A.alt").is_err());
        assert_eq!(format, original);
    }

    #[test]
    fn malformed_keys_and_affected_values_reject_without_mutation() {
        for key in [GLYPH_ORDER_KEY, PSNAMES_KEY] {
            let mut format = format_with_references();
            format.lib.insert(key.into(), "malformed".into());
            let original = format.clone();
            assert!(format.rename_glyph_references("A", "A.alt").is_err());
            assert_eq!(format, original);
            assert!(format.remove_glyph_references("A").is_err());
            assert_eq!(format, original);
        }
        let mut format = format_with_references();
        format
            .lib
            .get_mut(PSNAMES_KEY)
            .unwrap()
            .as_dictionary_mut()
            .unwrap()
            .insert("A".into(), 42.into());
        let original = format.clone();
        assert!(format.rename_glyph_references("A", "A.alt").is_err());
        assert_eq!(format, original);
        assert!(format.remove_glyph_references("A").is_err());
        assert_eq!(format, original);
    }

    #[test]
    fn record_is_glyph_free_and_preserves_exact_format_structure() {
        let mut font = Font::new();
        font.meta.creator = Some("com.runebender.test".into());
        font.font_info.family_name = Some("Canonical owner".into());
        font.lib.insert("com.example.font".into(), "opaque".into());
        font.default_layer_mut().color = Some(Color::new(0.1, 0.2, 0.3, 0.4).unwrap());
        font.default_layer_mut()
            .lib
            .insert("com.example.default".into(), 1.into());
        font.default_layer_mut().insert_glyph(Glyph::new("A"));
        let background = font.layers.new_layer("public.background").unwrap();
        background.color = Some(Color::new(0.4, 0.3, 0.2, 0.1).unwrap());
        background
            .lib
            .insert("com.example.background".into(), 2.into());
        background.insert_glyph(Glyph::new("B"));
        font.features = "feature liga { sub A A by A; } liga;".into();
        font.data
            .insert(PathBuf::from("com.example/payload.bin"), vec![1, 2, 3])
            .unwrap();
        let image = include_bytes!("../../../tests/fixtures/variable/reference.png").to_vec();
        font.images
            .insert(PathBuf::from("reference.png"), image.clone())
            .unwrap();

        let expected_layers = font
            .layers
            .iter()
            .map(|layer| {
                (
                    layer.name().to_string(),
                    layer.path().to_path_buf(),
                    layer.color,
                    layer.lib.clone(),
                )
            })
            .collect::<Vec<_>>();
        let format = SourceFormatData::from_ufo(&font);
        let template = format.to_ufo_template();

        assert!(template.layers.iter().all(norad::Layer::is_empty));
        assert_eq!(
            template
                .layers
                .iter()
                .map(|layer| {
                    (
                        layer.name().to_string(),
                        layer.path().to_path_buf(),
                        layer.color,
                        layer.lib.clone(),
                    )
                })
                .collect::<Vec<_>>(),
            expected_layers
        );
        assert_eq!(template.meta, font.meta);
        assert_eq!(template.lib, font.lib);
        assert!(template.font_info.family_name.is_none());
        assert!(template.features.is_empty());
        assert_eq!(
            template
                .data
                .get(Path::new("com.example/payload.bin"))
                .unwrap()
                .unwrap()
                .as_ref(),
            [1, 2, 3]
        );
        assert_eq!(
            template
                .images
                .get(Path::new("reference.png"))
                .unwrap()
                .unwrap()
                .as_ref(),
            image
        );
    }
}
