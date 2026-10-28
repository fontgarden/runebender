// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Immutable reference metadata and real face selection, independent of filesystem discovery.

use skrifa::{MetadataProvider as _, raw::TableProvider as _};
use std::sync::Arc;

/// One actual font face, grouped by family and style rather than filename or weight.
#[derive(Debug)]
pub struct ReferenceFace {
    /// Exact bytes used for coverage and rendering.
    pub bytes: Arc<Vec<u8>>,
    /// Family name and non-default style qualifiers.
    pub label: String,
    /// Case-insensitive family, width class and italic flag.
    pub group: (String, u16, bool),
    /// Supported weight range; equal endpoints mean a static weight.
    pub weights: (f32, f32),
    /// Default variation coordinates.
    pub axes: Vec<(String, f32)>,
}
impl ReferenceFace {
    /// Parse a single TTF/OTF face without consulting system fonts.
    pub fn parse(bytes: Arc<Vec<u8>>) -> Result<Self, String> {
        let font = skrifa::FontRef::new(&bytes).map_err(|e| e.to_string())?;
        let name = |id| {
            font.localized_strings(id)
                .english_or_first()
                .map(|s| s.chars().collect::<String>())
        };
        let family = name(skrifa::string::StringId::TYPOGRAPHIC_FAMILY_NAME)
            .or_else(|| name(skrifa::string::StringId::FAMILY_NAME))
            .filter(|s| !s.is_empty())
            .ok_or("Missing family name")?;
        let os2 = font.os2().map_err(|e| e.to_string())?;
        let italic = os2.fs_selection().bits() & (1 | 512) != 0;
        let width = os2.us_width_class();
        let mut label = family.clone();
        if width != 5 {
            label.push_str(&format!(" · Width {width}"));
        }
        if italic {
            label.push_str(" Italic");
        }
        let axes = font
            .axes()
            .iter()
            .map(|axis| (axis.tag().to_string(), axis.default_value()))
            .collect();
        let weights = font.axes().get_by_tag(skrifa::Tag::new(b"wght")).map_or(
            (
                f32::from(os2.us_weight_class()),
                f32::from(os2.us_weight_class()),
            ),
            |axis| (axis.min_value(), axis.max_value()),
        );
        if !weights.0.is_finite() || !weights.1.is_finite() || weights.0 > weights.1 {
            return Err("Invalid weight range".into());
        }
        Ok(Self {
            bytes,
            label,
            group: (family.to_lowercase(), width, italic),
            weights,
            axes,
        })
    }
    /// Clamp to this face's actual weight range.
    pub fn weight(&self, requested: f32) -> f32 {
        requested.clamp(self.weights.0, self.weights.1)
    }
    /// Rank by nearest weight, lower on ties, then prefer a variable face.
    /// Callers iterate in filename order for otherwise identical ties.
    pub fn compare(&self, other: &Self, requested: f32) -> std::cmp::Ordering {
        let a = self.weight(requested);
        let b = other.weight(requested);
        (a - requested)
            .abs()
            .total_cmp(&(b - requested).abs())
            .then_with(|| a.total_cmp(&b))
            .then_with(|| other.variable().cmp(&self.variable()))
    }
    fn variable(&self) -> bool {
        self.axes.iter().any(|(tag, _)| tag == "wght")
    }
    /// Set weight explicitly, preserving this font's other axis defaults.
    pub fn variations(&self, requested: f32) -> String {
        self.axes
            .iter()
            .map(|(tag, value)| {
                format!(
                    "'{tag}' {}",
                    if tag == "wght" {
                        self.weight(requested)
                    } else {
                        *value
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
    /// Draw an encoded character at the selected weight in the document's em units.
    pub fn drawing(
        &self,
        character: char,
        requested: f32,
        units_per_em: f64,
    ) -> Result<(kurbo::BezPath, f64), String> {
        use skrifa::{
            instance::Size,
            outline::{DrawSettings, OutlinePen},
        };
        #[derive(Default)]
        struct Pen(kurbo::BezPath);
        impl OutlinePen for Pen {
            fn move_to(&mut self, x: f32, y: f32) {
                self.0.move_to((f64::from(x), f64::from(y)));
            }
            fn line_to(&mut self, x: f32, y: f32) {
                self.0.line_to((f64::from(x), f64::from(y)));
            }
            fn quad_to(&mut self, x: f32, y: f32, x1: f32, y1: f32) {
                self.0
                    .quad_to((f64::from(x), f64::from(y)), (f64::from(x1), f64::from(y1)));
            }
            fn curve_to(&mut self, x: f32, y: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
                self.0.curve_to(
                    (f64::from(x), f64::from(y)),
                    (f64::from(x1), f64::from(y1)),
                    (f64::from(x2), f64::from(y2)),
                );
            }
            fn close(&mut self) {
                self.0.close_path();
            }
        }
        let font = skrifa::FontRef::new(&self.bytes).map_err(|e| e.to_string())?;
        let id = font
            .charmap()
            .map(character)
            .filter(|id| id.to_u32() != 0)
            .ok_or_else(|| format!("Not covered: U+{:04X}", u32::from(character)))?;
        let location = font
            .axes()
            .location([(skrifa::Tag::new(b"wght"), self.weight(requested))]);
        let metrics = font.metrics(Size::unscaled(), &location);
        let scale = units_per_em / f64::from(metrics.units_per_em.max(1));
        let advance = f64::from(
            font.glyph_metrics(Size::unscaled(), &location)
                .advance_width(id)
                .unwrap_or(0.0),
        ) * scale;
        let mut pen = Pen::default();
        if let Some(outline) = font.outline_glyphs().get(id) {
            outline
                .draw(
                    DrawSettings::unhinted(Size::unscaled(), &location),
                    &mut pen,
                )
                .map_err(|e| e.to_string())?;
        }
        Ok((kurbo::Affine::scale(scale) * pen.0, advance))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn face(min: f32, max: f32) -> ReferenceFace {
        let mut face = ReferenceFace::parse(Arc::new(
            include_bytes!("../../assets/fonts/VirtuaGrotesk-Regular.ttf").to_vec(),
        ))
        .unwrap();
        face.weights = (min, max);
        face.axes = if min == max {
            vec![]
        } else {
            vec![("wght".into(), min)]
        };
        face
    }
    #[test]
    fn selection_uses_real_weights_and_lower_ties() {
        assert!(
            face(500.0, 500.0)
                .compare(&face(600.0, 600.0), 550.0)
                .is_lt()
        );
        assert!(
            face(600.0, 600.0)
                .compare(&face(500.0, 500.0), 560.0)
                .is_lt()
        );
        assert!(
            face(100.0, 900.0)
                .compare(&face(400.0, 400.0), 400.0)
                .is_lt()
        );
        assert_eq!(face(400.0, 700.0).weight(900.0), 700.0);
        assert_eq!(face(400.0, 400.0).variations(700.0), "");
    }
    #[test]
    fn glyphs_scale_with_em_and_missing_coverage_is_explicit() {
        let face = face(400.0, 400.0);
        let (a, advance) = face.drawing('A', 400.0, 1000.0).unwrap();
        let (b, doubled) = face.drawing('A', 400.0, 2000.0).unwrap();
        assert_eq!(kurbo::Affine::scale(2.0) * a, b);
        assert_eq!(advance * 2.0, doubled);
        assert!(face.drawing('\u{10ffff}', 400.0, 1000.0).is_err());
    }
}
