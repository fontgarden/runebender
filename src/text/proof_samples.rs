// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Proof samples: short texts that show a glyph in the context of its script.
//!
//! The format is the `sample` tables of `sources/proof-samples.toml` in a Font.Garden project,
//! which the web proof of fgtools also reads.
//! Runebender ships its own defaults in `assets/data/proof-samples.toml`.
//! A project file adds samples, replaces a default sample with the same key, and with
//! `defaults = false` removes the defaults.
//! The `text` tables of that format are for the long reading proof of the web proof;
//! this module ignores them.
//!
//! In a sample text, [`GLYPH_PLACEHOLDER`] stands for the character of the open glyph.
//!
//! Two optional keys of a sample are Runebender's own: `covers` lists the code points that the
//! sample tests, and `carriers` the other code points that its text can use, such as base
//! letters.
//! Each entry is `U+0628` or a range `U+06D6..U+06DC`.
//! The tests of the default samples check each text against them.

use serde::Deserialize;
use serde::de::{MapAccess, Visitor};
use std::fmt;
use std::ops::RangeInclusive;

/// The default samples, in the format of a project file.
pub const DEFAULT_SAMPLES: &str = include_str!("../../assets/data/proof-samples.toml");

/// The text that a sample uses for the character of the open glyph.
pub const GLYPH_PLACEHOLDER: &str = "{glyph}";

/// A script that a sample can be for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SampleScript {
    /// The Latin script.
    Latin,
    /// The Hebrew script.
    Hebrew,
    /// The Arabic script.
    Arabic,
}

impl SampleScript {
    /// The script of a character, or `None` for a character of no listed script, such as a
    /// European digit, punctuation or a space.
    pub fn of(character: char) -> Option<Self> {
        match u32::from(character) {
            0x0590..=0x05FF | 0xFB1D..=0xFB4F => Some(Self::Hebrew),
            0x0600..=0x06FF
            | 0x0750..=0x077F
            | 0x0870..=0x08FF
            | 0xFB50..=0xFDFF
            | 0xFE70..=0xFEFF => Some(Self::Arabic),
            0x0000..=0x024F
            | 0x1E00..=0x1EFF
            | 0x2C60..=0x2C7F
            | 0xA720..=0xA7FF
            | 0xAB30..=0xAB6F
            | 0xFB00..=0xFB06
                if character.is_alphabetic() =>
            {
                Some(Self::Latin)
            }
            _ => None,
        }
    }

    /// The script with this name in a sample file.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Latin" => Some(Self::Latin),
            "Hebrew" => Some(Self::Hebrew),
            "Arabic" => Some(Self::Arabic),
            _ => None,
        }
    }

    /// The name of the script in a sample file.
    pub fn name(self) -> &'static str {
        match self {
            Self::Latin => "Latin",
            Self::Hebrew => "Hebrew",
            Self::Arabic => "Arabic",
        }
    }
}

/// One sample of a sample file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofSample {
    /// The key of the sample table, as in `[sample.latin-words]`.
    pub key: String,
    /// The name that the sample menu shows.
    pub label: String,
    /// The scripts that the sample is for; empty for a sample for every glyph.
    pub scripts: Vec<SampleScript>,
    /// The text, which can contain [`GLYPH_PLACEHOLDER`].
    pub text: String,
    /// The language tag, when the file gives one.
    pub lang: Option<String>,
    /// The code points that the sample tests, from `covers`; empty when the file gives none.
    pub covers: Vec<RangeInclusive<char>>,
    /// The other code points that the text can use, from `carriers`.
    pub carriers: Vec<RangeInclusive<char>>,
}

impl ProofSample {
    /// Whether the sample is for a glyph of `script`.
    /// A sample without scripts, and every sample for a glyph without a script, match.
    pub fn matches(&self, script: Option<SampleScript>) -> bool {
        script.is_none_or(|script| self.scripts.is_empty() || self.scripts.contains(&script))
    }

    /// Whether the text contains [`GLYPH_PLACEHOLDER`].
    pub fn uses_glyph(&self) -> bool {
        self.text.contains(GLYPH_PLACEHOLDER)
    }

    /// The text with the character of the open glyph in place of [`GLYPH_PLACEHOLDER`], or
    /// `None` when the text needs a character and the glyph has none.
    pub fn text_for(&self, glyph: Option<char>) -> Option<String> {
        if !self.uses_glyph() {
            return Some(self.text.clone());
        }
        glyph.map(|c| {
            self.text
                .replace(GLYPH_PLACEHOLDER, c.encode_utf8(&mut [0; 4]))
        })
    }
}

/// The contents of one sample file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampleFile {
    /// Whether the default samples stay; `defaults = false` in the file removes them.
    pub defaults: bool,
    /// The samples, in the order of the file.
    pub samples: Vec<ProofSample>,
}

#[derive(Deserialize)]
struct RawFile {
    #[serde(default = "keep_defaults")]
    defaults: bool,
    #[serde(default)]
    sample: OrderedSamples,
}

fn keep_defaults() -> bool {
    true
}

#[derive(Deserialize)]
struct RawSample {
    label: String,
    text: String,
    #[serde(default)]
    scripts: Vec<String>,
    lang: Option<String>,
    #[serde(default)]
    covers: Vec<String>,
    #[serde(default)]
    carriers: Vec<String>,
}

/// Read `U+0628` or `U+06D6..U+06DC`.
fn code_points(entry: &str) -> Option<RangeInclusive<char>> {
    let one = |text: &str| {
        let hex = text.trim().strip_prefix("U+")?;
        u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
    };
    let (first, last) = match entry.split_once("..") {
        Some((first, last)) => (one(first)?, one(last)?),
        None => (one(entry)?, one(entry)?),
    };
    (first <= last).then_some(first..=last)
}

/// Read the `covers` or `carriers` entries of the sample with `key`.
fn code_point_list(
    key: &str,
    field: &str,
    entries: &[String],
) -> Result<Vec<RangeInclusive<char>>, String> {
    entries
        .iter()
        .map(|entry| {
            code_points(entry).ok_or_else(|| {
                format!("sample.{key}: {field} entry \"{entry}\" is not U+XXXX or U+XXXX..U+YYYY")
            })
        })
        .collect()
}

/// The sample tables in the order of the file, which a map type would not keep.
#[derive(Default)]
struct OrderedSamples(Vec<(String, RawSample)>);

impl<'de> Deserialize<'de> for OrderedSamples {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Tables;
        impl<'de> Visitor<'de> for Tables {
            type Value = OrderedSamples;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a table of sample tables")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut samples = Vec::new();
                while let Some(entry) = map.next_entry::<String, RawSample>()? {
                    samples.push(entry);
                }
                Ok(OrderedSamples(samples))
            }
        }
        deserializer.deserialize_map(Tables)
    }
}

/// Read a sample file.
///
/// # Errors
///
/// Returns a message for TOML that does not parse, a sample without `label` or `text`, an
/// unknown script name, and a `covers` or `carriers` entry that is not a code point or range.
pub fn parse(source: &str) -> Result<SampleFile, String> {
    let raw: RawFile = toml::from_str(source).map_err(|error| error.to_string())?;
    let samples = raw
        .sample
        .0
        .into_iter()
        .map(|(key, raw)| {
            let scripts = raw
                .scripts
                .iter()
                .map(|name| {
                    SampleScript::from_name(name).ok_or_else(|| {
                        format!(
                            "sample.{key}: unknown script \"{name}\"; \
                             the scripts are Latin, Hebrew and Arabic"
                        )
                    })
                })
                .collect::<Result<_, _>>()?;
            let covers = code_point_list(&key, "covers", &raw.covers)?;
            let carriers = code_point_list(&key, "carriers", &raw.carriers)?;
            Ok(ProofSample {
                key,
                label: raw.label,
                scripts,
                text: raw.text,
                lang: raw.lang,
                covers,
                carriers,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(SampleFile {
        defaults: raw.defaults,
        samples,
    })
}

/// The default samples of Runebender.
pub fn default_samples() -> Vec<ProofSample> {
    parse(DEFAULT_SAMPLES)
        .expect("the default sample file parses; a test checks it")
        .samples
}

/// The samples of a project: the defaults unless the file removes them, with each sample of
/// the file in place of the default with its key, and the other samples of the file after them.
pub fn merge(defaults: Vec<ProofSample>, file: SampleFile) -> Vec<ProofSample> {
    let mut merged = if file.defaults { defaults } else { Vec::new() };
    for sample in file.samples {
        match merged.iter_mut().find(|old| old.key == sample.key) {
            Some(old) => *old = sample,
            None => merged.push(sample),
        }
    }
    merged
}

/// The samples to offer for a glyph with this character: those that match its script and do
/// not need a character it does not have.
pub fn matching(
    samples: &[ProofSample],
    glyph: Option<char>,
) -> impl Iterator<Item = &ProofSample> {
    let script = glyph.and_then(SampleScript::of);
    samples
        .iter()
        .filter(move |sample| sample.matches(script) && (glyph.is_some() || !sample.uses_glyph()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys<'a>(samples: impl IntoIterator<Item = &'a ProofSample>) -> Vec<&'a str> {
        samples.into_iter().map(|s| s.key.as_str()).collect()
    }

    #[test]
    fn a_sample_file_parses_in_order_and_ignores_text_tables() {
        let file = parse(
            r#"
            [sample.zeta]
            label = "Zeta"
            text = "z"

            [sample.figures]
            label = "Figures"
            scripts = ["Latin"]
            text = "0123456789"
            lang = "en"

            [text.latin]
            label = "Latin text"
            [[text.latin.paragraphs]]
            text = "A paragraph."
            "#,
        )
        .unwrap();
        assert!(file.defaults);
        assert_eq!(keys(&file.samples), ["zeta", "figures"]);
        assert_eq!(file.samples[1].scripts, [SampleScript::Latin]);
        assert_eq!(file.samples[1].lang.as_deref(), Some("en"));
        assert!(file.samples[0].scripts.is_empty());
    }

    #[test]
    fn errors_name_the_sample_and_the_fault() {
        let unknown = parse("[sample.a]\nlabel = \"A\"\ntext = \"a\"\nscripts = [\"Greek\"]\n");
        assert!(
            unknown
                .unwrap_err()
                .contains("sample.a: unknown script \"Greek\"")
        );
        assert!(
            parse("[sample.a]\nlabel = \"A\"\n")
                .unwrap_err()
                .contains("text")
        );
        assert!(parse("[sample.a\n").is_err());
    }

    #[test]
    fn the_defaults_cover_three_scripts_with_a_glyph_spacing_sample_each() {
        let defaults = default_samples();
        for script in [
            SampleScript::Latin,
            SampleScript::Hebrew,
            SampleScript::Arabic,
        ] {
            let first = defaults
                .iter()
                .find(|sample| sample.scripts == [script])
                .unwrap();
            assert!(first.uses_glyph(), "{}", script.name());
        }
        for key in [
            "latin-words",
            "hebrew-words",
            "hebrew-marks",
            "latin-hebrew",
            "arabic-words",
            "arabic-joining",
            "arabic-marks",
            "latin-arabic",
        ] {
            assert!(defaults.iter().any(|sample| sample.key == key), "{key}");
        }
    }

    #[test]
    fn a_project_sample_replaces_the_default_with_its_key_and_others_follow() {
        let defaults = default_samples();
        let file = parse(
            "[sample.new]\nlabel = \"New\"\ntext = \"n\"\n\
             [sample.hebrew-words]\nlabel = \"Own words\"\nscripts = [\"Hebrew\"]\ntext = \"שלום\"\n",
        )
        .unwrap();
        let merged = merge(defaults.clone(), file);
        assert_eq!(merged.len(), defaults.len() + 1);
        let at = defaults
            .iter()
            .position(|s| s.key == "hebrew-words")
            .unwrap();
        assert_eq!(merged[at].label, "Own words");
        assert_eq!(merged.last().unwrap().key, "new");
    }

    #[test]
    fn defaults_false_keeps_only_the_samples_of_the_file() {
        let file =
            parse("defaults = false\n[sample.own]\nlabel = \"Own\"\ntext = \"o\"\n").unwrap();
        assert!(!file.defaults);
        assert_eq!(keys(&merge(default_samples(), file)), ["own"]);
    }

    #[test]
    fn the_menu_offers_the_samples_of_the_script_of_the_glyph() {
        let samples = default_samples();
        let hebrew = keys(matching(&samples, Some('ב')));
        assert_eq!(hebrew[0], "hebrew-glyph-spacing");
        assert!(hebrew.contains(&"latin-hebrew"));
        assert!(!hebrew.iter().any(|key| key.starts_with("arabic")));
        assert!(!hebrew.contains(&"latin-words"));
        let arabic = keys(matching(&samples, Some('ب')));
        assert_eq!(arabic[0], "arabic-glyph-spacing");
        assert!(arabic.contains(&"latin-arabic"));
        let latin = keys(matching(&samples, Some('n')));
        assert_eq!(latin[0], "latin-glyph-spacing");
        assert!(latin.contains(&"latin-hebrew") && latin.contains(&"latin-arabic"));
        // A character of no listed script, such as a digit, gets every sample.
        assert_eq!(matching(&samples, Some('5')).count(), samples.len());
        // A glyph without a character gets every sample that does not need one.
        let unencoded = keys(matching(&samples, None));
        assert_eq!(unencoded[0], "latin-words");
        assert!(!unencoded.iter().any(|key| key.ends_with("glyph-spacing")));
    }

    #[test]
    fn the_placeholder_takes_the_character_of_the_glyph() {
        let samples = default_samples();
        let hebrew = samples
            .iter()
            .find(|sample| sample.key == "hebrew-glyph-spacing")
            .unwrap();
        let text = hebrew.text_for(Some('ב')).unwrap();
        assert!(text.starts_with("בבב ה"), "{text}");
        assert!(!text.contains(GLYPH_PLACEHOLDER));
        assert_eq!(hebrew.text_for(None), None);
        let words = samples.iter().find(|s| s.key == "latin-words").unwrap();
        assert_eq!(words.text_for(None).as_deref(), Some("Hamburgefontsiv"));
    }

    fn in_ranges(ranges: &[RangeInclusive<char>], c: char) -> bool {
        ranges.iter().any(|range| range.contains(&c))
    }

    #[test]
    fn covers_and_carriers_parse_and_bad_entries_are_named() {
        let file = parse(
            "[sample.a]\nlabel = \"A\"\ntext = \"a\"\n\
             covers = [\"U+0628\", \"U+06D6..U+06DC\"]\ncarriers = [\"U+0020\"]\n",
        )
        .unwrap();
        assert_eq!(
            file.samples[0].covers,
            ['\u{0628}'..='\u{0628}', '\u{06D6}'..='\u{06DC}']
        );
        assert_eq!(file.samples[0].carriers, [' '..=' ']);
        for bad in ["0628", "U+XYZ", "U+06DC..U+06D6", "U+D800"] {
            let error = parse(&format!(
                "[sample.a]\nlabel = \"A\"\ntext = \"a\"\ncovers = [\"{bad}\"]\n"
            ))
            .unwrap_err();
            assert!(
                error.contains(&format!("sample.a: covers entry \"{bad}\"")),
                "{error}"
            );
        }
    }

    /// Each default sample that lists what it tests contains every code point of `covers`,
    /// and no code point outside `covers`, `carriers` and the space.
    #[test]
    fn every_default_sample_uses_only_and_all_the_code_points_it_claims() {
        let samples = default_samples();
        for sample in &samples {
            assert!(
                !sample.text.trim().is_empty(),
                "sample.{} is empty",
                sample.key
            );
            assert!(
                !sample.label.trim().is_empty(),
                "sample.{} has no label",
                sample.key
            );
            if sample.covers.is_empty() {
                continue;
            }
            for c in sample.text.chars() {
                assert!(
                    c == ' ' || in_ranges(&sample.covers, c) || in_ranges(&sample.carriers, c),
                    "sample.{}: U+{:04X} is not in covers or carriers",
                    sample.key,
                    u32::from(c)
                );
            }
            for c in sample.covers.iter().flat_map(|range| range.clone()) {
                assert!(
                    sample.text.contains(c),
                    "sample.{}: covers U+{:04X}, which the text lacks",
                    sample.key,
                    u32::from(c)
                );
            }
        }
    }

    /// The Quranic and Arabic samples together test every sign that the plan lists.
    #[test]
    fn the_arabic_samples_cover_the_quranic_repertoire() {
        let samples = default_samples();
        let covered = |c: char| samples.iter().any(|s| in_ranges(&s.covers, c));
        let required = [
            '\u{064B}'..='\u{0657}',
            '\u{0640}'..='\u{0640}',
            '\u{0670}'..='\u{0671}',
            '\u{0621}'..='\u{0626}',
            '\u{06D6}'..='\u{06ED}',
            '\u{0660}'..='\u{0669}',
            '\u{06F0}'..='\u{06F9}',
            '\u{08D3}'..='\u{08FF}',
            '\u{FD3E}'..='\u{FD3F}',
        ];
        for c in required.into_iter().flatten() {
            assert!(covered(c), "no sample covers U+{:04X}", u32::from(c));
        }
        for key in [
            "quran-basmala",
            "quran-pause-marks",
            "quran-end-of-ayah",
            "quran-section-marks",
            "quran-small-letters",
            "quran-small-waw-yeh",
            "quran-alef-wasla",
            "arabic-hamza-madda",
            "quran-tanween",
            "arabic-stacked-marks",
            "arabic-lam-ligatures",
            "arabic-extended-marks",
            "arabic-tatweel-marks",
            "persian-letters",
            "urdu-letters",
            "kurdish-letters",
        ] {
            let sample = samples.iter().find(|s| s.key == key).expect(key);
            assert_eq!(sample.scripts, [SampleScript::Arabic], "{key}");
            assert!(!sample.covers.is_empty(), "{key}");
        }
    }

    #[test]
    fn the_stacked_marks_come_in_both_orders_and_hamza_in_both_spellings() {
        let samples = default_samples();
        let text = |key: &str| &samples.iter().find(|s| s.key == key).unwrap().text;
        let stacked = text("arabic-stacked-marks");
        for (a, b) in [
            ('\u{0651}', '\u{064E}'),
            ('\u{0651}', '\u{0650}'),
            ('\u{0651}', '\u{0670}'),
        ] {
            assert!(stacked.contains(&format!("\u{0628}{a}{b}")), "{a}{b}");
            assert!(stacked.contains(&format!("\u{0628}{b}{a}")), "{b}{a}");
        }
        let hamza = text("arabic-hamza-madda");
        for (precomposed, decomposed) in [
            ("\u{0622}", "\u{0627}\u{0653}"),
            ("\u{0623}", "\u{0627}\u{0654}"),
            ("\u{0625}", "\u{0627}\u{0655}"),
            ("\u{0624}", "\u{0648}\u{0654}"),
            ("\u{0626}", "\u{064A}\u{0654}"),
        ] {
            assert!(hamza.contains(&format!("{precomposed} {decomposed}")));
        }
    }

    #[test]
    fn scripts_of_characters() {
        assert_eq!(SampleScript::of('A'), Some(SampleScript::Latin));
        assert_eq!(SampleScript::of('é'), Some(SampleScript::Latin));
        assert_eq!(SampleScript::of('ש'), Some(SampleScript::Hebrew));
        assert_eq!(SampleScript::of('\u{05B8}'), Some(SampleScript::Hebrew));
        assert_eq!(SampleScript::of('ك'), Some(SampleScript::Arabic));
        assert_eq!(SampleScript::of('7'), None);
        assert_eq!(SampleScript::of(' '), None);
    }
}
