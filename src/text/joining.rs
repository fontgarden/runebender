// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Platform-independent shaping helpers shared by Runebender frontends.
//!
//! The script and the joining type of a character come from the Unicode Character Database
//! through `icu_properties`, the crate that the kerning writer of fontc reads the same data
//! from, so the text line and the exported font agree on which characters are Arabic.

use icu_properties::CodePointMapData;
use icu_properties::props::{JoiningType, Script};
use icu_properties::script::ScriptWithExtensions;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// The contextual form an Arabic letter takes based on its neighbors.
pub enum PositionalForm {
    /// Not joined on either side.
    Isolated,
    /// Joined to the following letter only.
    Initial,
    /// Joined on both sides.
    Medial,
    /// Joined to the preceding letter only.
    Final,
}

impl PositionalForm {
    /// Returns the glyph-name suffix for this form, such as `.init`; empty for `Isolated`.
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Isolated => "",
            Self::Initial => ".init",
            Self::Medial => ".medi",
            Self::Final => ".fina",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// How a character joins with its neighbors: the Unicode `Joining_Type` property.
pub enum ArabicJoiningType {
    /// Joins on both sides, like `beh` (`D`).
    Dual,
    /// Joins to the preceding letter only, like `alef` or `dal` (`R`).
    Right,
    /// Joins to the following letter only (`L`), as in Manichaean and Phags-pa.
    Left,
    /// Never joins, such as `hamza` or a Latin letter (`U`).
    NonJoining,
    /// Forces joining on both sides, like the tatweel (`U+0640`) and the zero width joiner
    /// (`C`).
    JoinCausing,
    /// Marks that are skipped when finding the joining neighbors (`T`).
    Transparent,
}

impl ArabicJoiningType {
    /// Returns `true` when a character of this type connects to the letter after it.
    pub const fn joins_forward(self) -> bool {
        matches!(self, Self::Dual | Self::Left | Self::JoinCausing)
    }

    /// Returns `true` when a character of this type connects to the letter before it.
    pub const fn joins_backward(self) -> bool {
        matches!(self, Self::Dual | Self::Right | Self::JoinCausing)
    }

    /// Returns `true` for `Transparent`.
    pub const fn is_transparent(self) -> bool {
        matches!(self, Self::Transparent)
    }
}

/// Returns the positional form of `chars[index]`, skipping transparent marks when looking at neighbors.
/// Out-of-range indices and non-Arabic characters return `Isolated`.
pub fn arabic_positional_form(chars: &[char], index: usize) -> PositionalForm {
    let Some(&char) = chars.get(index) else {
        return PositionalForm::Isolated;
    };
    if !is_arabic(char) {
        return PositionalForm::Isolated;
    }
    let joining_type = arabic_joining_type(char);
    if matches!(
        joining_type,
        ArabicJoiningType::NonJoining | ArabicJoiningType::Transparent
    ) {
        return PositionalForm::Isolated;
    }

    let prev_joins = previous_joining_type(chars, index)
        .map(ArabicJoiningType::joins_forward)
        .unwrap_or(false);
    let next_joins = next_joining_type(chars, index)
        .map(ArabicJoiningType::joins_backward)
        .unwrap_or(false);

    match joining_type {
        ArabicJoiningType::Dual if prev_joins && next_joins => PositionalForm::Medial,
        ArabicJoiningType::Dual if prev_joins => PositionalForm::Final,
        ArabicJoiningType::Dual if next_joins => PositionalForm::Initial,
        ArabicJoiningType::Right if prev_joins => PositionalForm::Final,
        ArabicJoiningType::Left if next_joins => PositionalForm::Initial,
        _ => PositionalForm::Isolated,
    }
}

fn previous_joining_type(chars: &[char], index: usize) -> Option<ArabicJoiningType> {
    for char in chars[..index].iter().rev() {
        let joining_type = arabic_joining_type(*char);
        if !joining_type.is_transparent() {
            return Some(joining_type);
        }
    }
    None
}

fn next_joining_type(chars: &[char], index: usize) -> Option<ArabicJoiningType> {
    for char in chars.iter().skip(index + 1) {
        let joining_type = arabic_joining_type(*char);
        if !joining_type.is_transparent() {
            return Some(joining_type);
        }
    }
    None
}

/// The Arabic blocks: Arabic, Arabic Supplement, Arabic Extended-B, Arabic Extended-A, and the
/// Arabic Presentation Forms A and B.
const ARABIC_BLOCKS: [std::ops::RangeInclusive<u32>; 6] = [
    0x0600..=0x06FF,
    0x0750..=0x077F,
    0x0870..=0x089F,
    0x08A0..=0x08FF,
    0xFB50..=0xFDFF,
    0xFE70..=0xFEFF,
];

/// Returns `true` for a character of the Arabic blocks, or one whose Unicode
/// `Script_Extensions` include Arabic.
///
/// The blocks are Arabic (U+0600 to U+06FF), Arabic Supplement (U+0750 to U+077F), Arabic
/// Extended-B (U+0870 to U+089F), Arabic Extended-A (U+08A0 to U+08FF) and the Arabic
/// Presentation Forms A and B (U+FB50 to U+FDFF and U+FE70 to U+FEFF), with the Quranic marks
/// and the ornate parentheses U+FD3E and U+FD3F.
/// Some characters of the blocks, such as the number mark U+0605, belong to the Common script
/// alone, so the blocks are tested as well as the script.
/// `Script_Extensions` adds the Arabic characters outside the blocks, such as the Arabic
/// mathematical letters, and it covers the Common and Inherited characters that Arabic text
/// uses, such as the tatweel, the harakat, the Arabic comma and the Arabic-Indic digits, so a
/// shaping run does not break at them.
pub fn is_arabic(char: char) -> bool {
    let codepoint = u32::from(char);
    ARABIC_BLOCKS.iter().any(|block| block.contains(&codepoint))
        || ScriptWithExtensions::new().has_script(char, Script::Arabic)
}

/// Returns the Unicode `Joining_Type` of a character; characters without one are
/// `NonJoining`, and marks of every script are `Transparent`.
pub fn arabic_joining_type(char: char) -> ArabicJoiningType {
    match CodePointMapData::<JoiningType>::new().get(char) {
        JoiningType::DualJoining => ArabicJoiningType::Dual,
        JoiningType::RightJoining => ArabicJoiningType::Right,
        JoiningType::LeftJoining => ArabicJoiningType::Left,
        JoiningType::JoinCausing => ArabicJoiningType::JoinCausing,
        JoiningType::Transparent => ArabicJoiningType::Transparent,
        _ => ArabicJoiningType::NonJoining,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joining_types_come_from_the_unicode_data() {
        assert_eq!(arabic_joining_type('\u{0627}'), ArabicJoiningType::Right);
        assert_eq!(arabic_joining_type('\u{0628}'), ArabicJoiningType::Dual);
        assert_eq!(
            arabic_joining_type('\u{0621}'),
            ArabicJoiningType::NonJoining
        );
        assert_eq!(
            arabic_joining_type('\u{0640}'),
            ArabicJoiningType::JoinCausing
        );
        assert_eq!(
            arabic_joining_type('\u{064e}'),
            ArabicJoiningType::Transparent
        );
        // Letters that the earlier hand-made table did not have.
        for (char, joining) in [
            ('\u{066E}', ArabicJoiningType::Dual),        // dotless beh
            ('\u{06A9}', ArabicJoiningType::Dual),        // keheh
            ('\u{06CC}', ArabicJoiningType::Dual),        // Farsi yeh
            ('\u{0688}', ArabicJoiningType::Right),       // ddal
            ('\u{0671}', ArabicJoiningType::Right),       // alef wasla
            ('\u{0762}', ArabicJoiningType::Dual),        // keheh with dot above
            ('\u{0870}', ArabicJoiningType::Right),       // alef with attached fatha
            ('\u{08A0}', ArabicJoiningType::Dual),        // beh with small v below
            ('\u{06E1}', ArabicJoiningType::Transparent), // small high dotless head of khah
            ('\u{08F0}', ArabicJoiningType::Transparent), // open fathatan
            ('\u{0670}', ArabicJoiningType::Transparent), // superscript alef
            ('\u{0660}', ArabicJoiningType::NonJoining),  // Arabic-Indic digit zero
            ('\u{200D}', ArabicJoiningType::JoinCausing), // zero width joiner
            ('\u{A872}', ArabicJoiningType::Left),        // Phags-pa superfixed letter ra
        ] {
            assert_eq!(arabic_joining_type(char), joining, "{:04X}", char as u32);
        }
    }

    /// Every assigned character of the Arabic blocks is Arabic.
    #[test]
    fn is_arabic_covers_every_assigned_character_of_the_arabic_blocks() {
        use icu_properties::props::GeneralCategory;
        let category = CodePointMapData::<GeneralCategory>::new();
        let mut checked = 0;
        let mut by_script = 0;
        for block in ARABIC_BLOCKS {
            for codepoint in block {
                let Some(char) = char::from_u32(codepoint) else {
                    continue;
                };
                if matches!(
                    category.get(char),
                    GeneralCategory::Unassigned | GeneralCategory::PrivateUse
                ) {
                    continue;
                }
                // U+FDD0 to U+FDEF are noncharacters, which are unassigned.
                assert!(is_arabic(char), "U+{codepoint:04X} is Arabic");
                checked += 1;
                by_script +=
                    usize::from(ScriptWithExtensions::new().has_script(char, Script::Arabic));
            }
        }
        assert!(checked > 1000, "{checked}");
        // Nearly all of them by their script; the blocks add the few Common characters.
        assert!(checked - by_script < 16, "{checked} {by_script}");
        for char in [
            '\u{FDF2}', '\u{FD3E}', '\u{FD3F}', '\u{0640}', '\u{064B}', '\u{060C}', '\u{0660}',
            '\u{0605}',
        ] {
            assert!(is_arabic(char), "{:04X}", char as u32);
        }
        // Outside the blocks: an Arabic mathematical letter, by its script.
        assert!(is_arabic('\u{1EE01}'));
        for char in ['a', '0', ' ', '\u{05D0}', '\u{0300}'] {
            assert!(!is_arabic(char), "{:04X}", char as u32);
        }
    }

    #[test]
    fn positional_forms_skip_transparent_marks() {
        let chars = ['\u{0628}', '\u{064e}', '\u{0633}', '\u{0645}'];

        assert_eq!(arabic_positional_form(&chars, 0), PositionalForm::Initial);
        assert_eq!(arabic_positional_form(&chars, 1), PositionalForm::Isolated);
        assert_eq!(arabic_positional_form(&chars, 2), PositionalForm::Medial);
        assert_eq!(arabic_positional_form(&chars, 3), PositionalForm::Final);
    }

    #[test]
    fn right_joining_letters_only_form_final() {
        let chars = ['\u{0628}', '\u{0627}', '\u{0628}'];

        assert_eq!(arabic_positional_form(&chars, 0), PositionalForm::Initial);
        assert_eq!(arabic_positional_form(&chars, 1), PositionalForm::Final);
        assert_eq!(arabic_positional_form(&chars, 2), PositionalForm::Isolated);
    }

    #[test]
    fn letters_outside_the_old_table_join() {
        // keheh, Farsi yeh: medial and final forms in a word.
        let chars = ['\u{0628}', '\u{06A9}', '\u{06CC}'];
        assert_eq!(arabic_positional_form(&chars, 0), PositionalForm::Initial);
        assert_eq!(arabic_positional_form(&chars, 1), PositionalForm::Medial);
        assert_eq!(arabic_positional_form(&chars, 2), PositionalForm::Final);
    }
}
