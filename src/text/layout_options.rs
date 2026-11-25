// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The script, language system and feature tags that a font's GSUB and GPOS tables expose.

use std::collections::BTreeSet;

use skrifa::raw::TableProvider as _;
use skrifa::{FontRef, Tag};

/// One script and language system registered in a font's layout tables.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LayoutSystem {
    /// The OpenType script tag, including any trailing spaces.
    pub script: Tag,
    /// The OpenType language tag, or the script's default system.
    pub language: Option<Tag>,
}

impl LayoutSystem {
    /// The Unicode shaping script and `HarfBuzz` language override for this exact OpenType system.
    /// The private-use subtags are understood by both `HarfRust` and `HarfBuzz`.
    pub fn shaping_options(self) -> (String, String) {
        let tag = self.script.to_string();
        let raw = tag.trim_end();
        // OpenType uses versioned script tags for Indic shaping models and shortened
        // names for a few scripts. The shaper needs their ISO 15924 names as well.
        let script = match raw {
            "DFLT" => "zzzz",
            "jamo" => "hang",
            "math" => "zmth",
            "bng2" | "bng3" => "beng",
            "dev2" | "dev3" => "deva",
            "gjr2" | "gjr3" => "gujr",
            "gur2" | "gur3" => "guru",
            "knd2" | "knd3" => "knda",
            "mlm2" | "mlm3" => "mlym",
            "mym2" => "mymr",
            "ory2" | "ory3" => "orya",
            "tel2" | "tel3" => "telu",
            "tml2" | "tml3" => "taml",
            "lao" => "laoo",
            "nko" => "nkoo",
            "vai" => "vaii",
            "yi" => "yiii",
            raw => raw,
        };
        let mut language = format!("x-hbsc{raw}");
        if let Some(tag) = self.language {
            language.push_str("-hbot");
            language.push_str(tag.to_string().trim_end());
        }
        (script.into(), language.to_ascii_lowercase())
    }
}

/// Distinct tags available in the layout tables of the displayed font.
#[derive(Debug, Default)]
pub struct LayoutOptions {
    /// The script and language choices, sorted by their OpenType tags.
    pub systems: BTreeSet<LayoutSystem>,
    /// The feature tags from both substitution and positioning, in lexical order.
    pub features: BTreeSet<Tag>,
}

impl LayoutOptions {
    /// Inspect existing tables without compiling source or shaping any text.
    pub fn from_font(bytes: &[u8]) -> Self {
        let mut options = Self::default();
        let Ok(font) = FontRef::new(bytes) else {
            return options;
        };
        let gsub = font.gsub().ok();
        let gpos = font.gpos().ok();
        for list in [
            gsub.as_ref().and_then(|t| t.feature_list().ok()),
            gpos.as_ref().and_then(|t| t.feature_list().ok()),
        ]
        .into_iter()
        .flatten()
        {
            options
                .features
                .extend(list.feature_records().iter().map(|rec| rec.feature_tag()));
        }
        for list in [
            gsub.as_ref().and_then(|t| t.script_list().ok()),
            gpos.as_ref().and_then(|t| t.script_list().ok()),
        ]
        .into_iter()
        .flatten()
        {
            for record in list.script_records() {
                let script = record.script_tag();
                let Ok(table) = record.script(list.offset_data()) else {
                    continue;
                };
                if table.default_lang_sys().is_some_and(|table| table.is_ok()) {
                    options.systems.insert(LayoutSystem {
                        script,
                        language: None,
                    });
                }
                for language in table.lang_sys_records() {
                    options.systems.insert(LayoutSystem {
                        script,
                        language: Some(language.lang_sys_tag()),
                    });
                }
            }
        }
        options
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::shape::{ShapingFont, ShapingGlyph, ShapingSource};

    #[test]
    fn table_choices_enable_an_alternate_and_select_a_language_specific_lookup() {
        let font = ShapingFont::build(&ShapingSource {
            units_per_em: 1000.0,
            glyphs: [
                (".notdef", vec![]),
                ("A", vec![65]),
                ("A.alt", vec![]),
                ("A.locl", vec![]),
            ]
            .into_iter()
            .map(|(name, unicodes)| ShapingGlyph {
                name: name.into(),
                advance: 600.0,
                unicodes,
            })
            .collect(),
            features: "languagesystem latn dflt; languagesystem latn TRK;
                feature locl { script latn; language TRK exclude_dflt; sub A by A.locl; } locl;
                feature ss01 { sub A by A.alt; } ss01;
                feature kern { pos A.alt A.alt -70; } kern;"
                .into(),
        })
        .unwrap();
        let options = LayoutOptions::from_font(font.bytes());
        assert_eq!(
            options
                .features
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["kern", "locl", "ss01"]
        );
        let system = |language| LayoutSystem {
            script: Tag::new(b"latn"),
            language,
        };
        let default = system(None);
        let turkish = system(Some(Tag::new(b"TRK ")));
        assert!(options.systems.contains(&default));
        assert!(options.systems.contains(&turkish));
        let shape = |system: LayoutSystem, features: &[(String, bool)]| {
            let (script, language) = system.shaping_options();
            font.shape_with_options("AA", false, features, Some(&script), Some(&language))
                .unwrap()
        };
        let ordinary = shape(default, &[]);
        assert_eq!(font.glyph_name(ordinary[0].glyph_id), Some("A"));
        let localized = shape(turkish, &[]);
        assert_eq!(font.glyph_name(localized[0].glyph_id), Some("A.locl"));
        let alternate = shape(default, &[("ss01".into(), true)]);
        assert_eq!(font.glyph_name(alternate[0].glyph_id), Some("A.alt"));
        assert_eq!(alternate[0].x_advance, 530.0);
        assert_eq!(
            shape(default, &[("ss01".into(), true), ("kern".into(), false)])[0].x_advance,
            600.0
        );
    }
}
