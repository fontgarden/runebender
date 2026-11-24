// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Test strings that show one Arabic letter in each joining context and with each mark class
//! of the font.
//!
//! [`context_text`] writes, in logical order, the letter isolated, initial, medial and final
//! beside a dual-joining partner, then the same forms with one mark of each anchor class and
//! with a shadda stack.
//! A right-joining letter has only its isolated and final forms, and a non-joining letter only
//! its isolated form.
//! [`pair_matrix`] writes the letter joined before and after each letter of a set.
//! The mark classes come from the caller as [`ContextMark`] values, read from the mark anchors
//! of the font, so this module does not read layout tables.
//! A character that is not an Arabic letter or mark gives an empty string.

use super::joining::{ArabicJoiningType, PositionalForm};
use super::joining_data;

/// The partner that [`context_text`] uses when the caller's partner does not join on both
/// sides: beh, U+0628.
pub const DEFAULT_PARTNER: char = '\u{0628}';

/// Shadda, U+0651, the base of the stacked-mark contexts.
const SHADDA: char = '\u{0651}';

/// The basic letters of the Arabic alphabet, one of each joining group, for [`pair_matrix`].
pub const PAIR_LETTERS: &[char] = &[
    '\u{0627}', '\u{0628}', '\u{062C}', '\u{062F}', '\u{0631}', '\u{0633}', '\u{0635}', '\u{0637}',
    '\u{0639}', '\u{0641}', '\u{0642}', '\u{0643}', '\u{0644}', '\u{0645}', '\u{0646}', '\u{0647}',
    '\u{0648}', '\u{064A}',
];

/// The marks that represent an anchor class, in order of preference.
const PREFERRED: &[(&str, &[char])] = &[
    (
        "top",
        &[
            '\u{064E}', '\u{064F}', '\u{0652}', '\u{064B}', '\u{064C}', '\u{0670}',
        ],
    ),
    ("bottom", &['\u{0650}', '\u{064D}', '\u{0656}']),
];

/// A mark glyph of the font: its character and the anchor class it attaches with, which is the
/// name of its mark anchor without the leading underscore, such as `top` for `_top`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextMark {
    /// The character of the mark glyph.
    pub character: char,
    /// The anchor class, such as `top` or `bottom`.
    pub anchor: String,
}

impl ContextMark {
    /// A mark with this character and anchor class.
    pub fn new(character: char, anchor: impl Into<String>) -> Self {
        Self {
            character,
            anchor: anchor.into(),
        }
    }
}

/// Whether `c` is an Arabic nonspacing mark, which can be a [`ContextMark`].
pub fn is_arabic_mark(c: char) -> bool {
    joining_data::is_mark(c)
}

/// The joining forms that `letter` has, in the order isolated, initial, medial, final.
/// Empty for a character that is not an Arabic letter.
pub fn joining_forms(letter: char) -> &'static [PositionalForm] {
    use PositionalForm::{Final, Initial, Isolated, Medial};
    if !joining_data::is_letter(letter) {
        return &[];
    }
    match joining_data::joining_type(letter) {
        ArabicJoiningType::Dual | ArabicJoiningType::JoinCausing => {
            &[Isolated, Initial, Medial, Final]
        }
        ArabicJoiningType::Right => &[Isolated, Final],
        ArabicJoiningType::Left => &[Isolated, Initial],
        ArabicJoiningType::NonJoining | ArabicJoiningType::Transparent => &[Isolated],
    }
}

/// One representative mark for each anchor class of `marks`: `top` first, then `bottom`, then
/// the other classes by name.
/// A class takes its preferred harakah when the font has it, else its lowest code point;
/// shadda represents a class only when it is the class's only mark.
pub fn mark_classes(marks: &[ContextMark]) -> Vec<(String, char)> {
    let mut classes: Vec<&str> = marks.iter().map(|mark| mark.anchor.as_str()).collect();
    classes.sort_by_key(|class| {
        let rank = PREFERRED
            .iter()
            .position(|(name, _)| name == class)
            .unwrap_or(PREFERRED.len());
        (rank, *class)
    });
    classes.dedup();
    classes
        .into_iter()
        .filter_map(|class| {
            let members: Vec<char> = marks
                .iter()
                .filter(|mark| mark.anchor == class && joining_data::is_mark(mark.character))
                .map(|mark| mark.character)
                .collect();
            let preferred = PREFERRED
                .iter()
                .find(|(name, _)| *name == class)
                .and_then(|(_, order)| order.iter().find(|c| members.contains(c)));
            let fallback = members.iter().filter(|c| **c != SHADDA).min();
            preferred
                .or(fallback)
                .or_else(|| members.iter().min())
                .map(|mark| (class.to_owned(), *mark))
        })
        .collect()
}

/// The words that show `letter`, with `marks` after it, in each of its joining forms.
fn forms(letter: char, marks: &str, partner: char) -> Vec<String> {
    joining_forms(letter)
        .iter()
        .map(|form| match form {
            PositionalForm::Isolated => format!("{letter}{marks}"),
            PositionalForm::Initial => format!("{letter}{marks}{partner}"),
            PositionalForm::Medial => format!("{partner}{letter}{marks}{partner}"),
            PositionalForm::Final => format!("{partner}{letter}{marks}"),
        })
        .collect()
}

/// A line that shows `letter` in each of its joining forms beside `partner`, then with one mark
/// of each class in `marks`, then with shadda under the top and the bottom marks when the font
/// has shadda.
///
/// Forms are separated by a space and groups by two spaces.
/// The partner must join on both sides; another partner is replaced by [`DEFAULT_PARTNER`].
/// For an Arabic mark, the line shows the mark on the partner in each joining form.
/// For any other character the line is empty.
pub fn context_text(letter: char, partner: char, marks: &[ContextMark]) -> String {
    let partner = if joining_data::joining_type(partner) == ArabicJoiningType::Dual {
        partner
    } else {
        DEFAULT_PARTNER
    };
    if joining_data::is_mark(letter) {
        return forms(partner, letter.encode_utf8(&mut [0; 4]), partner).join(" ");
    }
    if !joining_data::is_letter(letter) {
        return String::new();
    }
    let classes = mark_classes(marks);
    let mut groups = vec![forms(letter, "", partner)];
    for (_, mark) in &classes {
        groups.push(forms(letter, mark.encode_utf8(&mut [0; 4]), partner));
    }
    if marks.iter().any(|mark| mark.character == SHADDA) {
        for (_, mark) in classes
            .iter()
            .filter(|(class, mark)| (class == "top" || class == "bottom") && *mark != SHADDA)
        {
            groups.push(forms(letter, &format!("{SHADDA}{mark}"), partner));
        }
    }
    groups
        .into_iter()
        .map(|group| group.join(" "))
        .collect::<Vec<_>>()
        .join("  ")
}

/// The words that join `letter` after and before each Arabic letter of `others`, as
/// `letter other` and `other letter` pairs; empty when `letter` is not an Arabic letter.
///
/// Pairs are separated by a space and the pairs of one other letter by two spaces.
pub fn pair_matrix(letter: char, others: &[char]) -> String {
    if !joining_data::is_letter(letter) {
        return String::new();
    }
    others
        .iter()
        .filter(|other| joining_data::is_letter(**other))
        .map(|other| format!("{letter}{other} {other}{letter}"))
        .collect::<Vec<_>>()
        .join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::joining::arabic_positional_form;

    const BEH: char = '\u{0628}';
    const NOON: char = '\u{0646}';
    const REH: char = '\u{0631}';
    const FATHA: char = '\u{064E}';
    const KASRA: char = '\u{0650}';

    fn font_marks() -> Vec<ContextMark> {
        vec![
            ContextMark::new(KASRA, "bottom"),
            ContextMark::new(SHADDA, "top"),
            ContextMark::new('\u{064F}', "top"),
            ContextMark::new(FATHA, "top"),
        ]
    }

    /// The forms of each occurrence of `letter` in `text`, by the joining rules of shaping.
    fn forms_of(text: &str, letter: char) -> Vec<PositionalForm> {
        let chars: Vec<char> = text.chars().collect();
        (0..chars.len())
            .filter(|i| chars[*i] == letter)
            .map(|i| arabic_positional_form(&chars, i))
            .collect()
    }

    #[test]
    fn beh_appears_in_all_four_forms_in_order() {
        let text = context_text(BEH, NOON, &[]);
        assert_eq!(text, "ب بن نبن نب");
        use PositionalForm::*;
        assert_eq!(forms_of(&text, BEH), [Isolated, Initial, Medial, Final]);
    }

    #[test]
    fn reh_appears_only_isolated_and_final() {
        let text = context_text(REH, NOON, &[]);
        assert_eq!(text, "ر نر");
        use PositionalForm::*;
        assert_eq!(forms_of(&text, REH), [Isolated, Final]);
        assert_eq!(context_text('\u{0621}', NOON, &[]), "ء");
    }

    #[test]
    fn marks_sit_on_the_letter_in_every_form_with_a_shadda_stack() {
        let text = context_text(BEH, NOON, &font_marks());
        let groups: Vec<&str> = text.split("  ").collect();
        // The same four forms, with each mark sequence after the beh.
        let group = |marks: &str| format!("ب{marks} ب{marks}ن نب{marks}ن نب{marks}");
        assert_eq!(
            groups,
            [
                group(""),
                group("\u{064E}"),
                group("\u{0650}"),
                group("\u{0651}\u{064E}"),
                group("\u{0651}\u{0650}"),
            ]
        );
        // Each mark follows its base letter, never a partner or a space.
        let chars: Vec<char> = text.chars().collect();
        for (i, c) in chars.iter().enumerate() {
            if joining_data::is_mark(*c) {
                let base = chars[..i]
                    .iter()
                    .rev()
                    .find(|c| !joining_data::is_mark(**c))
                    .unwrap();
                assert_eq!(*base, BEH);
            }
        }
    }

    #[test]
    fn mark_classes_prefer_the_harakat_and_order_top_and_bottom_first() {
        let mut marks = font_marks();
        marks.push(ContextMark::new('\u{0655}', "hamza_below"));
        marks.push(ContextMark::new('\u{0654}', "hamza"));
        assert_eq!(
            mark_classes(&marks),
            [
                ("top".to_owned(), FATHA),
                ("bottom".to_owned(), KASRA),
                ("hamza".to_owned(), '\u{0654}'),
                ("hamza_below".to_owned(), '\u{0655}'),
            ]
        );
        assert_eq!(
            mark_classes(&[ContextMark::new(SHADDA, "top")]),
            [("top".to_owned(), SHADDA)]
        );
        // A letter listed as a mark is not a mark.
        assert!(mark_classes(&[ContextMark::new(BEH, "top")]).is_empty());
    }

    #[test]
    fn a_non_arabic_character_gives_an_empty_string() {
        assert_eq!(context_text('A', BEH, &font_marks()), "");
        assert_eq!(context_text('\u{05D1}', BEH, &font_marks()), "");
        assert_eq!(context_text('\u{0661}', BEH, &font_marks()), "");
        assert_eq!(pair_matrix('A', PAIR_LETTERS), "");
    }

    #[test]
    fn a_mark_is_shown_on_the_partner_and_a_bad_partner_is_replaced() {
        assert_eq!(context_text(FATHA, NOON, &[]), "نَ نَن ننَن ننَ");
        assert_eq!(context_text(BEH, REH, &[]), "ب بب ببب بب");
    }

    #[test]
    fn the_pair_matrix_joins_the_letter_on_both_sides_of_each_letter() {
        let text = pair_matrix(BEH, &[REH, 'x', NOON]);
        assert_eq!(text, "بر رب  بن نب");
        assert_eq!(
            pair_matrix(REH, PAIR_LETTERS).split("  ").count(),
            PAIR_LETTERS.len()
        );
    }

    #[test]
    fn right_joining_persian_and_urdu_letters_have_two_forms() {
        for letter in ['\u{0688}', '\u{06D2}', '\u{06C6}'] {
            assert_eq!(joining_forms(letter).len(), 2);
        }
        for letter in ['\u{067E}', '\u{06A9}', '\u{06CC}', '\u{06C1}'] {
            assert_eq!(joining_forms(letter).len(), 4);
        }
    }
}
