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
    assert!(error.contains("interpolation-incompatible paths"), "{error}");
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
