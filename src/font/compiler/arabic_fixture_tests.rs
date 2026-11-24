// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The complete fontc compile of the synthetic Arabic fixture.
//!
//! `tests/fixtures/arabic/README.md` lists the test strings and the two unfinished-master
//! glyphs. These tests record what the export compile does with them, so the live layout
//! engine has a reference to match.

use crate::font::project::Project;
use crate::testing::fonts;
use crate::text::shape::ShapingFont;

/// The glyph whose contours do not interpolate: Bold has one more point.
const INCOMPATIBLE_GLYPH: &str = "noon-ar";
/// The glyph with a `top` anchor in Bold and none in Regular.
const PARTIAL_ANCHOR_GLYPH: &str = "dal-ar";

/// One shaped glyph: name, advance, x offset and y offset.
type Shaped = (String, f64, f64, f64);

fn load() -> Project {
    Project::load(&fonts::arabic_designspace()).expect("the Arabic fixture loads")
}

/// The fixture without its two unfinished-master glyphs, compiled at the default location.
fn finished_font() -> ShapingFont {
    let mut project = load();
    for glyph in [INCOMPATIBLE_GLYPH, PARTIAL_ANCHOR_GLYPH] {
        project
            .remove_document_glyph(glyph)
            .expect("the unfinished glyph is removable");
    }
    let compiled = project
        .compile()
        .expect("the finished glyphs compile with fontc");
    ShapingFont::from_bytes(compiled.bytes.to_vec()).expect("the compiled font is readable")
}

/// Shape Arabic text right to left and ASCII text left to right, with kerning on or off.
fn shape(font: &ShapingFont, text: &str, kern: bool) -> Vec<Shaped> {
    let right_to_left = !text.is_ascii();
    let script = if right_to_left { "arab" } else { "latn" };
    font.shape_with_options(
        text,
        right_to_left,
        &[("kern".into(), kern)],
        Some(script),
        None,
    )
    .expect("the text shapes")
    .into_iter()
    .map(|glyph| {
        (
            font.glyph_name(glyph.glyph_id)
                .expect("every shaped glyph has a name")
                .to_owned(),
            glyph.x_advance,
            glyph.x_offset,
            glyph.y_offset,
        )
    })
    .collect()
}

/// Glyph names in the output order of the shaper, which is visual order.
fn names(shaped: &[Shaped]) -> Vec<&str> {
    shaped.iter().map(|(name, ..)| name.as_str()).collect()
}

fn glyph<'a>(shaped: &'a [Shaped], name: &str) -> &'a Shaped {
    shaped
        .iter()
        .find(|(found, ..)| found == name)
        .unwrap_or_else(|| panic!("{name} is not in {shaped:?}"))
}

#[test]
fn unfinished_masters_stop_the_complete_compile() {
    let mut project = load();
    assert!(!project.check_compat(INCOMPATIBLE_GLYPH));
    assert!(project.check_compat("lam_alef-ar"));
    let error = project
        .compile()
        .expect_err("the partial anchor stops the compile");
    assert!(error.contains(PARTIAL_ANCHOR_GLYPH), "{error}");
    assert!(error.contains("no value at default location"), "{error}");

    project
        .remove_document_glyph(PARTIAL_ANCHOR_GLYPH)
        .expect("the glyph is removable");
    let error = project
        .compile()
        .expect_err("the incompatible contour stops the compile");
    assert!(error.contains(INCOMPATIBLE_GLYPH), "{error}");
    assert!(
        error.contains("interpolation-incompatible paths"),
        "{error}"
    );
}

#[test]
fn joining_forms_and_lam_alef_ligature() {
    let font = finished_font();
    assert_eq!(
        names(&shape(&font, "بسم", true)),
        ["meem-ar.fina", "seen-ar.medi", "beh-ar.init"]
    );
    assert_eq!(names(&shape(&font, "لا", true)), ["lam_alef-ar"]);
    assert_eq!(
        names(&shape(&font, "بلا", true)),
        ["lam_alef-ar.fina", "beh-ar.init"]
    );
}

#[test]
fn marks_attach_to_bases_ligature_components_and_marks() {
    let font = finished_font();

    // Mark to ligature: the fatha follows lam (component 1), the damma follows alef.
    let shaped = shape(&font, "لَاُ", true);
    assert_eq!(names(&shaped), ["damma-ar", "fatha-ar", "lam_alef-ar"]);
    assert_eq!(glyph(&shaped, "fatha-ar").2, 380.0);
    assert_eq!(glyph(&shaped, "damma-ar").2, 140.0);
    assert_eq!(glyph(&shaped, "fatha-ar").3, 760.0);

    // ccmp decomposes alef with hamza, and the hamza lands on component 2.
    let shaped = shape(&font, "لأ", true);
    assert_eq!(names(&shaped), ["hamzaabove-ar", "lam_alef-ar"]);
    let hamza = glyph(&shaped, "hamzaabove-ar");
    assert_eq!((hamza.2, hamza.3), (140.0, 760.0));

    // Mark to mark: the fatha stacks on the `top` anchor of the shadda.
    let shaped = shape(&font, "بَّ", true);
    assert_eq!(names(&shaped), ["fatha-ar", "shadda-ar", "beh-ar"]);
    let shadda = glyph(&shaped, "shadda-ar");
    let fatha = glyph(&shaped, "fatha-ar");
    assert_eq!((shadda.2, shadda.3), (260.0, 320.0));
    assert_eq!((fatha.2, fatha.3), (260.0, 560.0));

    // The hand-written lookup after `# Automatic Code` in `mark` lowers kasra under yeh.
    let under_beh = shape(&font, "بِ", true);
    let under_yeh = shape(&font, "يِ", true);
    assert_eq!(glyph(&under_beh, "kasra-ar").2, 260.0);
    assert_eq!(glyph(&under_beh, "kasra-ar").3, -260.0);
    assert_eq!(glyph(&under_yeh, "kasra-ar").3, -340.0);
}

#[test]
fn cursive_attachment_raises_each_join() {
    let font = finished_font();
    let shaped = shape(&font, "سسس", true);
    assert_eq!(
        names(&shaped),
        ["seen-ar.fina", "seen-ar.medi", "seen-ar.init"]
    );
    let y: Vec<f64> = shaped.iter().map(|glyph| glyph.3).collect();
    assert_eq!(y, [0.0, -80.0, -160.0]);
}

#[test]
fn right_to_left_and_latin_kerning() {
    let font = finished_font();
    let reh_advance = |text: &str, kern: bool| glyph(&shape(&font, text, kern), "reh-ar").1;
    assert_eq!(reh_advance("را", false), 360.0);

    // Group pair: a right-to-left value record changes both advance and placement.
    let shaped = shape(&font, "را", true);
    assert_eq!(names(&shaped), ["alef-ar", "reh-ar"]);
    let reh = glyph(&shaped, "reh-ar");
    assert_eq!((reh.1, reh.2), (300.0, -60.0));

    // Glyph pair with the isolated meem that follows a reh.
    assert_eq!(reh_advance("رم", true), 320.0);
    // The contextual rule that depends on a following fatha adds 40.
    assert_eq!(reh_advance("رَا", true), 340.0);
    // `lookupflag IgnoreMarks` kerns across the damma.
    assert_eq!(reh_advance("رُه", true), 330.0);
    assert_eq!(reh_advance("رُه", false), 360.0);

    let shaped = shape(&font, "AV", true);
    assert_eq!(names(&shaped), ["A", "V"]);
    assert_eq!(glyph(&shaped, "A").1, 530.0);
    assert_eq!(glyph(&shape(&font, "AV", false), "A").1, 600.0);
}

#[test]
fn regular_source_has_the_feature_file() {
    assert!(fonts::arabic_dir().join("generate.py").is_file());
    assert!(fonts::arabic_regular_ufo().join("features.fea").is_file());
}

/// The layout tables built from live values give the same text as the complete compile,
/// and the unfinished glyphs do not stop them.
#[test]
fn live_layout_tables_equal_the_complete_compile_with_unfinished_glyphs_present() {
    use crate::text::layout_tables::{LayoutInputs, build_shaping_font};

    let project = load();
    assert!(
        project.compile().is_err(),
        "the unfinished glyphs are present"
    );
    let source = project.document_sources().next().unwrap().id();
    let live = build_shaping_font(&LayoutInputs::from_project(&project, source).unwrap()).unwrap();
    let compiled = finished_font();

    let rows = |font: &ShapingFont, text: &str, rtl: bool| -> Vec<(String, f64, f64, f64)> {
        font.shape_with_options(text, rtl, &[], None, None)
            .unwrap()
            .iter()
            .map(|glyph| {
                (
                    font.glyph_name(glyph.glyph_id).unwrap_or("?").to_string(),
                    glyph.x_advance,
                    glyph.x_offset,
                    glyph.y_offset,
                )
            })
            .collect()
    };
    for (text, rtl) in [
        ("بسم", true),
        ("لا", true),
        ("بلا", true),
        ("لَاُ", true),
        ("لأ", true),
        ("بَّ", true),
        ("بِ", true),
        ("يِ", true),
        ("سسس", true),
        ("را", true),
        ("رم", true),
        ("رَا", true),
        ("رُه", true),
        ("AV", false),
        ("To 012.", false),
        ("تت", true),
        ("تَت", true),
        ("تِ", true),
        ("بـسم", true),
        ("بࣰ", true),
        ("بۡ", true),
        ("١٢", false),
        ("ر١", true),
    ] {
        assert_eq!(rows(&live, text, rtl), rows(&compiled, text, rtl), "{text}");
    }
    // The composites take the anchors of their components in both.
    let off = [("ccmp".to_owned(), false)];
    for text in ["تت", "تَ", "تَتِ"] {
        assert_eq!(
            shape_with(&live, text, true, &off),
            shape_with(&compiled, text, true, &off),
            "{text} without ccmp"
        );
    }
}

/// Shape with explicit feature settings, in the rows of [`shape`] plus the cluster.
fn shape_with(
    font: &ShapingFont,
    text: &str,
    rtl: bool,
    features: &[(String, bool)],
) -> Vec<(String, f64, f64, f64, u32)> {
    font.shape_with_options(text, rtl, features, Some("arab"), None)
        .expect("the text shapes")
        .into_iter()
        .map(|glyph| {
            (
                font.glyph_name(glyph.glyph_id)
                    .expect("every shaped glyph has a name")
                    .to_owned(),
                glyph.x_advance,
                glyph.x_offset,
                glyph.y_offset,
                glyph.cluster,
            )
        })
        .collect()
}

#[test]
fn skeleton_and_dots_through_ccmp_and_composites_without_ccmp() {
    let font = finished_font();
    let row = |name: &str, advance: f64, x: f64, y: f64, cluster: u32| {
        (name.to_owned(), advance, x, y, cluster)
    };
    // `ccmp` decomposes each teh into the skeleton and the dots, so one character gives two
    // glyphs with the same cluster, and the dots attach to the `top` anchor of the skeleton.
    assert_eq!(
        shape_with(&font, "تت", true, &[]),
        [
            row("twodotshorizontalabove-ar", 0.0, 270.0, 320.0, 2),
            row("behDotless-ar.fina", 480.0, 0.0, 0.0, 2),
            row("twodotshorizontalabove-ar", 0.0, 120.0, 320.0, 0),
            row("behDotless-ar.init", 300.0, 0.0, 0.0, 0),
        ]
    );
    // A fatha stacks on the `top` anchor of the dots, by mark-to-mark attachment.
    let shaped = shape_with(&font, "تَت", true, &[]);
    let fatha = shaped.iter().find(|glyph| glyph.0 == "fatha-ar").unwrap();
    assert_eq!((fatha.2, fatha.3), (120.0, 520.0));
    // A kasra attaches to the skeleton, which has no dot below.
    let shaped = shape_with(&font, "تِ", true, &[]);
    let kasra = shaped.iter().find(|glyph| glyph.0 == "kasra-ar").unwrap();
    assert_eq!((kasra.2, kasra.3), (260.0, -140.0));

    // Without ccmp the composites are shown. They have no anchors of their own, and the fatha
    // attaches to the `top` that the composite takes from its dots component.
    let off = [("ccmp".to_owned(), false)];
    let names = |text: &str| -> Vec<String> {
        shape_with(&font, text, true, &off)
            .into_iter()
            .map(|glyph| glyph.0)
            .collect()
    };
    assert_eq!(names("تت"), ["teh-ar.fina", "teh-ar.init"]);
    let shaped = shape_with(&font, "تَ", true, &off);
    let fatha = shaped.iter().find(|glyph| glyph.0 == "fatha-ar").unwrap();
    assert_eq!((fatha.2, fatha.3), (260.0, 520.0));
}

#[test]
fn tatweel_quranic_marks_and_digits() {
    let font = finished_font();
    let names = |text: &str| -> Vec<String> {
        shape_with(&font, text, true, &[])
            .into_iter()
            .map(|glyph| glyph.0)
            .collect()
    };
    // The tatweel joins on both sides.
    assert_eq!(
        names("بـسم"),
        ["meem-ar.fina", "seen-ar.medi", "tatweel-ar", "beh-ar.init"]
    );
    for (text, mark) in [
        ("بࣰ", "fathatanOpen-ar"),
        ("بۡ", "smallhighdotlessheadofkhah-ar"),
    ] {
        let shaped = shape_with(&font, text, true, &[]);
        let found = shaped.iter().find(|glyph| glyph.0 == mark).unwrap();
        assert_eq!((found.2, found.3), (260.0, 320.0), "{text}");
    }
    // Arabic-Indic digits are a left-to-right pair, which the kerning keeps.
    let digits = shape_with(&font, "١٢", false, &[]);
    assert_eq!(digits[0].0, "one-ar");
    assert_eq!(digits[0].1, 470.0);
    // The pair of reh and a digit has mixed direction; fontc drops it.
    let mixed = shape_with(&font, "ر١", true, &[]);
    assert_eq!(
        mixed.iter().map(|glyph| glyph.1).collect::<Vec<_>>(),
        [500.0, 360.0]
    );
}

/// The ligature caret positions of the GDEF table, by glyph id of the covered glyph.
fn ligature_carets(bytes: &[u8]) -> Vec<(u16, Vec<i16>)> {
    use skrifa::raw::{TableProvider, tables::gdef::CaretValue};
    let font = skrifa::FontRef::new(bytes).expect("a font");
    let gdef = font.gdef().expect("a GDEF table");
    let Some(list) = gdef.lig_caret_list() else {
        return Vec::new();
    };
    let list = list.expect("a readable caret list");
    let coverage = list.coverage().expect("a caret coverage");
    coverage
        .iter()
        .zip(list.lig_glyphs().iter())
        .map(|(glyph, ligature)| {
            let carets = ligature
                .expect("a readable ligature")
                .caret_values()
                .iter()
                .map(|caret| match caret.expect("a readable caret") {
                    CaretValue::Format1(value) => value.coordinate(),
                    // The variable export writes the default coordinate with a device table.
                    CaretValue::Format3(value) => value.coordinate(),
                    CaretValue::Format2(_) => i16::MIN,
                })
                .collect();
            (glyph.to_u16(), carets)
        })
        .collect()
}

#[test]
fn the_ligature_caret_reaches_gdef_in_the_export_and_the_live_tables() {
    use crate::text::layout_tables::{LayoutInputs, build_shaping_font};

    let mut project = load();
    for glyph in [INCOMPATIBLE_GLYPH, PARTIAL_ANCHOR_GLYPH] {
        project.remove_document_glyph(glyph).unwrap();
    }
    let compiled = project.compile().unwrap();
    let source = project.document_sources().next().unwrap().id();
    let live = build_shaping_font(&LayoutInputs::from_project(&project, source).unwrap()).unwrap();
    let exported = ShapingFont::from_bytes(compiled.bytes.to_vec()).unwrap();
    let carets = |font: &ShapingFont, bytes: &[u8]| -> Vec<(String, Vec<i16>)> {
        ligature_carets(bytes)
            .into_iter()
            .map(|(glyph, carets)| (font.glyph_name(glyph).unwrap().to_owned(), carets))
            .collect()
    };
    let expected = vec![
        ("lam_alef-ar".to_owned(), vec![260]),
        ("lam_alef-ar.fina".to_owned(), vec![250]),
    ];
    assert_eq!(carets(&exported, &compiled.bytes), expected);
    assert_eq!(carets(&live, &live.clone().into_bytes()), expected);
}
