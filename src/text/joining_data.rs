// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Joining type and joining group of Arabic letters, for the context strings and the kashida
//! positions.
//!
//! The values follow `ArabicShaping.txt` of the Unicode Character Database for the Arabic and
//! Arabic Supplement blocks, the letters of Arabic Extended-A, and the Arabic marks.
//! Joining groups are given where the kashida rules name them; Arabic Extended-A letters have
//! a joining type and no group.
//! This table is private to the text modules: [`super::joining`] keeps the joining rules of
//! the shaping path, and a later change can supply both from one Unicode property source.

use super::joining::ArabicJoiningType;

/// The joining groups of `ArabicShaping.txt` that the kashida rules name, and the others of
/// the Arabic letters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JoiningGroup {
    Ain,
    Alef,
    Beh,
    BurushaskiYehBarree,
    Dal,
    FarsiYeh,
    Feh,
    Gaf,
    Hah,
    Heh,
    HehGoal,
    Kaf,
    KnottedHeh,
    Lam,
    Meem,
    Noon,
    Nya,
    Qaf,
    Reh,
    Sad,
    Seen,
    SwashKaf,
    Tah,
    TehMarbuta,
    TehMarbutaGoal,
    Waw,
    Yeh,
    YehBarree,
    YehWithTail,
}

use ArabicJoiningType::{Dual as D, JoinCausing as C, NonJoining as U, Right as R};
use JoiningGroup as G;

/// Ranges of letters: first, last, joining type, joining group.
const LETTERS: &[(u32, u32, ArabicJoiningType, Option<JoiningGroup>)] = &[
    (0x0620, 0x0620, D, Some(G::Yeh)),
    (0x0621, 0x0621, U, None),
    (0x0622, 0x0623, R, Some(G::Alef)),
    (0x0624, 0x0624, R, Some(G::Waw)),
    (0x0625, 0x0625, R, Some(G::Alef)),
    (0x0626, 0x0626, D, Some(G::Yeh)),
    (0x0627, 0x0627, R, Some(G::Alef)),
    (0x0628, 0x0628, D, Some(G::Beh)),
    (0x0629, 0x0629, R, Some(G::TehMarbuta)),
    (0x062A, 0x062B, D, Some(G::Beh)),
    (0x062C, 0x062E, D, Some(G::Hah)),
    (0x062F, 0x0630, R, Some(G::Dal)),
    (0x0631, 0x0632, R, Some(G::Reh)),
    (0x0633, 0x0634, D, Some(G::Seen)),
    (0x0635, 0x0636, D, Some(G::Sad)),
    (0x0637, 0x0638, D, Some(G::Tah)),
    (0x0639, 0x063A, D, Some(G::Ain)),
    (0x063B, 0x063C, D, Some(G::Gaf)),
    (0x063D, 0x063F, D, Some(G::FarsiYeh)),
    (0x0640, 0x0640, C, None),
    (0x0641, 0x0641, D, Some(G::Feh)),
    (0x0642, 0x0642, D, Some(G::Qaf)),
    (0x0643, 0x0643, D, Some(G::Kaf)),
    (0x0644, 0x0644, D, Some(G::Lam)),
    (0x0645, 0x0645, D, Some(G::Meem)),
    (0x0646, 0x0646, D, Some(G::Noon)),
    (0x0647, 0x0647, D, Some(G::Heh)),
    (0x0648, 0x0648, R, Some(G::Waw)),
    (0x0649, 0x064A, D, Some(G::Yeh)),
    (0x066E, 0x066E, D, Some(G::Beh)),
    (0x066F, 0x066F, D, Some(G::Qaf)),
    (0x0671, 0x0673, R, Some(G::Alef)),
    (0x0674, 0x0674, U, None),
    (0x0675, 0x0675, R, Some(G::Alef)),
    (0x0676, 0x0677, R, Some(G::Waw)),
    (0x0678, 0x0678, D, Some(G::Yeh)),
    (0x0679, 0x0680, D, Some(G::Beh)),
    (0x0681, 0x0687, D, Some(G::Hah)),
    (0x0688, 0x0690, R, Some(G::Dal)),
    (0x0691, 0x0699, R, Some(G::Reh)),
    (0x069A, 0x069C, D, Some(G::Seen)),
    (0x069D, 0x069E, D, Some(G::Sad)),
    (0x069F, 0x069F, D, Some(G::Tah)),
    (0x06A0, 0x06A0, D, Some(G::Ain)),
    (0x06A1, 0x06A6, D, Some(G::Feh)),
    (0x06A7, 0x06A8, D, Some(G::Qaf)),
    (0x06A9, 0x06A9, D, Some(G::Gaf)),
    (0x06AA, 0x06AA, D, Some(G::SwashKaf)),
    (0x06AB, 0x06AB, D, Some(G::Gaf)),
    (0x06AC, 0x06AE, D, Some(G::Kaf)),
    (0x06AF, 0x06B4, D, Some(G::Gaf)),
    (0x06B5, 0x06B8, D, Some(G::Lam)),
    (0x06B9, 0x06BC, D, Some(G::Noon)),
    (0x06BD, 0x06BD, D, Some(G::Nya)),
    (0x06BE, 0x06BE, D, Some(G::KnottedHeh)),
    (0x06BF, 0x06BF, D, Some(G::Hah)),
    (0x06C0, 0x06C0, R, Some(G::TehMarbuta)),
    (0x06C1, 0x06C2, D, Some(G::HehGoal)),
    (0x06C3, 0x06C3, R, Some(G::TehMarbutaGoal)),
    (0x06C4, 0x06CB, R, Some(G::Waw)),
    (0x06CC, 0x06CC, D, Some(G::FarsiYeh)),
    (0x06CD, 0x06CD, R, Some(G::YehWithTail)),
    (0x06CE, 0x06CE, D, Some(G::FarsiYeh)),
    (0x06CF, 0x06CF, R, Some(G::Waw)),
    (0x06D0, 0x06D1, D, Some(G::Yeh)),
    (0x06D2, 0x06D3, R, Some(G::YehBarree)),
    (0x06D5, 0x06D5, R, Some(G::TehMarbuta)),
    (0x06EE, 0x06EE, R, Some(G::Dal)),
    (0x06EF, 0x06EF, R, Some(G::Reh)),
    (0x06FA, 0x06FA, D, Some(G::Seen)),
    (0x06FB, 0x06FB, D, Some(G::Sad)),
    (0x06FC, 0x06FC, D, Some(G::Ain)),
    (0x06FF, 0x06FF, D, Some(G::KnottedHeh)),
    (0x0750, 0x0756, D, Some(G::Beh)),
    (0x0757, 0x0758, D, Some(G::Hah)),
    (0x0759, 0x075A, R, Some(G::Dal)),
    (0x075B, 0x075B, R, Some(G::Reh)),
    (0x075C, 0x075C, D, Some(G::Seen)),
    (0x075D, 0x075F, D, Some(G::Ain)),
    (0x0760, 0x0761, D, Some(G::Feh)),
    (0x0762, 0x0764, D, Some(G::Gaf)),
    (0x0765, 0x0766, D, Some(G::Meem)),
    (0x0767, 0x0769, D, Some(G::Noon)),
    (0x076A, 0x076A, D, Some(G::Lam)),
    (0x076B, 0x076C, R, Some(G::Reh)),
    (0x076D, 0x076D, D, Some(G::Seen)),
    (0x076E, 0x076F, D, Some(G::Hah)),
    (0x0770, 0x0770, D, Some(G::Seen)),
    (0x0771, 0x0771, R, Some(G::Reh)),
    (0x0772, 0x0772, D, Some(G::Hah)),
    (0x0773, 0x0774, R, Some(G::Alef)),
    (0x0775, 0x0776, D, Some(G::FarsiYeh)),
    (0x0777, 0x0777, D, Some(G::Yeh)),
    (0x0778, 0x0779, R, Some(G::Waw)),
    (0x077A, 0x077B, D, Some(G::BurushaskiYehBarree)),
    (0x077C, 0x077C, D, Some(G::Hah)),
    (0x077D, 0x077E, D, Some(G::Seen)),
    (0x077F, 0x077F, D, Some(G::Kaf)),
    (0x08A0, 0x08A9, D, None),
    (0x08AA, 0x08AC, R, None),
    (0x08AD, 0x08AD, U, None),
    (0x08AE, 0x08AE, R, None),
    (0x08AF, 0x08B0, D, None),
    (0x08B1, 0x08B2, R, None),
    (0x08B3, 0x08B8, D, None),
    (0x08B9, 0x08B9, R, None),
    (0x08BA, 0x08C8, D, None),
];

/// Ranges of the Arabic nonspacing marks, which are transparent to joining.
const MARKS: &[(u32, u32)] = &[
    (0x0610, 0x061A),
    (0x064B, 0x065F),
    (0x0670, 0x0670),
    (0x06D6, 0x06DC),
    (0x06DF, 0x06E4),
    (0x06E7, 0x06E8),
    (0x06EA, 0x06ED),
    (0x0898, 0x089F),
    (0x08CA, 0x08E1),
    (0x08E3, 0x08FF),
];

fn letter(c: char) -> Option<(ArabicJoiningType, Option<JoiningGroup>)> {
    let cp = u32::from(c);
    LETTERS
        .iter()
        .find(|(first, last, _, _)| (*first..=*last).contains(&cp))
        .map(|(_, _, joining, group)| (*joining, *group))
}

/// Whether `c` is an Arabic nonspacing mark.
pub(crate) fn is_mark(c: char) -> bool {
    let cp = u32::from(c);
    MARKS
        .iter()
        .any(|(first, last)| (*first..=*last).contains(&cp))
}

/// Whether `c` is an Arabic letter, the tatweel included.
pub(crate) fn is_letter(c: char) -> bool {
    letter(c).is_some()
}

/// The joining type of `c`: transparent for a mark and for the format characters that
/// `ArabicShaping.txt` makes transparent, join causing for the zero width joiner, and non
/// joining for everything else outside the table.
pub(crate) fn joining_type(c: char) -> ArabicJoiningType {
    if is_mark(c) || matches!(u32::from(c), 0x061C | 0x200B | 0x200E | 0x200F) {
        return ArabicJoiningType::Transparent;
    }
    if c == '\u{200D}' {
        return ArabicJoiningType::JoinCausing;
    }
    letter(c).map_or(U, |(joining, _)| joining)
}

/// The joining group of a letter, when the table gives one.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the kashida rules of plan item 6.9 will read the joining groups"
    )
)]
pub(crate) fn joining_group(c: char) -> Option<JoiningGroup> {
    letter(c).and_then(|(_, group)| group)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_no_overlap_and_agrees_with_the_shaping_subset() {
        for pair in LETTERS.windows(2) {
            assert!(pair[0].1 < pair[1].0, "{:04X}", pair[1].0);
        }
        for cp in 0x0600..=0x06FF {
            let c = char::from_u32(cp).unwrap();
            let shaping = super::super::joining::arabic_joining_type(c);
            // The shaping subset also makes the signs U+06DD, U+06DE and U+06E9 and the small
            // waw and yeh transparent; only the letters are compared.
            if matches!(shaping, D | R | C) {
                assert_eq!(joining_type(c), shaping, "U+{cp:04X}");
            }
        }
    }

    #[test]
    fn persian_urdu_and_kurdish_letters_have_their_joining_types() {
        for (c, joining) in [
            ('\u{067E}', D),
            ('\u{06A9}', D),
            ('\u{06CC}', D),
            ('\u{06C1}', D),
            ('\u{06BE}', D),
            ('\u{0688}', R),
            ('\u{0691}', R),
            ('\u{06D2}', R),
            ('\u{06D5}', R),
            ('\u{06C6}', R),
            ('\u{0695}', R),
            ('\u{06B5}', D),
        ] {
            assert_eq!(joining_type(c), joining, "U+{:04X}", u32::from(c));
        }
        assert_eq!(joining_group('\u{06A9}'), Some(G::Gaf));
        assert_eq!(joining_type('\u{06DD}'), U);
        assert_eq!(joining_type('\u{08F0}'), ArabicJoiningType::Transparent);
        assert_eq!(joining_type('\u{200C}'), U);
        assert!(!is_letter('\u{0660}'));
    }
}
