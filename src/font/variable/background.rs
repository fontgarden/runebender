// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Atomic background edits scoped to one glyph and its source layer container.

use super::{GlyphId, GlyphLayerAddress, LayerId, SourceId, VariableData};
use crate::font::babelfont::{CanonicalLayerSnapshot, LayerEditDraft, layer_key};
use crate::font::persistence::source_format::LayerContainerCreation;

const BACKGROUND_NAMES: [&str; 2] = ["public.background", "background"];

#[derive(Clone, Debug)]
pub(in crate::font) struct BackgroundLayerEdit {
    pub address: GlyphLayerAddress,
    pub before: Option<CanonicalLayerSnapshot>,
    pub after: Option<CanonicalLayerSnapshot>,
}

/// Only consulted/edited layers and the structure this command can actually change.
#[derive(Clone, Debug)]
pub(in crate::font) struct BackgroundEdit {
    pub glyph: String,
    glyph_id: GlyphId,
    pub background: LayerId,
    pub label: &'static str,
    pub reads: Vec<CanonicalLayerSnapshot>,
    pub writes: Vec<BackgroundLayerEdit>,
    before_order: Vec<Option<String>>,
    after_order: Vec<Option<String>>,
    container: Option<LayerContainerCreation>,
}

impl VariableData {
    pub(in crate::font) fn background_layer_id(&self, source: SourceId) -> Option<LayerId> {
        let format = self.source_formats.get(&source)?;
        let name = BACKGROUND_NAMES
            .into_iter()
            .find(|name| format.contains_layer(name))
            .unwrap_or(BACKGROUND_NAMES[0]);
        Some(LayerId {
            source,
            name: name.into(),
        })
    }

    pub(in crate::font) fn copy_layer_to_background(
        &self,
        address: &GlyphLayerAddress,
    ) -> Result<Option<BackgroundEdit>, String> {
        let background = self
            .background_layer_id(address.layer.source)
            .ok_or("unknown source")?;
        if address.layer == background {
            return Err("the background cannot copy itself".into());
        }
        let source = self
            .layer_snapshot(address)
            .ok_or("missing foreground glyph layer")?;
        let target = GlyphLayerAddress {
            glyph: address.glyph.clone(),
            layer: background.clone(),
        };
        let before = self.background_snapshot(&target)?;
        let after = if let Some(before) = &before {
            let (layer, preserved) = before.clone().into_parts();
            let mut draft = LayerEditDraft::new(layer, preserved);
            if !draft
                .replace_layer_contours(source.view())
                .map_err(|error| error.to_string())?
            {
                return Ok(None);
            }
            let (layer, preserved) = draft.into_parts();
            CanonicalLayerSnapshot::new(target.clone(), layer, preserved)
        } else {
            source.contours_only_at(target.clone())
        };
        self.background_edit(
            &address.glyph,
            background,
            "send to background",
            vec![source],
            vec![BackgroundLayerEdit {
                address: target,
                before,
                after: Some(after),
            }],
        )
        .map(Some)
    }

    pub(in crate::font) fn swap_layer_with_background(
        &self,
        address: &GlyphLayerAddress,
    ) -> Result<Option<BackgroundEdit>, String> {
        let background = self
            .background_layer_id(address.layer.source)
            .ok_or("unknown source")?;
        if address.layer == background {
            return Err("the background cannot swap with itself".into());
        }
        let foreground = self
            .layer_snapshot(address)
            .ok_or("missing foreground glyph layer")?;
        let target = GlyphLayerAddress {
            glyph: address.glyph.clone(),
            layer: background.clone(),
        };
        let back = self
            .background_snapshot(&target)?
            .ok_or("missing background glyph layer")?;
        let (layer, preserved) = foreground.clone().into_parts();
        let mut front_draft = LayerEditDraft::new(layer, preserved);
        let (layer, preserved) = back.clone().into_parts();
        let mut back_draft = LayerEditDraft::new(layer, preserved);
        let front_changed = front_draft
            .replace_layer_contours_only(back.view())
            .map_err(|error| error.to_string())?;
        let back_changed = back_draft
            .replace_layer_contours_only(foreground.view())
            .map_err(|error| error.to_string())?;
        let width_changed = back_draft
            .set_width(foreground.view().width())
            .map_err(|error| error.to_string())?;
        if !front_changed && !back_changed && !width_changed {
            return Ok(None);
        }
        let (layer, preserved) = front_draft.into_parts();
        let front_after = CanonicalLayerSnapshot::new(address.clone(), layer, preserved);
        let (layer, preserved) = back_draft.into_parts();
        let back_after = CanonicalLayerSnapshot::new(target.clone(), layer, preserved);
        self.background_edit(
            &address.glyph,
            background,
            "swap background",
            Vec::new(),
            vec![
                BackgroundLayerEdit {
                    address: address.clone(),
                    before: Some(foreground),
                    after: Some(front_after),
                },
                BackgroundLayerEdit {
                    address: target,
                    before: Some(back),
                    after: Some(back_after),
                },
            ],
        )
        .map(Some)
    }

    pub(in crate::font) fn clear_background_layer(
        &self,
        glyph: &str,
        source: SourceId,
    ) -> Result<Option<BackgroundEdit>, String> {
        let background = self.background_layer_id(source).ok_or("unknown source")?;
        if !self.glyphs.contains_key(glyph) {
            return Ok(None);
        }
        let target = GlyphLayerAddress {
            glyph: glyph.into(),
            layer: background.clone(),
        };
        let Some(before) = self.background_snapshot(&target)? else {
            return Ok(None);
        };
        self.background_edit(
            glyph,
            background,
            "clear background",
            Vec::new(),
            vec![BackgroundLayerEdit {
                address: target,
                before: Some(before),
                after: None,
            }],
        )
        .map(Some)
    }

    fn background_snapshot(
        &self,
        address: &GlyphLayerAddress,
    ) -> Result<Option<CanonicalLayerSnapshot>, String> {
        let glyph = self.glyphs.get(&address.glyph).ok_or("missing glyph")?;
        let geometry = self
            .font
            .glyphs
            .get(&address.glyph)
            .ok_or("missing glyph geometry")?;
        let preserved = glyph.layers.contains_key(&address.layer);
        if preserved != geometry.get_layer(&layer_key(&address.layer)).is_some() {
            return Err("background preservation and geometry disagree".into());
        }
        Ok(self.layer_snapshot(address))
    }

    fn background_edit(
        &self,
        glyph: &str,
        background: LayerId,
        label: &'static str,
        reads: Vec<CanonicalLayerSnapshot>,
        writes: Vec<BackgroundLayerEdit>,
    ) -> Result<BackgroundEdit, String> {
        let geometry = self
            .font
            .glyphs
            .get(glyph)
            .ok_or("missing glyph geometry")?;
        let before_order: Vec<_> = geometry
            .layers
            .iter()
            .map(|layer| layer.id.as_ref().map(ToString::to_string))
            .collect();
        let mut after_order = before_order.clone();
        for write in &writes {
            let key = layer_key(&write.address.layer);
            if write.after.is_none() {
                after_order.retain(|id| id.as_deref() != Some(key.as_str()));
            } else if write.before.is_none() {
                after_order.push(Some(key));
            }
        }
        let container = self
            .source_formats
            .get(&background.source)
            .ok_or("unknown source")?
            .prepare_layer_creation(&background.name)?;
        Ok(BackgroundEdit {
            glyph: glyph.into(),
            glyph_id: self.glyphs.get(glyph).ok_or("missing glyph")?.id,
            background,
            label,
            reads,
            writes,
            before_order,
            after_order,
            container,
        })
    }

    pub(in crate::font) fn check_background_edit(
        &self,
        edit: &BackgroundEdit,
        forward: bool,
    ) -> Result<(), String> {
        let glyph = self
            .glyphs
            .get(&edit.glyph)
            .ok_or("background glyph disappeared")?;
        if glyph.id != edit.glyph_id {
            return Err("background glyph identity changed".into());
        }
        let geometry = self
            .font
            .glyphs
            .get(&edit.glyph)
            .ok_or("background glyph geometry disappeared")?;
        let order = if forward {
            &edit.before_order
        } else {
            &edit.after_order
        };
        if !geometry
            .layers
            .iter()
            .map(|layer| layer.id.as_deref())
            .eq(order.iter().map(Option::as_deref))
        {
            return Err("background glyph layer order changed".into());
        }
        let format = self
            .source_formats
            .get(&edit.background.source)
            .ok_or("background source disappeared")?;
        if let Some(container) = &edit.container {
            if !format.matches_layer_creation(container, forward) {
                return Err("background layer container changed".into());
            }
            if !forward
                && self.glyphs.iter().any(|(name, glyph)| {
                    name != &edit.glyph && glyph.layers.contains_key(&edit.background)
                })
            {
                return Err("another glyph now uses the created background container".into());
            }
        } else if !format.contains_layer(&edit.background.name) {
            return Err("background layer container disappeared".into());
        }
        for read in &edit.reads {
            if self.layer_matches_snapshot(read) != Some(true) {
                return Err("background foreground dependency changed".into());
            }
        }
        for write in &edit.writes {
            let expected = if forward { &write.before } else { &write.after };
            let matches = match expected {
                Some(expected) => self.layer_matches_snapshot(expected) == Some(true),
                None => {
                    !glyph.layers.contains_key(&write.address.layer)
                        && geometry
                            .get_layer(&layer_key(&write.address.layer))
                            .is_none()
                }
            };
            if !matches {
                return Err("background layer changed after history capture".into());
            }
        }
        Ok(())
    }

    pub(in crate::font) fn apply_background_edit(
        &mut self,
        edit: &mut BackgroundEdit,
        forward: bool,
    ) -> Result<(), String> {
        self.check_background_edit(edit, forward)?;
        if let Some(container) = &mut edit.container {
            self.source_formats
                .get_mut(&edit.background.source)
                .expect("validated background source")
                .install_layer_creation(container, forward)?;
        }
        // Container insertion validates before mutation; all remaining publication is infallible.
        let order = if forward {
            &edit.after_order
        } else {
            &edit.before_order
        };
        let glyph = self
            .glyphs
            .get_mut(&edit.glyph)
            .expect("validated background glyph");
        let geometry = self
            .font
            .glyphs
            .get_mut(&edit.glyph)
            .expect("validated background geometry");
        for write in &edit.writes {
            let replacement = if forward { &write.after } else { &write.before };
            let key = layer_key(&write.address.layer);
            if let Some(replacement) = replacement {
                let (layer, preserved) = replacement.clone().into_parts();
                glyph.layers.insert(write.address.layer.clone(), preserved);
                if let Some(existing) = geometry.get_layer_mut(&key) {
                    *existing = layer;
                } else {
                    let index = order
                        .iter()
                        .position(|id| id.as_deref() == Some(key.as_str()))
                        .expect("replacement layer has a recorded position");
                    geometry.layers.insert(index, layer);
                }
            } else {
                glyph.layers.remove(&write.address.layer);
                geometry
                    .layers
                    .retain(|layer| layer.id.as_deref() != Some(key.as_str()));
            }
        }
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
}
