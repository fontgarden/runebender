// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The direction that the kerning writer of fontc gives a kerning pair.
//!
//! UFO kerning stores pairs in logical order and has no direction of its own.
//! The kerning writer of `fontbe` 1.0.0, the back end of fontc (`src/features/kern.rs`),
//! decides the direction of each pair from the glyphs in it, and it leaves some pairs out of
//! the font.
//! This module repeats those rules, so the editor can say which pairs the exported font and
//! the live layout tables keep, and how:
//!
//! 1. Each glyph has the scripts of its code points, by `Script_Extensions`, limited to the
//!    scripts of the font.
//!    A glyph without a script of the font counts as Common.
//!    A glyph without a code point has the scripts and classes of the encoded glyphs whose
//!    GSUB closure reaches it, as `fontbe` assigns them.
//! 2. A pair whose two sides have scripts of opposite direction, such as Arabic and Latin, is
//!    left out ("mixed direction").
//! 3. Each glyph also has a bidirectional class: `R` and `AL` are right to left; `L`, `AN` and
//!    `EN` are left to right.
//!    A pair with glyphs of both classes is left out ("ambiguous direction").
//!    Arabic-Indic digits are `AN`, so a pair of reh and an Arabic-Indic digit is left out,
//!    and a pair of two Arabic-Indic digits is left to right.
//! 4. A pair that is kept is right to left when its scripts are right to left and none of its
//!    glyphs is left to right; a right-to-left pair changes both placement and advance.
//!
//! The scripts of the font are those of code points with exactly one script, plus those of the
//! script list of the layout tables, which come from the `languagesystem` statements.
//! `fontbe` reads the same statements from the feature file; an OpenType script tag that
//! does not name a Unicode script in the simple way (`arab` for `Arab`) is not read here, so a
//! glyph of such a script counts as Common.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use icu_properties::props::{BidiClass, Script};
use icu_properties::script::ScriptWithExtensions;
use icu_properties::{CodePointMapData, PropertyNamesShort, PropertyParser};
use skrifa::raw::collections::IntSet;
use skrifa::raw::types::GlyphId;
use skrifa::raw::{ReadError, TableProvider};
use skrifa::{FontRef, MetadataProvider};

use crate::text::shape::ShapingFont;

/// What the kerning writer of fontc does with one kerning pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernDirection {
    /// The pair is kept as a left-to-right pair: it changes the advance of the first glyph.
    LeftToRight,
    /// The pair is kept as a right-to-left pair: it changes the placement and the advance.
    RightToLeft,
    /// The pair is left out of the font, because its glyphs have mixed or ambiguous direction.
    Dropped,
    /// Some glyph combinations of a group pair are kept and some are left out.
    PartlyDropped,
}

/// The script direction of a Unicode script, as `fontbe` gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum ScriptDirection {
    /// The Common script: either direction.
    Auto,
    LeftToRight,
    RightToLeft,
}

impl ScriptDirection {
    /// The list of `fontbe` 1.0.0, from fontTools.
    fn of(script: &str) -> Self {
        match script {
            "Zyyy" => Self::Auto,
            "Arab" | "Hebr" | "Syrc" | "Thaa" | "Cprt" | "Khar" | "Phnx" | "Nkoo" | "Lydi"
            | "Avst" | "Armi" | "Phli" | "Prti" | "Sarb" | "Orkh" | "Samr" | "Mand" | "Merc"
            | "Mero" | "Mani" | "Mend" | "Nbat" | "Narb" | "Palm" | "Phlp" | "Hatr" | "Hung"
            | "Adlm" | "Rohg" | "Sogo" | "Sogd" | "Elym" | "Chrs" | "Yezi" | "Ougr" => {
                Self::RightToLeft
            }
            _ => Self::LeftToRight,
        }
    }

    fn plays_with(self, other: Self) -> bool {
        self == Self::Auto || other == Self::Auto || self == other
    }
}

const COMMON: &str = "Zyyy";
const INHERITED: &str = "Zinh";

/// The scripts and bidirectional classes of a glyph.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GlyphClasses {
    scripts: BTreeSet<String>,
    left_to_right: bool,
    right_to_left: bool,
}

/// The short name of a script, such as `Arab`.
fn short_name(script: Script) -> Option<String> {
    PropertyNamesShort::<Script>::new()
        .get(script)
        .map(str::to_owned)
}

/// The scripts of a code point by `Script_Extensions`, with Hiragana and Katakana as `Hrkt`,
/// as `fontbe` reads them.
fn script_extensions(codepoint: u32) -> Vec<String> {
    let mut scripts: Vec<String> = Vec::new();
    for script in ScriptWithExtensions::new()
        .get_script_extensions_val32(codepoint)
        .iter()
    {
        let Some(name) = short_name(script) else {
            continue;
        };
        let name = if name == "Hira" || name == "Kana" {
            "Hrkt".to_owned()
        } else {
            name
        };
        if !scripts.contains(&name) {
            scripts.push(name);
        }
    }
    scripts
}

/// The bidirectional class of a code point as the kerning writer reads it: `Some(true)` for
/// right to left, `Some(false)` for left to right, `None` for a neutral character.
fn bidi_direction(codepoint: u32) -> Option<bool> {
    match CodePointMapData::<BidiClass>::new().get32(codepoint) {
        BidiClass::RightToLeft | BidiClass::ArabicLetter => Some(true),
        BidiClass::LeftToRight | BidiClass::ArabicNumber | BidiClass::EuropeanNumber => Some(false),
        _ => None,
    }
}

/// The Unicode script of an OpenType script tag in the simple case, `arab` for `Arab`.
fn script_of_tag(tag: &str) -> Option<String> {
    let tag = tag.trim_end();
    if tag == "DFLT" || tag.len() != 4 {
        return None;
    }
    let mut name = tag.to_owned();
    name[..1].make_ascii_uppercase();
    PropertyParser::<Script>::new()
        .get_strict(&name)
        .and_then(short_name)
}

/// The kerning directions of the glyphs of one shaping font.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KernDirections {
    glyphs: HashMap<String, GlyphClasses>,
}

impl KernDirections {
    /// Read the code points, scripts and GSUB of a shaping font.
    ///
    /// An unreadable table gives no classes, so every glyph counts as Common and neutral.
    pub fn from_font(font: &ShapingFont) -> Self {
        Self::read(font).unwrap_or_default()
    }

    fn read(font: &ShapingFont) -> Result<Self, ReadError> {
        let bytes = font.bytes();
        let font_ref = FontRef::new(bytes)?;
        let names = font.glyph_order();
        let mappings: Vec<(u32, GlyphId)> = font_ref.charmap().mappings().collect();

        // The scripts of the font.
        let mut known: HashSet<String> = HashSet::new();
        for (codepoint, _) in &mappings {
            if let [only] = script_extensions(*codepoint).as_slice() {
                known.insert(only.clone());
            }
        }
        for list in [
            font_ref
                .gsub()
                .ok()
                .and_then(|gsub| gsub.script_list().ok()),
            font_ref
                .gpos()
                .ok()
                .and_then(|gpos| gpos.script_list().ok()),
        ]
        .into_iter()
        .flatten()
        {
            for record in list.script_records() {
                if let Some(script) = script_of_tag(&record.script_tag().to_string()) {
                    known.insert(script);
                }
            }
        }

        // Classify the encoded glyphs, then add the glyphs that the GSUB closure reaches.
        let mut sets: BTreeMap<Property, IntSet<GlyphId>> = BTreeMap::new();
        let mut neutral: IntSet<GlyphId> = IntSet::new();
        for (codepoint, glyph) in &mappings {
            let mut properties = Vec::new();
            if known.is_empty() {
                properties.push(Property::Script(COMMON.to_owned()));
            } else {
                properties.extend(
                    script_extensions(*codepoint)
                        .into_iter()
                        .filter(|script| {
                            script == COMMON || script == INHERITED || known.contains(script)
                        })
                        .map(Property::Script),
                );
            }
            if let Some(right_to_left) = bidi_direction(*codepoint) {
                properties.push(Property::Bidi(right_to_left));
            }
            let has_script = properties
                .iter()
                .any(|property| matches!(property, Property::Script(_)));
            let has_bidi = properties
                .iter()
                .any(|property| matches!(property, Property::Bidi(_)));
            for property in properties {
                sets.entry(property).or_default().insert(*glyph);
            }
            // `fontbe` classifies scripts and bidi classes separately, each with its own
            // neutral set; the two neutral sets are kept apart below.
            if !has_script {
                sets.entry(Property::NoScript).or_default().insert(*glyph);
            }
            if !has_bidi {
                neutral.insert(*glyph);
            }
        }
        if let Ok(gsub) = font_ref.gsub() {
            let lookups = gsub.collect_lookups(&IntSet::all())?;
            let no_script = sets.remove(&Property::NoScript).unwrap_or_default();
            let mut script_neutral = no_script;
            gsub.closure_glyphs(&lookups, &mut script_neutral)?;
            let mut bidi_neutral = neutral;
            gsub.closure_glyphs(&lookups, &mut bidi_neutral)?;
            for (property, glyphs) in &mut sets {
                let neutral = match property {
                    Property::Script(_) | Property::NoScript => &script_neutral,
                    Property::Bidi(_) => &bidi_neutral,
                };
                let mut reached = glyphs.clone();
                reached.union(neutral);
                gsub.closure_glyphs(&lookups, &mut reached)?;
                for glyph in reached.iter() {
                    if !neutral.contains(glyph) {
                        glyphs.insert(glyph);
                    }
                }
            }
        }

        let mut glyphs: HashMap<String, GlyphClasses> = HashMap::new();
        for (property, members) in sets {
            for glyph in members.iter() {
                let Some(name) = usize::try_from(glyph.to_u32())
                    .ok()
                    .and_then(|index| names.get(index))
                else {
                    continue;
                };
                let classes = glyphs.entry(name.clone()).or_default();
                match &property {
                    Property::Script(script) => {
                        classes.scripts.insert(script.clone());
                    }
                    Property::Bidi(true) => classes.right_to_left = true,
                    Property::Bidi(false) => classes.left_to_right = true,
                    Property::NoScript => {}
                }
            }
        }
        Ok(Self { glyphs })
    }

    /// The scripts of a glyph as the kerning writer resolves them: Common when it has none or
    /// has Common or Inherited among them.
    fn resolved_scripts(&self, glyph: &str) -> BTreeSet<String> {
        let common = || BTreeSet::from([COMMON.to_owned()]);
        match self.glyphs.get(glyph) {
            Some(classes)
                if !classes.scripts.is_empty()
                    && !classes.scripts.contains(COMMON)
                    && !classes.scripts.contains(INHERITED) =>
            {
                classes.scripts.clone()
            }
            _ => common(),
        }
    }

    /// What the kerning writer does with a pair of glyph sets: the members of the first and
    /// second side, a single glyph or the members of a group, in logical order.
    pub fn pair<'a>(
        &self,
        first: impl IntoIterator<Item = &'a str>,
        second: impl IntoIterator<Item = &'a str>,
    ) -> KernDirection {
        let side = |glyphs: Vec<&'a str>| {
            let mut by_direction: BTreeMap<ScriptDirection, Vec<&'a str>> = BTreeMap::new();
            for glyph in glyphs {
                for script in self.resolved_scripts(glyph) {
                    let list = by_direction
                        .entry(ScriptDirection::of(&script))
                        .or_default();
                    if !list.contains(&glyph) {
                        list.push(glyph);
                    }
                }
            }
            by_direction
        };
        let first = side(first.into_iter().collect());
        let second = side(second.into_iter().collect());
        let (mut kept, mut dropped) = (BTreeSet::new(), false);
        for (first_direction, first_glyphs) in &first {
            for (second_direction, second_glyphs) in &second {
                if !first_direction.plays_with(*second_direction) {
                    dropped = true;
                    continue;
                }
                let glyphs = first_glyphs.iter().chain(second_glyphs);
                let (mut left_to_right, mut right_to_left) = (false, false);
                for glyph in glyphs.clone() {
                    if let Some(classes) = self.glyphs.get(*glyph) {
                        left_to_right |= classes.left_to_right;
                        right_to_left |= classes.right_to_left;
                    }
                }
                if left_to_right && right_to_left {
                    dropped = true;
                    continue;
                }
                let mut scripts: BTreeSet<String> = BTreeSet::new();
                for glyph in glyphs {
                    scripts.extend(self.resolved_scripts(glyph));
                }
                let all_common = [first_glyphs, second_glyphs].iter().all(|side| {
                    side.iter()
                        .any(|glyph| self.resolved_scripts(glyph).contains(COMMON))
                });
                if !all_common {
                    scripts.remove(COMMON);
                }
                let script_is_rtl = scripts.first().is_some_and(|script| {
                    ScriptDirection::of(script) == ScriptDirection::RightToLeft
                });
                kept.insert(script_is_rtl && !left_to_right);
            }
        }
        match (kept.len(), dropped) {
            (0, _) => KernDirection::Dropped,
            (_, true) => KernDirection::PartlyDropped,
            _ if kept.contains(&true) && kept.len() == 1 => KernDirection::RightToLeft,
            _ if kept.len() == 1 => KernDirection::LeftToRight,
            // A group pair with both directions; the font has both kinds of record.
            _ => KernDirection::PartlyDropped,
        }
    }
}

/// A property that the classification sorts glyphs by.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Property {
    Script(String),
    /// An encoded glyph without a script of the font.
    NoScript,
    /// A bidirectional class: true for right to left.
    Bidi(bool),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::project::Project;
    use crate::text::layout_tables::{LayoutInputs, build_shaping_font};

    fn fixture() -> (LayoutInputs, KernDirections) {
        let project = Project::load(&crate::testing::fonts::arabic_regular_ufo()).unwrap();
        let source = project.document_sources().next().unwrap().id();
        let inputs = LayoutInputs::from_project(&project, source).unwrap();
        let directions = KernDirections::from_font(&build_shaping_font(&inputs).unwrap());
        (inputs, directions)
    }

    #[test]
    fn directions_follow_the_kerning_writer_of_fontc() {
        let (inputs, directions) = fixture();
        let members = |group: &str| -> Vec<&str> {
            inputs.groups[group].iter().map(String::as_str).collect()
        };
        let pair = |a: &[&str], b: &[&str]| directions.pair(a.iter().copied(), b.iter().copied());
        assert_eq!(
            pair(&members("public.kern1.reh"), &members("public.kern2.alef")),
            KernDirection::RightToLeft
        );
        // `reh-ar.fina` has no code point; the GSUB closure of reh gives it its classes.
        assert_eq!(
            pair(&["reh-ar.fina"], &["alef-ar"]),
            KernDirection::RightToLeft
        );
        assert_eq!(pair(&["reh-ar"], &["meem-ar"]), KernDirection::RightToLeft);
        // Arabic-Indic digits are left to right.
        assert_eq!(pair(&["one-ar"], &["two-ar"]), KernDirection::LeftToRight);
        // A pair of reh and a digit has an ambiguous direction, and one of reh and a Latin
        // letter a mixed one: fontc leaves both out.
        assert_eq!(pair(&["reh-ar"], &["one-ar"]), KernDirection::Dropped);
        assert_eq!(pair(&["reh-ar"], &["A"]), KernDirection::Dropped);
        assert_eq!(pair(&["A"], &["V"]), KernDirection::LeftToRight);
        // A group with a Latin member is split by script, and only the Arabic part is kept.
        assert_eq!(
            pair(&["reh-ar"], &["alef-ar", "A"]),
            KernDirection::PartlyDropped
        );
        // A digit has the Arabic script, so a group with alef and a digit is one part, and
        // the part has both bidirectional classes: the whole pair is left out.
        assert_eq!(
            pair(&["reh-ar"], &["alef-ar", "one-ar"]),
            KernDirection::Dropped
        );
    }

    /// The kept pairs are the ones the compiled kerning applies: a dropped pair changes
    /// nothing in the shaped text.
    #[test]
    fn the_rules_agree_with_the_compiled_kerning_of_the_fixture() {
        let (inputs, directions) = fixture();
        assert_eq!(
            directions.pair(["reh-ar"], ["one-ar"]),
            KernDirection::Dropped
        );
        let font = build_shaping_font(&inputs).unwrap();
        let shaped = font
            .shape_with_options("ر١", true, &[], Some("arab"), None)
            .unwrap();
        assert_eq!(
            shaped
                .iter()
                .map(|glyph| glyph.x_advance)
                .collect::<Vec<_>>(),
            [500.0, 360.0]
        );

        // With a digit in the alef group, the writer leaves out the whole group pair, and reh
        // and alef are no longer kerned; with a Latin letter in it, the Arabic part stays.
        let reh_advance = |extra: &str| {
            let mut inputs = inputs.clone();
            inputs
                .groups
                .get_mut("public.kern2.alef")
                .unwrap()
                .push(extra.to_owned());
            let directions = KernDirections::from_font(&build_shaping_font(&inputs).unwrap());
            let members: Vec<&str> = inputs.groups["public.kern2.alef"]
                .iter()
                .map(String::as_str)
                .collect();
            let shaped = build_shaping_font(&inputs)
                .unwrap()
                .shape_with_options("را", true, &[], Some("arab"), None)
                .unwrap();
            (
                directions.pair(["reh-ar", "reh-ar.fina"], members),
                shaped[1].x_advance,
            )
        };
        assert_eq!(reh_advance("one-ar"), (KernDirection::Dropped, 360.0));
        assert_eq!(reh_advance("A"), (KernDirection::PartlyDropped, 300.0));
    }
}
