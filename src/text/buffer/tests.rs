// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(test)]

//! Tests for the text buffer.

use super::*;

/// Typing after a seeded active sort must lay out to the right of
/// it, with the active index stable. Regression test for a GPUI
/// overlap.
#[test]
fn typing_after_active_sort_lays_out_beside_it() {
    let path = crate::testing::fonts::regular_ufo();
    let font = norad::Font::load(path).expect("fixture UFO loads");
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(TextGlyphInventory::from_font(&font));
    buffer.set_kerning_model(TextKerningModel::from_font(&font));

    let one = font.get_glyph("one").unwrap();
    buffer.insert_glyph("one", Some('1'), one.width);
    buffer.activate_sort(0);
    assert_eq!(buffer.cursor(), 1);

    assert!(buffer.insert_character('a'));
    assert!(buffer.insert_character('s'));

    assert_eq!(buffer.active_sort(), Some(0), "active sort must stay");
    let layout = buffer.layout(1200.0);
    assert_eq!(layout.items.len(), 3);
    let one_item = layout.items.iter().find(|i| i.index == 0).unwrap();
    let a_item = layout.items.iter().find(|i| i.index == 1).unwrap();
    let s_item = layout.items.iter().find(|i| i.index == 2).unwrap();
    assert_eq!(one_item.x, 0.0);
    let kern_one_a = crate::font::font_ops::kern_value(&font, "one", "a");
    assert!(
        (a_item.x - (one.width + kern_one_a)).abs() < 1e-6,
        "a at {} expected {}",
        a_item.x,
        one.width + kern_one_a
    );
    assert!(a_item.x >= one.width - 100.0, "a must not overlap one");
    assert!(s_item.x > a_item.x);
}

/// The native constructors must agree with the norad-level kerning
/// resolution the editors already use in `glyph_ops`.
#[test]
fn from_font_builds_working_models() {
    let path = crate::testing::fonts::regular_ufo();
    let font = norad::Font::load(path).expect("fixture UFO loads");
    let inventory = TextGlyphInventory::from_font(&font);
    let kerning = TextKerningModel::from_font(&font);

    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(inventory);
    buffer.set_kerning_model(kerning);

    // Characters resolve to glyphs with real advances.
    assert!(buffer.insert_character('A'));
    assert!(buffer.insert_character('B'));
    let a_width = font.get_glyph("A").unwrap().width;
    let layout = buffer.layout(1200.0);
    assert_eq!(layout.items.len(), 2);
    assert_eq!(layout.items[0].advance_width, a_width);

    // Find an encoded pair with a nonzero kern and check the
    // buffer applies exactly what glyph_ops resolves.
    let mut checked = false;
    'outer: for glyph in font.default_layer().iter() {
        let Some(lc) = glyph.codepoints.iter().next() else {
            continue;
        };
        for other in font.default_layer().iter() {
            let Some(rc) = other.codepoints.iter().next() else {
                continue;
            };
            let expected = crate::font::font_ops::kern_value(
                &font,
                glyph.name().as_str(),
                other.name().as_str(),
            );
            if expected == 0.0 {
                continue;
            }
            let mut b = TextBuffer::new();
            b.set_glyph_inventory(TextGlyphInventory::from_font(&font));
            b.set_kerning_model(TextKerningModel::from_font(&font));
            assert!(b.insert_character(lc));
            assert!(b.insert_character(rc));
            let l = b.layout(1200.0);
            let gap = l.items[1].x - l.items[0].x - l.items[0].advance_width;
            assert!(
                (gap - expected).abs() < 1e-6,
                "{}/{}: gap {gap} expected {expected}",
                glyph.name(),
                other.name()
            );
            checked = true;
            break 'outer;
        }
    }
    assert!(checked, "fixture font has no kerned encoded pair");
}

#[test]
fn canonical_project_builds_the_same_text_inputs_as_its_source_boundary() {
    let mut font = norad::Font::new();
    font.font_info.units_per_em = Some(1000_u32.into());
    font.features = "feature liga { sub A V by A; } liga;".into();
    for (name, codepoint, width) in [("A", 'A', 600.0), ("V", 'V', 620.0)] {
        let mut glyph = norad::Glyph::new(name);
        glyph.codepoints.insert(codepoint);
        glyph.width = width;
        font.default_layer_mut().insert_glyph(glyph);
    }
    font.kerning
        .entry(norad::Name::new("A").unwrap())
        .or_default()
        .insert(norad::Name::new("V").unwrap(), -80.5);
    let designspace = crate::font::persistence::memory::designspace_from_str(
        r#"<designspace format="5.0">
          <axes><axis tag="wght" name="Weight" minimum="400" default="400" maximum="900"/></axes>
          <sources><source filename="Regular.ufo" name="regular"><location><dimension name="Weight" xvalue="400"/></location></source></sources>
        </designspace>"#,
    )
    .unwrap();
    let project = crate::font::project::Project::from_designspace(designspace, |path| {
        Ok(crate::font::project::SourceInput::from_font(
            font.clone(),
            path.into(),
        ))
    })
    .expect("fixture project loads");
    let source = project
        .document_sources()
        .next()
        .expect("standalone UFO has one source")
        .id();

    assert_eq!(
        TextGlyphInventory::from_project(&project, source).expect("canonical inventory"),
        TextGlyphInventory::from_font(&font),
    );
    assert_eq!(
        TextKerningModel::from_project(&project, source).expect("canonical kerning"),
        TextKerningModel::from_font(&font),
    );
}

#[test]
fn insert_glyph_moves_cursor_and_sets_active_sort() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);

    assert_eq!(buffer.len(), 2);
    assert_eq!(buffer.cursor(), 2);
    assert_eq!(buffer.active_sort(), Some(1));
    assert_eq!(
        buffer.iter().last().and_then(TextSort::glyph_name),
        Some("B")
    );
}

#[test]
fn insert_inactive_glyph_preserves_active_sort() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.set_cursor(0);
    buffer.insert_inactive_glyph("B", Some('B'), 610.0);

    assert_eq!(buffer.len(), 2);
    assert_eq!(buffer.cursor(), 1);
    assert_eq!(buffer.active_sort(), Some(1));
    assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("B"));
    assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("A"));
}

#[test]
fn activate_sort_preserves_cursor_position() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);
    buffer.set_cursor(0);

    assert!(buffer.activate_sort(1));

    assert_eq!(buffer.active_sort(), Some(1));
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn active_sort_flags_remain_unique_after_switch_and_insert() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);
    buffer.insert_glyph("C", Some('C'), 620.0);

    assert!(buffer.activate_sort(0));
    assert_eq!(
        buffer
            .iter()
            .enumerate()
            .filter_map(|(index, sort)| sort.active.then_some(index))
            .collect::<Vec<_>>(),
        vec![0]
    );

    assert!(buffer.activate_sort(2));
    assert_eq!(
        buffer
            .iter()
            .enumerate()
            .filter_map(|(index, sort)| sort.active.then_some(index))
            .collect::<Vec<_>>(),
        vec![2]
    );

    buffer.set_cursor(0);
    buffer.insert_glyph("D", Some('D'), 630.0);
    assert_eq!(
        buffer
            .iter()
            .enumerate()
            .filter_map(|(index, sort)| sort.active.then_some(index))
            .collect::<Vec<_>>(),
        vec![0]
    );
}

/// The stored text of a buffer holds a line break as a line ending.
/// A buffer built again from that text keeps its lines, where `insert_characters` joined them.
#[test]
fn stored_text_restores_its_line_breaks() {
    let inventory = || {
        serde_json::from_str(
            r#"{
                "unicode": { "65": "A", "66": "B" },
                "widths": { "A": 640, "B": 600 }
            }"#,
        )
        .expect("valid glyph inventory")
    };
    let mut typed = TextBuffer::new();
    typed.set_glyph_inventory(inventory());
    assert!(typed.insert_character('A'));
    typed.insert_line_break();
    assert!(typed.insert_character('B'));
    let stored = typed.text();
    assert_eq!(stored, "A\nB");

    let mut restored = TextBuffer::new();
    restored.set_glyph_inventory(inventory());
    assert_eq!(restored.insert_text(&stored), 3);
    assert_eq!(restored.text(), stored);
    assert_eq!(restored.len(), typed.len());

    let mut windows = TextBuffer::new();
    windows.set_glyph_inventory(inventory());
    assert_eq!(windows.insert_text("A\r\nB\rZ"), 4, "Z has no glyph");
    assert_eq!(windows.text(), "A\nB\n");
}

#[test]
fn insert_character_uses_glyph_inventory() {
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": { "65": "A" },
                "widths": { "A": 640 }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('A'));
    assert!(!buffer.insert_character('Z'));

    assert_eq!(buffer.len(), 1);
    assert_eq!(buffer.cursor(), 1);
    assert_eq!(buffer.active_sort(), None);
    assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
    let TextSortKind::Glyph {
        codepoint,
        advance_width,
        ..
    } = &buffer.sort(0).expect("sort exists").kind
    else {
        panic!("expected glyph sort");
    };
    assert_eq!(*codepoint, Some('A'));
    assert_eq!(*advance_width, 640.0);
}

#[test]
fn batch_characters_preserve_selection_and_active_sort_identity() {
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(
        serde_json::from_str(r#"{"unicode":{"65":"A","86":"V"},"widths":{"A":600,"V":620}}"#)
            .unwrap(),
    );
    buffer.insert_inactive_glyph("A", Some('A'), 600.0);
    buffer.insert_inactive_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("V", Some('V'), 620.0);
    buffer.select_range(0, 1);
    let before = buffer.clone();
    assert_eq!(buffer.insert_characters(""), 0);
    assert_eq!(buffer.insert_characters("\u{10FFFF}\n"), 0);
    assert_eq!(
        buffer, before,
        "unmapped text must not delete the selection"
    );

    assert_eq!(buffer.insert_characters("\u{10FFFF}VA"), 2);
    assert_eq!(buffer.text(), "VAAV");
    assert_eq!(buffer.cursor(), 2);
    assert_eq!(buffer.selection_range(), None);
    assert_eq!(buffer.active_sort(), Some(3));
    assert_eq!(buffer.sort(3).unwrap().glyph_name(), Some("V"));
    assert_eq!(buffer.layout(1000.0).items[1].x, 620.0);
}

#[test]
fn batch_characters_match_incremental_shaping_and_bidi_layout() {
    let inventory: TextGlyphInventory = serde_json::from_str(
        r#"{
            "unicode":{"32":"space","65":"A","102":"f","105":"i","1576":"beh-ar","1605":"meem-ar"},
            "widths":{"space":250,"A":600,"f":300,"i":200,"fi":450,
                      "beh-ar":500,"beh-ar.init":480,"meem-ar":520,"meem-ar.fina":490}
        }"#,
    )
    .unwrap();
    for features in ["", "feature liga { sub f i by fi; } liga;"] {
        for direction in [
            None,
            Some(TextDirection::LeftToRight),
            Some(TextDirection::RightToLeft),
        ] {
            for text in ["fi A بم", "بم fi", "A\u{10FFFF}fi"] {
                let mut original = TextBuffer::new();
                let mut inventory = inventory.clone();
                inventory.features = features.into();
                original.set_glyph_inventory(inventory);
                if let Some(direction) = direction {
                    original.set_direction(direction);
                }
                let mut batch = original.clone();
                let mut inserted = 0;
                for character in text.chars() {
                    inserted += usize::from(original.insert_character(character));
                }
                original.shape_arabic_if_rtl();
                assert_eq!(batch.insert_characters(text), inserted);
                assert_eq!(batch.sorts, original.sorts, "{text}");
                assert_eq!(batch.cursor(), original.cursor());
                assert_eq!(batch.layout(1000.0), original.layout(1000.0), "{text}");
                if !features.is_empty() {
                    assert!(batch.iter().any(|sort| sort.glyph_name() == Some("fi")));
                    assert!(
                        !batch.shape_arabic_if_rtl(),
                        "unchanged shaping must not fall back"
                    );
                    assert!(!batch.shape_arabic());
                    assert!(!batch.shape_arabic_around_if_rtl(0));
                    assert_eq!(
                        batch.sorts, original.sorts,
                        "repeated shaping preserves ligatures"
                    );
                    assert_eq!(batch.layout(1000.0), original.layout(1000.0));
                }
            }
        }
    }
}

#[test]
fn insert_character_missing_width_uses_xilem_shaper_fallback() {
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": { "65": "A" },
                "outlines": { "A": "M0,0 L10,0" }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('A'));

    let TextSortKind::Glyph { advance_width, .. } = &buffer.sort(0).expect("sort exists").kind
    else {
        panic!("expected glyph sort");
    };
    assert_eq!(*advance_width, 500.0);
}

#[test]
fn clear_resets_direction_like_fresh_xilem_session() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("A", Some('A'), 600.0);

    buffer.clear();

    assert_eq!(buffer.direction(), TextDirection::LeftToRight);
    assert_eq!(buffer.len(), 0);
    assert_eq!(buffer.cursor(), 0);
    assert_eq!(buffer.active_sort(), None);
}

#[test]
fn auto_direction_shapes_arabic_without_pinning_rtl() {
    let mut buffer = TextBuffer::new();
    // No set_direction call: Auto mode must shape Arabic on its own.
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1605": "meem-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "meem-ar": 520,
                    "meem-ar.fina": 500
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0645}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("meem-ar.fina")
    );
}

#[test]
fn insert_character_shapes_rtl_arabic_neighbors() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1605": "meem-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "meem-ar": 520,
                    "meem-ar.fina": 500
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0645}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("meem-ar.fina")
    );
}

#[test]
fn rtl_arabic_shaping_context_crosses_line_breaks_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    buffer.insert_line_break();
    assert!(buffer.insert_character('\u{0647}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar")
    );
    assert!(matches!(
        buffer.sort(1).map(|sort| &sort.kind),
        Some(TextSortKind::LineBreak)
    ));
    assert_eq!(
        buffer.sort(2).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn rtl_arabic_insert_after_transparent_mark_reshapes_previous_letter() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1614": "fatha-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "fatha-ar": 0,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{064e}'));
    assert!(buffer.insert_character('\u{0647}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("fatha-ar")
    );
    assert_eq!(
        buffer.sort(2).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn rtl_arabic_tatweel_joins_neighbors_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1600": "tatweel-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "tatweel-ar": 250,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0640}'));
    assert!(buffer.insert_character('\u{0647}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("tatweel-ar")
    );
    assert_eq!(
        buffer.sort(2).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn rtl_arabic_positional_glyph_can_exist_without_width_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                },
                "outlines": {
                    "beh-ar.init": "M0,0 L10,0"
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0647}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn rtl_arabic_delete_transparent_mark_repairs_joining_neighbors() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1614": "fatha-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "fatha-ar": 0,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.insert_glyph("fatha-ar", Some('\u{064e}'), 0.0);
    buffer.insert_glyph("heh-ar", Some('\u{0647}'), 510.0);
    buffer.set_cursor(2);

    assert!(buffer.delete_before_cursor().is_some());
    assert!(buffer.shape_arabic_around_if_rtl(buffer.cursor()));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn rtl_arabic_insert_right_joining_sort_reshapes_next_letter() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1575": "alef-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "alef-ar": 450,
                    "alef-ar.fina": 430,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0647}'));
    buffer.set_cursor(1);
    assert!(buffer.insert_character('\u{0627}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("alef-ar.fina")
    );
    assert_eq!(
        buffer.sort(2).and_then(TextSort::glyph_name),
        Some("heh-ar")
    );
}

#[test]
fn rtl_arabic_insert_latin_separator_breaks_joining_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "65": "A",
                    "1576": "beh-ar",
                    "1605": "meem-ar"
                },
                "widths": {
                    "A": 700,
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "meem-ar": 520,
                    "meem-ar.fina": 500
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0645}'));
    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("meem-ar.fina")
    );

    buffer.set_cursor(1);
    assert!(buffer.insert_character('A'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar")
    );
    assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("A"));
    assert_eq!(
        buffer.sort(2).and_then(TextSort::glyph_name),
        Some("meem-ar")
    );
}

#[test]
fn rtl_arabic_delete_latin_separator_repairs_joining_neighbors() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "65": "A",
                    "1576": "beh-ar",
                    "1605": "meem-ar"
                },
                "widths": {
                    "A": 700,
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "meem-ar": 520,
                    "meem-ar.fina": 500
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('A'));
    assert!(buffer.insert_character('\u{0645}'));
    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar")
    );
    assert_eq!(
        buffer.sort(2).and_then(TextSort::glyph_name),
        Some("meem-ar")
    );

    buffer.set_cursor(2);
    assert!(buffer.delete_before_cursor().is_some());
    assert!(buffer.shape_arabic_around_if_rtl(buffer.cursor()));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("meem-ar.fina")
    );
}

#[test]
fn insert_character_ltr_preserves_existing_shaped_forms() {
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "65": "A",
                    "1576": "beh-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "A": 700,
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );
    buffer.set_direction(TextDirection::RightToLeft);
    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0647}'));
    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );

    buffer.set_direction(TextDirection::LeftToRight);
    assert!(buffer.insert_character('A'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
    assert_eq!(buffer.sort(2).and_then(TextSort::glyph_name), Some("A"));
}

#[test]
fn delete_before_cursor_updates_active_sort() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);
    buffer.activate_sort(1);
    buffer.set_cursor(1);

    let deleted = buffer.delete_before_cursor();

    assert_eq!(deleted.as_ref().and_then(TextSort::glyph_name), Some("A"));
    assert_eq!(buffer.cursor(), 0);
    assert_eq!(buffer.active_sort(), Some(0));
}

#[test]
fn delete_before_cursor_clears_deleted_active_sort_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);
    buffer.insert_glyph("C", Some('C'), 620.0);
    buffer.activate_sort(1);
    buffer.set_cursor(2);

    let deleted = buffer.delete_before_cursor();

    assert_eq!(deleted.as_ref().and_then(TextSort::glyph_name), Some("B"));
    assert_eq!(buffer.cursor(), 1);
    assert_eq!(buffer.active_sort(), None);
    assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
    assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("C"));
    assert!(!buffer.iter().any(|sort| sort.active));
}

#[test]
fn delete_after_cursor_clears_deleted_active_sort_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);
    buffer.insert_glyph("C", Some('C'), 620.0);
    buffer.activate_sort(1);
    buffer.set_cursor(1);

    let deleted = buffer.delete_after_cursor();

    assert_eq!(deleted.as_ref().and_then(TextSort::glyph_name), Some("B"));
    assert_eq!(buffer.cursor(), 1);
    assert_eq!(buffer.active_sort(), None);
    assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
    assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("C"));
    assert!(!buffer.iter().any(|sort| sort.active));
}

#[test]
fn line_break_preserves_active_sort() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_line_break();

    assert_eq!(buffer.len(), 2);
    assert_eq!(buffer.cursor(), 2);
    assert_eq!(buffer.active_sort(), Some(0));
}

#[test]
fn line_break_before_active_shifts_active_sort_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);
    buffer.activate_sort(1);
    buffer.set_cursor(1);

    buffer.insert_line_break();

    assert_eq!(buffer.cursor(), 2);
    assert_eq!(buffer.active_sort(), Some(2));
    assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
    assert!(matches!(
        buffer.sort(1).map(|sort| &sort.kind),
        Some(TextSortKind::LineBreak)
    ));
    assert_eq!(buffer.sort(2).and_then(TextSort::glyph_name), Some("B"));
    assert!(buffer.sort(2).is_some_and(|sort| sort.active));
}

#[test]
fn typed_sort_before_active_shifts_active_sort() {
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": { "65": "A", "66": "B" },
                "widths": { "A": 640, "B": 650 }
            }"#,
        )
        .expect("valid glyph inventory"),
    );
    buffer.insert_glyph("B", Some('B'), 650.0);
    buffer.set_cursor(0);

    assert!(buffer.insert_character('A'));

    assert_eq!(buffer.cursor(), 1);
    assert_eq!(buffer.active_sort(), Some(1));
    assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
    assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("B"));
}

#[test]
fn visual_cursor_movement_respects_direction() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 600.0);

    buffer.move_cursor_visual_left();
    assert_eq!(buffer.cursor(), 1);

    buffer.set_direction(TextDirection::RightToLeft);
    buffer.move_cursor_visual_left();
    assert_eq!(buffer.cursor(), 2);
    buffer.move_cursor_visual_right();
    assert_eq!(buffer.cursor(), 1);
}

#[test]
fn keyboard_selection_tracks_logical_sorts_in_both_directions() {
    let mut buffer = TextBuffer::new();
    for name in ["A", "B", "C"] {
        buffer.insert_glyph(name, name.chars().next(), 500.0);
    }

    buffer.extend_selection_visual_left();
    buffer.extend_selection_visual_left();
    assert_eq!(buffer.selection_range(), Some(1..3));
    assert_eq!(
        buffer
            .delete_after_cursor()
            .as_ref()
            .and_then(TextSort::glyph_name),
        Some("B")
    );
    assert_eq!(buffer.cursor(), 1);
    assert_eq!(buffer.selection_range(), None);
    assert_eq!(
        buffer
            .iter()
            .filter_map(TextSort::glyph_name)
            .collect::<Vec<_>>(),
        vec!["A"]
    );

    buffer.insert_glyph("B", Some('B'), 500.0);
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_cursor(0);
    buffer.extend_selection_visual_left();
    buffer.extend_selection_visual_left();
    assert_eq!(buffer.selection_range(), Some(0..2));
    buffer.move_cursor_visual_right();
    assert_eq!(buffer.selection_range(), None);
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn typing_replaces_the_keyboard_selection() {
    let mut buffer = TextBuffer::new();
    for name in ["A", "B", "C"] {
        buffer.insert_glyph(name, name.chars().next(), 500.0);
    }
    buffer.extend_selection_visual_left();
    buffer.extend_selection_visual_left();

    buffer.insert_glyph("D", Some('D'), 500.0);

    assert_eq!(buffer.selection_range(), None);
    assert_eq!(buffer.cursor(), 2);
    assert_eq!(
        buffer
            .iter()
            .filter_map(TextSort::glyph_name)
            .collect::<Vec<_>>(),
        vec!["A", "D"]
    );
}

#[test]
fn selected_text_preserves_unicode_marks_and_line_breaks() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.insert_glyph("kasra-ar", Some('\u{0650}'), 0.0);
    buffer.insert_line_break();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.select_range(0, buffer.len());

    assert_eq!(buffer.text(), "\u{0628}\u{0650}\nA");
    assert_eq!(
        buffer.selected_text().as_deref(),
        Some("\u{0628}\u{0650}\nA")
    );
}

#[test]
fn hit_test_activates_clicked_ltr_sort() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);

    let hit = buffer.hit_test(650.0, 200.0, 1000.0, 800.0, -200.0);

    assert_eq!(hit.active_sort, Some(1));
    assert_eq!(hit.cursor, 2);
}

#[test]
fn hit_test_rejects_sort_above_ascender() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);

    let hit = buffer.hit_test(650.0, 900.0, 1000.0, 800.0, -200.0);

    assert_eq!(hit.active_sort, None);
    assert_eq!(hit.cursor, 1);
}

/// Three lines of two glyphs each, 500 units wide, 1000-unit lines.
fn three_line_buffer() -> TextBuffer {
    let mut buffer = TextBuffer::new();
    for line in 0..3 {
        if line > 0 {
            buffer.insert_line_break();
        }
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("B", Some('B'), 500.0);
    }
    buffer
}

#[test]
fn cursor_moves_up_and_down_between_lines() {
    let mut buffer = three_line_buffer();
    // Caret sits after the last glyph of the last line.
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 2);

    assert!(buffer.move_cursor_vertically(-1, 1000.0));
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 1);
    assert!(buffer.move_cursor_vertically(-1, 1000.0));
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 0);

    assert!(buffer.move_cursor_vertically(1, 1000.0));
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 1);
}

#[test]
fn cursor_keeps_its_column_when_changing_line() {
    let mut buffer = three_line_buffer();
    // Between the two glyphs of the bottom line.
    buffer.set_cursor(7);
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 2);

    assert!(buffer.move_cursor_vertically(-1, 1000.0));
    // Same offset into the line above, not its start or end.
    let (line_start, _) = buffer.line_range_for_number(1);
    assert_eq!(buffer.cursor(), line_start + 1);
}

#[test]
fn cursor_stops_at_the_first_and_last_line() {
    let mut buffer = three_line_buffer();
    assert!(!buffer.move_cursor_vertically(1, 1000.0));
    buffer.set_cursor(0);
    assert!(!buffer.move_cursor_vertically(-1, 1000.0));
}

#[test]
fn home_and_end_move_within_the_caret_line() {
    let mut buffer = three_line_buffer();
    // Sorts are [A B ↵ A B ↵ A B], so the middle line spans 3..5.
    buffer.set_cursor(4); // between the middle line's two glyphs
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 1);

    buffer.move_cursor_to_line_edge(true);
    assert_eq!(buffer.cursor(), 5);
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 1);

    buffer.move_cursor_to_line_edge(false);
    assert_eq!(buffer.cursor(), 3);
}

#[test]
fn click_places_the_caret_between_sorts() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);

    // Left half of the first glyph: before it.
    assert_eq!(buffer.place_cursor_at(100.0, 0.0, 1000.0, 800.0, -200.0), 0);
    // Right half of the first glyph: between the two.
    assert_eq!(buffer.place_cursor_at(400.0, 0.0, 1000.0, 800.0, -200.0), 1);
    // Past the end of the run: after the last glyph.
    assert_eq!(
        buffer.place_cursor_at(2000.0, 0.0, 1000.0, 800.0, -200.0),
        2
    );
}

#[test]
fn click_places_the_caret_on_the_clicked_line() {
    let mut buffer = three_line_buffer();

    // Middle line sits one line-height below the first.
    let cursor = buffer.place_cursor_at(100.0, -1000.0, 1000.0, 800.0, -200.0);
    assert_eq!(buffer.line_number_for_sort(cursor), 1);
}

/// A buffer wired to the pinned integration font's inventory and features, the
/// way the editor sets one up.
#[cfg(feature = "integration-fonts")]
fn buffer_with_shaping_font() -> TextBuffer {
    let ufo_dir = crate::testing::fonts::virtua_regular_ufo();
    let font = norad::Font::load(&ufo_dir).expect("test UFO loads");
    let features = std::fs::read_to_string(ufo_dir.join("features.fea")).expect("features.fea");

    let mut widths = HashMap::new();
    let mut unicode = HashMap::new();
    for glyph in font.layers.default_layer().iter() {
        widths.insert(glyph.name().to_string(), glyph.width);
        for codepoint in glyph.codepoints.iter() {
            unicode.insert(codepoint as u32, glyph.name().to_string());
        }
    }

    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(TextGlyphInventory {
        unicode,
        widths,
        outlines: HashMap::new(),
        features,
        units_per_em: 1000.0,
    });
    buffer
}

#[cfg(feature = "integration-fonts")]
fn type_chars(buffer: &mut TextBuffer, text: &str) {
    for char in text.chars() {
        let name = buffer
            .glyph_inventory
            .unicode
            .get(&(char as u32))
            .cloned()
            .unwrap_or_else(|| ".notdef".to_string());
        let width = buffer
            .glyph_inventory
            .widths
            .get(&name)
            .copied()
            .unwrap_or(0.0);
        buffer.insert_glyph(name, Some(char), width);
    }
}

#[cfg(feature = "integration-fonts")]
#[test]
fn latin_keeps_one_glyph_per_character_when_shaped_by_the_font() {
    let mut buffer = buffer_with_shaping_font();
    buffer.set_direction(TextDirection::LeftToRight);
    type_chars(&mut buffer, "Runebender.org");

    buffer.shape_arabic();

    assert_eq!(buffer.len(), 14);
    let absorbed = (0..buffer.len())
        .filter(|i| buffer.sort(*i).expect("sort").is_absorbed())
        .count();
    assert_eq!(absorbed, 0, "no Latin character should be folded away");
    assert_eq!(buffer.layout(1000.0).items.len(), 14);
    assert_eq!(buffer.sort_glyph_name(0), Some("R"));
}

#[cfg(feature = "integration-fonts")]
#[test]
fn arabic_in_a_latin_line_still_ligates() {
    // A line whose first strong character is Latin still reads LTR,
    // but the Arabic inside it has to be shaped as its own run or
    // the script-specific features never run.
    let mut buffer = buffer_with_shaping_font();
    buffer.set_auto_direction();
    type_chars(&mut buffer, "hi \u{0644}\u{0627}");

    buffer.shape_arabic();

    assert_eq!(buffer.sort_glyph_name(3), Some("lam_alef-ar"));
    assert!(buffer.sort(4).expect("alef sort").is_absorbed());
}

#[test]
fn a_glyph_opens_beside_the_one_being_edited() {
    // Double-clicking a component puts its base next to the glyph
    // that uses it, wherever the cursor happens to be.
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);
    buffer.insert_glyph("C", Some('C'), 500.0);
    buffer.activate_sort(0);
    buffer.set_cursor(3); // cursor parked at the end

    let index = buffer.insert_glyph_after_active("acutecomb", None, 0.0);

    assert_eq!(index, 1);
    assert_eq!(buffer.sort_glyph_name(1), Some("acutecomb"));
    // ...and it is what gets edited.
    assert_eq!(buffer.active_sort(), Some(1));
    assert_eq!(buffer.cursor(), 2);
}

#[test]
fn a_glyph_opens_at_the_cursor_when_nothing_is_active() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);
    buffer.set_active_sort(None);
    buffer.set_cursor(1);

    assert_eq!(buffer.insert_glyph_after_active("C", Some('C'), 500.0), 1);
    assert_eq!(buffer.sort_glyph_name(1), Some("C"));
}

#[cfg(feature = "integration-fonts")]
#[test]
fn lam_alef_renders_as_one_ligature_glyph() {
    let mut buffer = buffer_with_shaping_font();
    type_chars(&mut buffer, "\u{0644}\u{0627}");

    assert!(buffer.shape_arabic(), "shaping changed the buffer");
    assert_eq!(buffer.sort_glyph_name(0), Some("lam_alef-ar"));
    // The alef keeps its place in the buffer — the cursor and editing
    // still see two characters — but draws nothing.
    assert_eq!(buffer.len(), 2);
    assert!(buffer.sort(1).expect("alef sort").is_absorbed());

    // One glyph on the line, and it is the ligature.
    let layout = buffer.layout(1000.0);
    assert_eq!(layout.items.len(), 1);
    assert_eq!(layout.items[0].index, 0);
}

#[cfg(feature = "integration-fonts")]
#[test]
fn deleting_the_lam_brings_the_alef_back() {
    let mut buffer = buffer_with_shaping_font();
    type_chars(&mut buffer, "\u{0644}\u{0627}");
    buffer.shape_arabic();

    buffer.set_cursor(1);
    buffer.delete_before_cursor();
    buffer.shape_arabic();

    assert_eq!(buffer.len(), 1);
    assert!(!buffer.sort(0).expect("alef sort").is_absorbed());
    assert_eq!(buffer.sort_glyph_name(0), Some("alef-ar"));
    assert_eq!(buffer.layout(1000.0).items.len(), 1);
}

#[cfg(feature = "integration-fonts")]
#[test]
fn shaping_falls_back_when_the_feature_file_is_broken() {
    let mut buffer = buffer_with_shaping_font();
    let mut inventory = (*buffer.glyph_inventory).clone();
    inventory.features = "feature liga { sub missing by alsoMissing; } liga;".into();
    buffer.set_glyph_inventory(inventory);
    type_chars(&mut buffer, "\u{0628}\u{0628}");

    // The built-in joining rules still run, so the text stays shaped.
    assert!(buffer.shape_arabic());
    assert_eq!(buffer.sort_glyph_name(0), Some("beh-ar.init"));
    assert_eq!(buffer.sort_glyph_name(1), Some("beh-ar.fina"));
}

#[test]
fn hit_test_places_ltr_cursor_nearest_boundary() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);

    let hit = buffer.hit_test(20.0, 1200.0, 1000.0, 800.0, -200.0);

    assert_eq!(hit.active_sort, None);
    assert_eq!(hit.cursor, 0);
}

#[test]
fn hit_test_uses_xilem_exclusive_sort_max_edges() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 300.0);

    let boundary = buffer.hit_test(500.0, 100.0, 1000.0, 800.0, -200.0);
    assert_eq!(boundary.active_sort, Some(1));
    assert_eq!(boundary.cursor, 2);

    let top_edge = buffer.hit_test(250.0, 800.0, 1000.0, 800.0, -200.0);
    assert_eq!(top_edge.active_sort, None);
    assert_eq!(top_edge.cursor, 0);
}

#[test]
fn hit_test_uses_metric_box_for_ltr_line_selection() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("B", Some('B'), 500.0);

    let hit = buffer.hit_test(250.0, -300.0, 1000.0, 800.0, -200.0);

    assert_eq!(hit.active_sort, Some(2));
    assert_eq!(hit.cursor, 3);
}

#[test]
fn hit_test_uses_rtl_visual_cursor_positions() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);

    let hit = buffer.hit_test(980.0, -1200.0, 1000.0, 800.0, -200.0);

    assert_eq!(hit.active_sort, None);
    assert_eq!(hit.cursor, 0);
}

#[test]
fn hit_test_uses_metric_box_for_rtl_line_selection() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("B", Some('B'), 500.0);

    let hit = buffer.hit_test(250.0, -300.0, 1000.0, 800.0, -200.0);

    assert_eq!(hit.active_sort, Some(2));
    assert_eq!(hit.cursor, 3);
}

#[test]
fn activate_sort_at_returns_layout_origin_for_active_sort() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("B", Some('B'), 300.0);
    buffer.set_cursor(0);

    let activation = buffer
        .activate_sort_at(300.0, -300.0, 1000.0, 800.0, -200.0)
        .expect("sort activates");

    assert_eq!(activation.index, 2);
    assert_eq!(activation.x, 200.0);
    assert_eq!(activation.y, -1000.0);
    assert_eq!(buffer.active_sort(), Some(2));
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn update_glyph_changes_existing_sort_metadata() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);

    assert!(buffer.update_glyph(0, "beh-ar.init", Some('\u{0628}'), 480.0));
    let sort = buffer.sort(0).expect("sort exists");
    assert_eq!(sort.glyph_name(), Some("beh-ar.init"));
    let TextSortKind::Glyph { advance_width, .. } = sort.kind else {
        panic!("expected glyph sort");
    };
    assert_eq!(advance_width, 480.0);
}

#[test]
fn shape_arabic_uses_positional_forms_when_available() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.insert_glyph("heh-ar", Some('\u{0647}'), 510.0);

    assert!(buffer.shape_arabic());

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn shape_arabic_resets_to_base_forms_in_ltr() {
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );
    buffer.insert_glyph("beh-ar.init", Some('\u{0628}'), 480.0);

    assert!(buffer.shape_arabic());

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar")
    );
}

#[test]
fn set_direction_does_not_reshape_existing_sorts_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0647}'));
    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );

    buffer.set_direction(TextDirection::LeftToRight);

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn set_direction_only_changes_direction_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(buffer.begin_manual_kerning(1, 500.0));
    buffer.set_direction(TextDirection::RightToLeft);

    assert_eq!(buffer.direction(), TextDirection::RightToLeft);
    assert_eq!(buffer.manual_kerning_sort(), Some(1));
}

#[test]
fn set_kerning_model_keeps_manual_kerning_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(buffer.begin_manual_kerning(1, 500.0));
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "A": { "V": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    assert_eq!(buffer.manual_kerning_sort(), Some(1));
}

#[test]
fn set_glyph_inventory_keeps_manual_kerning_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(buffer.begin_manual_kerning(1, 500.0));
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": { "65": "A", "86": "V" },
                "widths": { "A": 500, "V": 500 },
                "outlines": {}
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert_eq!(buffer.manual_kerning_sort(), Some(1));
}

#[test]
fn update_glyph_keeps_manual_kerning_like_xilem_width_edit() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(buffer.begin_manual_kerning(1, 500.0));
    assert!(buffer.update_glyph(1, "V", Some('V'), 520.0));

    assert_eq!(buffer.manual_kerning_sort(), Some(1));
    let TextSortKind::Glyph { advance_width, .. } = &buffer.sort(1).expect("sort exists").kind
    else {
        panic!("expected glyph sort");
    };
    assert_eq!(*advance_width, 520.0);
}

#[test]
fn refresh_shaping_uses_live_arabic_widths_without_losing_selection() {
    let inventory = |beh_init: f64| {
        serde_json::from_value(serde_json::json!({
            "unicode": { "1576": "beh-ar", "1607": "heh-ar" },
            "widths": {
                "beh-ar": 500, "beh-ar.init": beh_init,
                "heh-ar": 510, "heh-ar.fina": 490
            }
        }))
        .expect("valid glyph inventory")
    };
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(inventory(480.0));
    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0647}'));
    buffer.select_range(0, 1);
    assert!(buffer.begin_manual_kerning(1, 0.0));

    buffer.set_glyph_inventory(inventory(517.0));
    assert!(buffer.refresh_shaping());

    let TextSortKind::Glyph {
        name,
        advance_width,
        ..
    } = &buffer.sort(0).expect("the beh sort remains").kind
    else {
        panic!("expected glyph sort");
    };
    assert_eq!(name, "beh-ar.init");
    assert_eq!(*advance_width, 517.0);
    assert_eq!(buffer.selection_range(), Some(0..1));
    assert_eq!(buffer.manual_kerning_sort(), Some(1));
}

#[test]
fn layout_positions_ltr_lines_and_cursor() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("B", Some('B'), 300.0);

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items.len(), 2);
    assert_eq!(layout.items[0].x, 0.0);
    assert_eq!(layout.items[0].y, 0.0);
    assert_eq!(layout.items[1].x, 0.0);
    assert_eq!(layout.items[1].y, -1000.0);
    assert_eq!(layout.cursor_x, 300.0);
    assert_eq!(layout.cursor_y, -1000.0);
}

#[test]
fn layout_places_cursor_on_empty_line_after_trailing_line_break_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 300.0);
    buffer.insert_line_break();

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items.len(), 1);
    assert_eq!(layout.cursor_x, 0.0);
    assert_eq!(layout.cursor_y, -1000.0);
}

#[test]
fn layout_applies_direct_kerning_pairs() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "A": { "V": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items[0].x, 0.0);
    assert_eq!(layout.items[1].x, 420.0);
    assert_eq!(layout.cursor_x, 920.0);
}

#[test]
fn manual_kerning_drag_updates_direct_pair() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "A": { "V": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    assert!(buffer.begin_manual_kerning(1, 500.0));
    assert_eq!(buffer.manual_kerning_sort(), Some(1));
    assert_eq!(buffer.drag_manual_kerning(530.0), Some(-50.0));

    let layout = buffer.layout(1000.0);
    assert_eq!(layout.items[1].x, 450.0);
    assert_eq!(layout.cursor_x, 950.0);
    assert!(buffer.end_manual_kerning());
    assert_eq!(buffer.manual_kerning_sort(), None);
}

#[test]
fn manual_kerning_drag_snaps_to_integer_units() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(buffer.begin_manual_kerning(1, 0.0));
    assert_eq!(buffer.drag_manual_kerning(96.16), Some(96.0));
    assert_eq!(
        buffer
            .kerning_model()
            .kerning
            .get("A")
            .and_then(|pairs| pairs.get("V"))
            .copied(),
        Some(96.0)
    );
}

#[test]
fn manual_kerning_enters_noop_session_after_line_break_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(!buffer.begin_manual_kerning(0, 0.0));
    assert!(buffer.begin_manual_kerning(2, 0.0));
    assert_eq!(buffer.manual_kerning_sort(), Some(2));
    assert_eq!(buffer.active_sort(), Some(2));
    assert_eq!(buffer.drag_manual_kerning(30.0), None);
    assert!(buffer.end_manual_kerning());
}

#[test]
fn structural_text_edits_cancel_manual_kerning() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(buffer.begin_manual_kerning(1, 500.0));
    assert_eq!(buffer.manual_kerning_sort(), Some(1));
    buffer.set_cursor(1);
    assert!(buffer.delete_after_cursor().is_some());
    assert_eq!(buffer.manual_kerning_sort(), None);

    buffer.insert_glyph("V", Some('V'), 500.0);
    assert!(buffer.begin_manual_kerning(1, 500.0));
    buffer.clear();
    assert_eq!(buffer.manual_kerning_sort(), None);
}

#[test]
fn layout_applies_group_kerning_pairs() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "groups": {
                    "public.kern1.A": ["A"],
                    "public.kern2.V": ["V"]
                },
                "kerning": {
                    "public.kern1.A": { "public.kern2.V": -90 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items[1].x, 410.0);
    assert_eq!(layout.cursor_x, 910.0);
}

#[test]
fn layout_applies_raw_xilem_group_names_without_public_prefix() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "groups": {
                    "leftRaw": ["A"],
                    "rightRaw": ["V"]
                },
                "kerning": {
                    "leftRaw": { "rightRaw": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items[1].x, 420.0);
    assert_eq!(layout.cursor_x, 920.0);
}

#[test]
fn layout_prioritizes_xilem_glyph_group_hints_before_other_memberships() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "groups": {
                    "firstLeft": ["A"],
                    "hintLeft": ["A"],
                    "firstRight": ["V"],
                    "hintRight": ["V"]
                },
                "leftGroups": { "V": "hintRight" },
                "rightGroups": { "A": "hintLeft" },
                "kerning": {
                    "firstLeft": { "firstRight": -20 },
                    "hintLeft": { "hintRight": -70 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items[1].x, 430.0);
    assert_eq!(layout.cursor_x, 930.0);
}

/// Where a given sort ended up in the preview strip.
fn preview_x(items: &[TextLayoutItem], index: usize) -> f64 {
    items
        .iter()
        .find(|item| item.index == index)
        .unwrap_or_else(|| panic!("sort {index} is in the preview"))
        .x
}

/// Where a given sort ended up. Items come back in the order they
/// are drawn, which for a right-to-left run is not the order the
/// sorts were typed in.
fn item_x(layout: &TextLayout, index: usize) -> f64 {
    layout
        .items
        .iter()
        .find(|item| item.index == index)
        .unwrap_or_else(|| panic!("sort {index} was laid out"))
        .x
}

#[test]
fn latin_inside_an_arabic_line_still_reads_left_to_right() {
    // "ا this ب": an Arabic line with a Latin word in it. The word
    // sits where bidi puts it — to the left of the first Arabic
    // letter — but its own letters run left to right. Reversing the
    // whole line is what made "this is a book" read "koob a si siht".
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 100.0);
    for (name, char) in [("t", 't'), ("h", 'h'), ("i", 'i'), ("s", 's')] {
        buffer.insert_glyph(name, Some(char), 100.0);
    }
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 100.0);

    let layout = buffer.layout(1000.0);

    // The Arabic letters take the outer edges: alef (typed first) on
    // the right, beh on the left.
    assert_eq!(item_x(&layout, 0), 500.0);
    assert_eq!(item_x(&layout, 5), 0.0);
    // t-h-i-s, in that order, between them.
    assert_eq!(item_x(&layout, 1), 100.0);
    assert_eq!(item_x(&layout, 2), 200.0);
    assert_eq!(item_x(&layout, 3), 300.0);
    assert_eq!(item_x(&layout, 4), 400.0);
}

#[test]
fn digits_in_an_arabic_line_read_left_to_right() {
    // Numbers are a weaker case than letters — bidi class EN, not L
    // — and they still run left to right inside an RTL line.
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 100.0);
    buffer.insert_glyph("one", Some('1'), 100.0);
    buffer.insert_glyph("two", Some('2'), 100.0);

    let layout = buffer.layout(1000.0);

    assert_eq!(item_x(&layout, 0), 200.0);
    assert_eq!(item_x(&layout, 1), 0.0);
    assert_eq!(item_x(&layout, 2), 100.0);
}

#[test]
fn arabic_inside_a_latin_line_still_reads_right_to_left() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("a", Some('a'), 100.0);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 100.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 100.0);
    buffer.insert_glyph("b", Some('b'), 100.0);

    let layout = buffer.layout(1000.0);

    assert_eq!(item_x(&layout, 0), 0.0);
    // The Arabic pair is mirrored inside the Latin sentence.
    assert_eq!(item_x(&layout, 1), 200.0);
    assert_eq!(item_x(&layout, 2), 100.0);
    assert_eq!(item_x(&layout, 3), 300.0);
}

#[test]
fn layout_positions_rtl_from_line_width() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 300.0);

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items.len(), 2);
    // First letter typed sits at the right edge.
    assert_eq!(item_x(&layout, 0), 300.0);
    assert_eq!(item_x(&layout, 1), 0.0);
    assert_eq!(layout.cursor_x, 0.0);
    assert_eq!(layout.cursor_y, 0.0);
}

#[test]
fn manual_kerning_drag_flips_sign_in_rtl() {
    // LTR: dragging right (+40) widens the pair by +40.
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("a", Some('a'), 100.0);
    buffer.insert_glyph("b", Some('b'), 100.0);
    assert!(buffer.begin_manual_kerning(1, 500.0));
    assert_eq!(buffer.drag_manual_kerning(540.0), Some(40.0));
    buffer.end_manual_kerning();

    // RTL: the same rightward drag closes the visual gap, so the
    // logical pair's kern goes to −40.
    let mut rtl = TextBuffer::new();
    rtl.set_direction(TextDirection::RightToLeft);
    rtl.insert_glyph("alef-ar", Some('\u{0627}'), 100.0);
    rtl.insert_glyph("beh-ar", Some('\u{0628}'), 100.0);
    assert!(rtl.begin_manual_kerning(1, 500.0));
    assert_eq!(rtl.drag_manual_kerning(540.0), Some(-40.0));
}

#[test]
fn activate_sort_at_uses_rtl_kerned_layout_origin_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "alef-ar": { "beh-ar": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let activation = buffer
        .activate_sort_at(100.0, 0.0, 1000.0, 800.0, -200.0)
        .expect("kerned RTL sort activates");

    assert_eq!(activation.index, 1);
    assert_eq!(activation.x, 80.0);
    assert_eq!(activation.y, 0.0);
    assert_eq!(buffer.active_sort(), Some(1));
}

#[test]
fn rtl_layout_places_cursor_on_empty_line_after_trailing_line_break_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("A", Some('A'), 300.0);
    buffer.insert_line_break();

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items.len(), 1);
    assert_eq!(layout.cursor_x, 300.0);
    assert_eq!(layout.cursor_y, -1000.0);
}

#[test]
fn auto_direction_reads_each_line_from_its_own_script() {
    let mut buffer = TextBuffer::new();
    // Line 1 Latin, line 2 Arabic — the case a single buffer
    // direction could never get right.
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);

    assert!(buffer.direction_is_auto());
    assert_eq!(
        buffer.resolved_line_direction(0),
        TextDirection::LeftToRight
    );
    assert_eq!(
        buffer.resolved_line_direction(1),
        TextDirection::RightToLeft
    );
}

#[test]
fn auto_direction_ignores_neutral_characters() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("one", Some('1'), 500.0);
    buffer.insert_glyph("period", Some('.'), 200.0);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);

    // Digits and punctuation don't decide; the Arabic letter does.
    assert_eq!(
        buffer.resolved_line_direction(0),
        TextDirection::RightToLeft
    );
}

#[test]
fn pinning_a_direction_overrides_detection() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);
    assert_eq!(buffer.cursor_direction(), TextDirection::RightToLeft);

    buffer.set_direction(TextDirection::LeftToRight);
    assert!(!buffer.direction_is_auto());
    assert_eq!(buffer.cursor_direction(), TextDirection::LeftToRight);

    buffer.set_auto_direction();
    assert_eq!(buffer.cursor_direction(), TextDirection::RightToLeft);
}

#[test]
fn mixed_lines_lay_out_in_their_own_directions() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 400.0);

    let layout = buffer.layout(1000.0);

    // Latin line runs rightwards from the origin.
    assert_eq!(item_x(&layout, 0), 0.0);
    // Arabic line right-aligns on the widest line (700) and reads
    // right to left: first letter nearest the right edge.
    assert_eq!(item_x(&layout, 2), 400.0);
    assert_eq!(item_x(&layout, 3), 0.0);
}

#[test]
fn preview_orders_runs_left_to_right_but_fills_each_run_by_direction() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 400.0);

    let preview = buffer.preview_layout();

    // Latin run first, then the Arabic run occupying [500, 1200]
    // with its first letter on the right.
    assert_eq!(preview_x(&preview, 0), 0.0);
    assert_eq!(preview_x(&preview, 2), 900.0);
    assert_eq!(preview_x(&preview, 3), 500.0);
}

#[test]
fn layout_right_aligns_rtl_lines_on_the_widest_line() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 300.0);

    let layout = buffer.layout(1000.0);

    // Both lines share the right edge at x = 500 (the widest line),
    // so the 300-wide second line starts 200 units further left.
    assert_eq!(layout.items.len(), 2);
    assert_eq!(item_x(&layout, 0), 0.0);
    assert_eq!(layout.items[0].y, 0.0);
    assert_eq!(item_x(&layout, 2), 200.0);
    assert_eq!(layout.items[1].y, -1000.0);
    assert_eq!(layout.cursor_x, 200.0);
    assert_eq!(layout.cursor_y, -1000.0);
}

#[test]
fn layout_applies_rtl_kerning_without_shifting_line_start() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "alef-ar": { "beh-ar": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let layout = buffer.layout(1000.0);

    assert_eq!(item_x(&layout, 0), 500.0);
    assert_eq!(item_x(&layout, 1), 80.0);
    assert_eq!(layout.cursor_x, 80.0);
}

#[test]
fn rtl_multiline_layout_resets_kerning_between_lines() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "alef-ar": { "beh-ar": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let layout = buffer.layout(1000.0);

    // Right edge is the widest line: 500 + 500 = 1000 advance
    // units (kerning does not shift the line's start).
    assert_eq!(layout.items.len(), 3);
    assert_eq!(item_x(&layout, 0), 500.0);
    assert_eq!(item_x(&layout, 1), 80.0);
    // The second line kerns from scratch and right-aligns.
    assert_eq!(item_x(&layout, 3), 500.0);
    assert_eq!(layout.cursor_x, 500.0);
    assert_eq!(layout.cursor_y, -1000.0);
}

#[test]
fn preview_layout_keeps_line_breaks_in_one_strip() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("V", Some('V'), 300.0);

    let preview = buffer.preview_layout();

    assert_eq!(preview.len(), 2);
    assert_eq!(preview[0].x, 0.0);
    assert_eq!(preview[0].y, 0.0);
    assert_eq!(preview[1].x, 500.0);
    assert_eq!(preview[1].y, 0.0);

    let canvas = buffer.layout(1000.0);
    assert_eq!(canvas.items[1].x, 0.0);
    assert_eq!(canvas.items[1].y, -1000.0);
}

#[test]
fn preview_layout_breaks_kerning_across_line_breaks() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("V", Some('V'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "A": { "V": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let preview = buffer.preview_layout();

    assert_eq!(preview[1].x, 500.0);
}

#[test]
fn rtl_preview_layout_keeps_line_breaks_in_one_strip() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 300.0);

    let preview = buffer.preview_layout();

    // Both lines read the same way, so the strip carries on across
    // the break: 800 units reading right to left, first glyph on the
    // right.
    assert_eq!(preview.len(), 2);
    assert_eq!(preview_x(&preview, 0), 300.0);
    assert_eq!(preview[0].y, 0.0);
    assert_eq!(preview_x(&preview, 2), 0.0);
    assert_eq!(preview[1].y, 0.0);

    let canvas = buffer.layout(1000.0);
    assert_eq!(item_x(&canvas, 0), 0.0);
    assert_eq!(canvas.items[0].y, 0.0);
    assert_eq!(item_x(&canvas, 2), 200.0);
    assert_eq!(canvas.items[1].y, -1000.0);
}

#[test]
fn rtl_preview_layout_breaks_kerning_across_line_breaks_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "alef-ar": { "beh-ar": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let preview = buffer.preview_layout();

    // The pair straddles a line break, so it does not kern.
    assert_eq!(preview_x(&preview, 0), 500.0);
    assert_eq!(preview_x(&preview, 2), 0.0);
}

#[test]
fn active_visual_kern_uses_logical_pair_for_the_neighbor_on_the_left_in_arabic() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("reh-ar.fina", Some('ر'), 496.0);
    buffer.insert_glyph("teh-ar.init", Some('ت'), 260.0);
    buffer.activate_sort(0);
    assert_eq!(
        buffer.active_visual_kern(true),
        Some(("reh-ar.fina", "teh-ar.init", 0.0))
    );
    assert_eq!(buffer.active_visual_kern(false), None);
    buffer.activate_sort(1);
    assert_eq!(
        buffer.active_visual_kern(false),
        Some(("reh-ar.fina", "teh-ar.init", 0.0))
    );
}

/// A buffer shaped by the live layout tables of the Regular master of the Arabic fixture.
fn arabic_fixture_buffer() -> TextBuffer {
    let project = crate::font::project::Project::load(&crate::testing::fonts::arabic_regular_ufo())
        .expect("the Arabic fixture loads");
    buffer_for_project(&project)
}

/// A buffer shaped by the live layout tables of the first source of `project`.
fn buffer_for_project(project: &crate::font::project::Project) -> TextBuffer {
    use crate::text::layout_tables::{LayoutInputs, build_shaping_font};
    let source = project.document_sources().next().expect("a source").id();
    let font = build_shaping_font(&LayoutInputs::from_project(project, source).unwrap())
        .expect("the live tables build");
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(TextGlyphInventory::from_project(project, source).unwrap());
    buffer.set_kerning_model(TextKerningModel::from_project(project, source).unwrap());
    buffer.set_compiled_font(Some(Arc::new(font.into_bytes())), Vec::new());
    buffer
}

/// Each layout item as (sort, cluster glyph, glyph name, x, y).
fn placed_names(buffer: &TextBuffer) -> Vec<(usize, Option<usize>, String, f64, f64)> {
    buffer
        .layout(1000.0)
        .items
        .iter()
        .map(|item| {
            (
                item.index,
                item.cluster_glyph,
                buffer.item_glyph_name(item).unwrap_or("?").to_owned(),
                item.x,
                item.y,
            )
        })
        .collect()
}

#[test]
fn a_ccmp_decomposition_keeps_every_glyph_of_the_cluster() {
    let mut buffer = arabic_fixture_buffer();
    assert_eq!(buffer.insert_characters("لأ"), 2);
    // `ccmp` decomposes alef with hamza, and `rlig` makes the ligature; the hamza stays with
    // the cluster of the ligature, and the alef is absorbed into it.
    assert_eq!(buffer.sort_glyph_name(0), Some("lam_alef-ar"));
    assert!(buffer.sort(1).unwrap().is_absorbed());
    let names: Vec<&str> = buffer
        .cluster_glyphs(0)
        .iter()
        .map(|glyph| glyph.name.as_str())
        .collect();
    assert_eq!(names, ["hamzaabove-ar", "lam_alef-ar"]);
    assert_eq!(
        placed_names(&buffer),
        [
            (0, Some(0), "hamzaabove-ar".into(), 140.0, 760.0),
            (0, None, "lam_alef-ar".into(), 0.0, 0.0),
        ]
    );
}

#[test]
fn one_character_gives_a_skeleton_and_its_dots() {
    let mut buffer = arabic_fixture_buffer();
    assert_eq!(buffer.insert_characters("تت"), 2);
    // Four glyphs from two characters, in the order of the shaper: right to left, the second
    // teh is drawn first, at the left.
    assert_eq!(
        placed_names(&buffer),
        [
            (1, Some(0), "twodotshorizontalabove-ar".into(), 270.0, 320.0),
            (1, None, "behDotless-ar.fina".into(), 0.0, 0.0),
            (0, Some(0), "twodotshorizontalabove-ar".into(), 600.0, 320.0),
            (0, None, "behDotless-ar.init".into(), 480.0, 0.0),
        ]
    );
    // Each sort names its skeleton, the first glyph in logical order.
    assert_eq!(buffer.sort_glyph_name(0), Some("behDotless-ar.init"));
    assert_eq!(buffer.sort_glyph_name(1), Some("behDotless-ar.fina"));
    assert_eq!(buffer.shaped_offset(0), (0.0, 0.0));

    // The caret between the two letters sits at the left edge of the first one.
    buffer.set_cursor(1);
    assert_eq!(buffer.layout(1000.0).cursor_x, 480.0);
    buffer.set_cursor(2);
    assert_eq!(buffer.layout(1000.0).cursor_x, 0.0);
    // A click inside the skeleton of the first teh hits that sort and its own glyph, and
    // places the caret at the nearest edge of the cluster.
    let hit = buffer.hit_test(700.0, 100.0, 1000.0, 900.0, -400.0);
    assert_eq!(hit.active_sort, Some(0));
    assert_eq!(hit.cluster_glyph, None);
    assert_eq!(hit.cursor, 1);
    assert_eq!(
        buffer.place_cursor_at(770.0, 100.0, 1000.0, 900.0, -400.0),
        0
    );
    assert_eq!(
        buffer.place_cursor_at(490.0, 100.0, 1000.0, 900.0, -400.0),
        1
    );

    // The dots can be the glyph being edited, and they are found by name.
    assert_eq!(
        buffer.find_glyph("twodotshorizontalabove-ar"),
        Some((0, Some(0)))
    );
    assert_eq!(buffer.find_glyph("behDotless-ar.fina"), Some((1, None)));
    assert!(buffer.activate_cluster_glyph(0, Some(0)));
    assert_eq!(
        buffer.active_glyph_name(),
        Some("twodotshorizontalabove-ar")
    );
    assert_eq!(buffer.active_cluster_glyph(), Some(0));
    assert!(!buffer.activate_cluster_glyph(0, Some(5)));
    assert!(buffer.activate_sort(0));
    assert_eq!(buffer.active_glyph_name(), Some("behDotless-ar.init"));
    // The own glyph is `None`, whatever its position.
    assert!(buffer.activate_cluster_glyph(1, Some(1)));
    assert_eq!(buffer.active_cluster_glyph(), None);

    // A typed character before the line clears the clusters until it is shaped again, so
    // no sort draws the glyphs of another.
    buffer.set_cursor(0);
    buffer.insert_line_break();
    assert!(buffer.cluster_glyphs(1).is_empty());
    buffer.shape_arabic_if_rtl();
    assert_eq!(buffer.cluster_glyphs(1).len(), 2);

    // With `ccmp` off, the composites draw one glyph each.
    let mut buffer = arabic_fixture_buffer();
    buffer.set_feature_overrides(vec![("ccmp".into(), false)]);
    buffer.insert_characters("تت");
    let names: Vec<String> = placed_names(&buffer)
        .into_iter()
        .map(|(_, _, name, _, _)| name)
        .collect();
    assert_eq!(names, ["teh-ar.fina", "teh-ar.init"]);
}

#[test]
fn digits_and_latin_are_left_to_right_runs_in_an_arabic_line() {
    let mut buffer = arabic_fixture_buffer();
    assert_eq!(buffer.insert_characters("ب ١٢ AV"), 7);
    assert_eq!(
        buffer.resolved_line_direction(0),
        TextDirection::RightToLeft
    );
    let runs: Vec<(bool, Vec<usize>)> = buffer
        .visual_runs_for_line(0, buffer.len())
        .iter()
        .map(|run| (run.rtl, run.sorts.clone()))
        .collect();
    // Left to right on the screen: the Latin run, a space, the digits, then the beh.
    assert_eq!(
        runs,
        [
            (false, vec![5, 6]),
            (true, vec![4]),
            (false, vec![2, 3]),
            (true, vec![0, 1]),
        ]
    );
    // Each left-to-right run is shaped left to right: the digit pair and the Latin pair kern
    // as left-to-right pairs, on the advance of the first glyph.
    assert_eq!(buffer.sort_glyph_name(2), Some("one-ar"));
    assert_eq!(buffer.sort_advance(2), 470.0);
    assert_eq!(buffer.sort_advance(5), 530.0);
    let x = |index: usize| item_x(&buffer.layout(1000.0), index);
    assert!(x(2) < x(3), "one is left of two");
    assert!(x(5) < x(6), "A is left of V");
    assert!(x(3) < x(0), "the digits are left of the beh");

    // The pair of reh and a digit has mixed direction, and the font has no record of it.
    let mut buffer = arabic_fixture_buffer();
    buffer.insert_characters("ر١");
    assert_eq!(buffer.sort_advance(0), 360.0);
}

/// The skeleton generator of item 6.3 and the multi-glyph clusters of the text line together:
/// empty teh glyphs get a generated `ccmp` decomposition, and the text line shows each teh as
/// its skeleton and its dots, one unit for the caret and for hit testing.
#[test]
fn a_generated_ccmp_decomposition_gives_each_letter_one_multi_glyph_cluster() {
    use crate::text::arabic_features::generate_project;
    let mut project =
        crate::font::project::Project::load(&crate::testing::fonts::arabic_regular_ufo())
            .expect("the Arabic fixture loads");
    let source = project.document_sources().next().expect("a source").id();
    let layer = project.document_source(source).unwrap().default_layer();
    for name in ["teh-ar", "teh-ar.init", "teh-ar.medi", "teh-ar.fina"] {
        project
            .edit_document_layer(name, &layer, |draft| {
                let components: Vec<_> = draft
                    .view()
                    .components()
                    .map(|component| component.id())
                    .collect();
                for component in components {
                    draft.remove_component(component)?;
                }
                draft.clear_contours();
                Ok(())
            })
            .expect("the composite becomes empty");
    }
    // Without the hand-written `ccmp`, the generator writes it.
    let features = project.document_feature_text(source).unwrap().to_owned();
    let start = features.find("feature ccmp {").unwrap();
    let end = features.find("} ccmp;").unwrap() + "} ccmp;".len();
    let features = format!("{}{}", &features[..start], &features[end..]);
    let block = generate_project(&project, source, &features);
    let ccmp = block.feature("ccmp").expect("a generated ccmp");
    assert_eq!(
        ccmp.rules,
        ["sub teh-ar by behDotless-ar twodotshorizontalabove-ar;"]
    );
    assert!(project.set_feature_text(block.apply_to(&features).unwrap()));

    let mut buffer = buffer_for_project(&project);
    assert_eq!(buffer.insert_characters("تت"), 2);
    for (index, skeleton) in [(0, "behDotless-ar.init"), (1, "behDotless-ar.fina")] {
        assert_eq!(buffer.sort_glyph_name(index), Some(skeleton));
        let cluster = buffer.cluster_glyphs(index);
        let names: Vec<&str> = cluster.iter().map(|glyph| glyph.name.as_str()).collect();
        assert_eq!(
            names,
            ["twodotshorizontalabove-ar", skeleton],
            "sort {index}"
        );
        let dots = &cluster[0];
        assert!(dots.x_offset != 0.0 && dots.y_offset != 0.0, "{dots:?}");
    }
    let layout = buffer.layout(1000.0);
    assert_eq!(layout.items.len(), 4);
    // The caret moves over each letter as one unit: from the right edge, past the first teh
    // with its dots, to the left edge.
    let (left, right) = TextBuffer::sort_edges(&layout, 0).unwrap();
    buffer.set_cursor(0);
    assert_eq!(buffer.layout(1000.0).cursor_x, right);
    buffer.move_cursor_visual_left();
    assert_eq!(buffer.cursor(), 1);
    assert_eq!(buffer.layout(1000.0).cursor_x, left);
    // A click anywhere in the box of the first teh, which its dots sit in, hits that letter.
    let hit = buffer.hit_test((left + right) / 2.0, 400.0, 1000.0, 900.0, -400.0);
    assert_eq!((hit.active_sort, hit.cursor), (Some(0), 1));
    // Deleting the letter deletes its skeleton and its dots.
    assert!(buffer.delete_before_cursor().is_some());
    buffer.shape_arabic_if_rtl();
    assert_eq!(buffer.layout(1000.0).items.len(), 2);
    assert_eq!(buffer.sort_glyph_name(0), Some("behDotless-ar"));
}

#[test]
fn kerning_pairs_skip_marks_in_a_right_to_left_line() {
    let mut buffer = arabic_fixture_buffer();
    buffer.insert_characters("رَا");
    assert!(buffer.is_mark_sort(1), "the fatha is a mark");
    assert!(!buffer.is_mark_sort(0) && !buffer.is_mark_sort(2));
    // A drag on the alef edits reh and alef, across the fatha, in their groups.
    let (first, second, boundary, rtl, value) = buffer.drag_pair(2).expect("a pair");
    assert_eq!(
        (first, second, rtl, value),
        ("reh-ar", "alef-ar", true, -60.0)
    );
    // The preview without live tables moves the line before the fatha, which sits on reh,
    // so the fatha moves with its base.
    assert_eq!(boundary, 1);
    assert_eq!(buffer.kern_scope(first, second), [true, true]);
    assert_eq!(
        buffer.pair_groups(first, second),
        [Some("public.kern1.reh"), Some("public.kern2.alef")]
    );
    // A drag on the fatha kerns its base, reh, which has no base before it.
    assert_eq!(buffer.drag_pair(1), None);
    // The metrics card of the alef shows the pair on its right side, reh, and none on its
    // left; the fatha has none.
    buffer.activate_sort(2);
    assert_eq!(
        buffer.active_visual_kern(false),
        Some(("reh-ar", "alef-ar", -60.0))
    );
    assert_eq!(buffer.active_visual_kern(true), None);
    buffer.activate_sort(1);
    assert_eq!(buffer.active_visual_kern(false), None);
    assert_eq!(buffer.active_visual_kern(true), None);
    buffer.activate_sort(0);
    assert_eq!(
        buffer.active_visual_kern(true),
        Some(("reh-ar", "alef-ar", -60.0))
    );
}

#[test]
fn a_right_to_left_run_kerns_right_to_left_in_either_line_direction_and_digits_left_to_right() {
    // An Arabic run in a Latin line.
    let mut buffer = arabic_fixture_buffer();
    buffer.insert_characters("AV را");
    assert_eq!(
        buffer.resolved_line_direction(0),
        TextDirection::LeftToRight
    );
    let (first, second, boundary, rtl, value) = buffer.drag_pair(4).expect("a pair");
    assert_eq!(
        (first, second, boundary, rtl, value),
        ("reh-ar", "alef-ar", 3, true, -60.0)
    );
    let (first, second, boundary, rtl, value) = buffer.drag_pair(1).expect("a pair");
    assert_eq!(
        (first, second, boundary, rtl, value),
        ("A", "V", 1, false, -70.0)
    );

    // Arabic-Indic digits in an Arabic line are a left-to-right run.
    let mut buffer = arabic_fixture_buffer();
    buffer.insert_characters("ر ١٢");
    assert_eq!(
        buffer.resolved_line_direction(0),
        TextDirection::RightToLeft
    );
    let (first, second, boundary, rtl, value) = buffer.drag_pair(3).expect("a pair");
    assert_eq!(
        (first, second, boundary, rtl, value),
        ("one-ar", "two-ar", 3, false, -30.0)
    );
    // The digit after the space starts its run, so it has no pair with reh.
    assert_eq!(buffer.drag_pair(2), None);
}
