// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The sidebar specimen: the selected glyph in the contexts a type designer reads it in,
//! then the basic sets of the scripts the font covers.
//!
//! Every proof uses the same folding section header as the rest of the sidebar.
//! The sidebar owns scrolling; this module shapes and paints only the proof bodies.
//! Left-to-right lines start at the left margin and right-to-left lines at the right one.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

use masonry::imaging::Painter;
use masonry::kurbo::{Affine, BezPath, Size};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::{canvas, sized_box};

use crate::application::design::recipes::{Group, Row};
use crate::application::design::{self, recipes};
use crate::application::editor::tools::text as text_tool;
use crate::application::workspace::Workspace;

/// One shaped line in font units, baseline at zero, starting at x = 0.
#[derive(Clone, Debug)]
struct Run {
    paths: Vec<BezPath>,
    width: f64,
    rtl: bool,
}

/// How a block sizes its lines.
#[derive(Clone, Copy, Debug)]
enum Setting {
    /// Every line at one size: the largest at which the widest line fits, up to `max_em`
    /// pixels of em.
    Even { max_em: f64 },
    /// The first line as large as fits, up to `max_em`, and each next line smaller.
    Waterfall { max_em: f64 },
}

#[derive(Clone, Debug)]
pub(crate) struct Block {
    key: &'static str,
    caption: String,
    lines: Vec<Run>,
    setting: Setting,
}

#[derive(Debug, Default)]
struct Specimen {
    blocks: Vec<Block>,
    ascender: f64,
    descender: f64,
}

/// The document state a specimen was shaped from.
type Key = (u64, usize, Vec<u64>, Option<usize>);

thread_local! {
    static CACHE: RefCell<Option<(Key, Arc<Specimen>)>> = const { RefCell::new(None) };
}

/// The independently folding proofs below Categories, Global Scripts, and Filters.
pub(crate) fn glyph_specimen(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let specimen = specimen(app);
    let sections = specimen
        .blocks
        .iter()
        .map(|block| {
            recipes::section_of(
                app,
                block.key,
                &block.caption,
                vec![Group::new(
                    "",
                    vec![Row::Specimen {
                        block: block.clone(),
                        ascender: specimen.ascender,
                        descender: specimen.descender,
                    }],
                )],
            )
            .boxed()
        })
        .collect::<Vec<_>>();
    recipes::panel_stack(sections)
}

/// One proof body without a second caption, divider, or scrolling area.
pub(crate) fn block_view(
    app: &Workspace,
    block: Block,
    ascender: f64,
    descender: f64,
) -> impl WidgetView<Workspace> + use<> {
    let specimen = Specimen {
        blocks: vec![block],
        ascender,
        descender,
    };
    let width = design::DOCK_WIDTH - 2.0 * design::PANEL_SECTION_INSET.px();
    let height = layout(None, &specimen, width, f64::INFINITY);
    let pal = app.palette.clone();
    sized_box(canvas(
        move |_app: &mut Workspace, _ctx, scene, size: Size| {
            let mut painter: Painter<'_> = Painter::new(scene);
            layout(
                Some((&mut painter, pal.proof_ink())),
                &specimen,
                size.width,
                size.height,
            );
        },
    ))
    .dims(Dimensions::new(
        Dim::Stretch,
        Dim::Fixed(Length::px(height)),
    ))
}

fn specimen(app: &Workspace) -> Arc<Specimen> {
    let key: Key = (
        app.font.project.document_revision(),
        app.font.active(),
        app.axis_values
            .iter()
            .map(|value| value.to_bits())
            .collect(),
        app.selected,
    );
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((cached, specimen)) = cache.as_ref()
            && *cached == key
        {
            return specimen.clone();
        }
        let specimen = Arc::new(build(app));
        *cache = Some((key, specimen.clone()));
        specimen
    })
}

/// Which script's proof strings a character takes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Script {
    Latin,
    Figures,
    Arabic,
    Hebrew,
    Other,
}

fn script_of(character: char) -> Script {
    match character {
        '0'..='9' => Script::Figures,
        '\u{0590}'..='\u{05FF}' | '\u{FB1D}'..='\u{FB4F}' => Script::Hebrew,
        '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' | '\u{FB50}'..='\u{FDFF}' => {
            Script::Arabic
        }
        c if c.is_alphabetic() && (c as u32) < 0x0250 => Script::Latin,
        _ => Script::Other,
    }
}

/// Traditional spacing strings for `c`: the character between the script's straight and round
/// control letters.
fn spacing_strings(c: char) -> Vec<String> {
    let between = |control: &str| format!("{control}{control}{c}{control}{control}");
    match script_of(c) {
        Script::Latin | Script::Other => vec![
            between("H"),
            between("O"),
            between("n"),
            between("o"),
            format!("H{c}HO{c}O"),
            format!("n{c}no{c}o"),
        ],
        Script::Figures => vec![
            between("0"),
            between("1"),
            between("H"),
            format!("0{c}01{c}1"),
        ],
        Script::Hebrew => vec![
            between("ה"),
            between("ס"),
            between("ו"),
            format!("ה{c}הס{c}ס"),
        ],
        // The four joining forms, then the letter beside a non-joining and a joining one.
        Script::Arabic => vec![
            format!("{c} {c}\u{0640} \u{0640}{c}\u{0640} \u{0640}{c}"),
            between("ا"),
            between("د"),
            between("ب"),
            format!("ب{c}بب{c}{c}ب"),
        ],
    }
}

/// The string the waterfall sets for `c`.
fn waterfall_string(c: char) -> String {
    match script_of(c) {
        Script::Latin | Script::Other if c.is_uppercase() => format!("H{c}HO{c}O"),
        Script::Latin | Script::Other => format!("n{c}no{c}o"),
        Script::Figures => format!("0{c}01{c}1"),
        Script::Hebrew => format!("ה{c}הס{c}ס"),
        Script::Arabic => format!("ب{c}ب {c}"),
    }
}

/// A script's basic set, one line per entry, with its caption.
fn basic_sets() -> [(Script, &'static str, &'static [&'static str]); 3] {
    [
        (
            Script::Latin,
            "Basic Latin",
            &[
                "ABCDEFGHIJKLM",
                "NOPQRSTUVWXYZ",
                "abcdefghijklm",
                "nopqrstuvwxyz",
                "0123456789",
                "The quick brown fox",
                "jumps over the lazy dog",
            ],
        ),
        (
            Script::Arabic,
            "Basic Arabic",
            &[
                "ا ب ت ث ج ح خ",
                "د ذ ر ز س ش ص",
                "ض ط ظ ع غ ف ق",
                "ك ل م ن ه و ي",
                "أبجد هوز حطي",
                "كلمن سعفص قرشت",
                "ثخذ ضظغ",
            ],
        ),
        (
            Script::Hebrew,
            "Basic Hebrew",
            &["אבגדהוזחטיכ", "לםמןנסעפצקר", "שתךףץ", "אבג דהו זחט"],
        ),
    ]
}

fn build(app: &Workspace) -> Specimen {
    let inputs = text_tool::TextInputs::new(&app.font).with_location(&app.font, &app.axis_values);
    let covered: HashSet<char> = app
        .font
        .glyphs
        .iter()
        .filter_map(|glyph| glyph.codepoint)
        .collect();
    let shape = |text: &str| -> Option<Run> {
        if !text
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '\u{0640}')
            .all(|c| covered.contains(&c))
        {
            return None;
        }
        let placed = text_tool::TextState::new(&inputs.clone().with_text(text)).placed();
        let start = placed
            .iter()
            .map(|sort| sort.origin.x)
            .fold(f64::INFINITY, f64::min);
        let end = placed
            .iter()
            .map(|sort| sort.origin.x + sort.advance)
            .fold(f64::NEG_INFINITY, f64::max);
        if !start.is_finite() || end <= start {
            return None;
        }
        let shift = Affine::translate((-start, 0.0));
        Some(Run {
            paths: placed.into_iter().map(|sort| shift * sort.path).collect(),
            width: end - start,
            rtl: text
                .chars()
                .any(|c| matches!(script_of(c), Script::Arabic | Script::Hebrew)),
        })
    };

    let mut blocks = Vec::new();
    // The selected glyph's character: its own, or its base glyph's for an unencoded form
    // such as `beh-ar.init`, which shaping then produces in context.
    let selected = app.selected.and_then(|index| app.font.glyphs.get(index));
    let character = selected.and_then(|glyph| {
        glyph.codepoint.or_else(|| {
            let base = glyph.name.split('.').next()?;
            app.font
                .glyphs
                .iter()
                .find(|candidate| candidate.name == base)
                .and_then(|candidate| candidate.codepoint)
        })
    });
    let selected_script = character.map(script_of);
    if let (Some(glyph), Some(c)) = (selected, character) {
        let lines: Vec<Run> = spacing_strings(c).iter().filter_map(|s| shape(s)).collect();
        if !lines.is_empty() {
            blocks.push(Block {
                key: "Sidebar Spacing",
                caption: format!("Spacing · {}", glyph.name),
                lines,
                setting: Setting::Even { max_em: 36.0 },
            });
        }
        if let Some(run) = shape(&waterfall_string(c)) {
            blocks.push(Block {
                key: "Sidebar Waterfall",
                caption: "Waterfall".into(),
                lines: vec![run; 6],
                setting: Setting::Waterfall { max_em: 72.0 },
            });
        }
    }
    // The selected glyph's script first, then the others the font covers.
    let mut sets = basic_sets();
    sets.sort_by_key(|(script, _, _)| Some(*script) != selected_script);
    for (_, caption, lines) in sets {
        let lines: Vec<Run> = lines.iter().filter_map(|line| shape(line)).collect();
        if lines.len() >= 2 {
            blocks.push(Block {
                key: caption,
                caption: caption.into(),
                lines,
                setting: Setting::Even { max_em: 24.0 },
            });
        }
    }
    Specimen {
        blocks,
        ascender: app.font.ascender(),
        descender: app.font.descender(),
    }
}

/// The smallest waterfall line, in pixels of em.
const SMALLEST_EM: f64 = 9.0;
/// Each waterfall line is this much of the one above it.
const WATERFALL_STEP: f64 = 0.72;
/// Line height as a share of the em.
const LINE: f64 = 1.15;

/// Pixels per font unit for each line of `block` in a measure of `width`.
fn line_scales(block: &Block, width: f64, em: f64) -> Vec<f64> {
    let widest = block.lines.iter().map(|run| run.width).fold(1.0, f64::max);
    match block.setting {
        Setting::Even { max_em } => {
            let scale = (width / widest).min(max_em / em);
            vec![scale; block.lines.len()]
        }
        Setting::Waterfall { max_em } => {
            let mut scale = (width / widest).min(max_em / em);
            let mut scales = Vec::new();
            for _ in &block.lines {
                if scale * em < SMALLEST_EM {
                    break;
                }
                scales.push(scale);
                scale *= WATERFALL_STEP;
            }
            scales
        }
    }
}

/// Lay the specimen out in a measure `size_width` wide, painting it when given a painter, and
/// return the height it takes. Blocks past `size_height` are left out.
fn layout(
    mut painter: Option<(&mut Painter<'_>, xilem::Color)>,
    specimen: &Specimen,
    size_width: f64,
    size_height: f64,
) -> f64 {
    let margin = 0.0;
    let width = size_width - margin * 2.0;
    if width <= 0.0 {
        return 0.0;
    }
    let em = (specimen.ascender - specimen.descender).max(1.0);
    let mut y = margin;
    for block in &specimen.blocks {
        let scales = line_scales(block, width, em);
        let lines_height: f64 = scales.iter().map(|scale| scale * em * LINE).sum();
        let height = lines_height;
        if scales.is_empty() || y + height > size_height - margin {
            break;
        }
        for (run, scale) in block.lines.iter().zip(&scales) {
            let line = scale * em * LINE;
            if let Some((painter, ink)) = painter.as_mut() {
                // The baseline sits so the em box is centred in the line.
                let baseline = y + (line - scale * em) / 2.0 + specimen.ascender * scale;
                let x = if run.rtl {
                    margin + width - run.width * scale
                } else {
                    margin
                };
                let t = Affine::new([*scale, 0.0, 0.0, -scale, x, baseline]);
                for path in &run.paths {
                    painter.fill(&(t * path.clone()), *ink).draw();
                }
            }
            y += line;
        }
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proofs_fold_independently_and_keep_their_state_when_selection_changes() {
        use crate::application::view::render::panel_resize_tests_support::context;
        use masonry::core::{Widget, WidgetRef};
        use masonry_testing::TestHarness;
        use xilem::core::View;

        fn canvases(widget: WidgetRef<'_, dyn Widget>) -> usize {
            usize::from(widget.short_type_name() == "Canvas")
                + widget.children().into_iter().map(canvases).sum::<usize>()
        }

        let mut app = Workspace::untitled().expect("blank font");
        app.selected = app.font.index_of("A");
        app.set_theme("gray");
        let blocks = specimen(&app);
        assert!(blocks.blocks.len() >= 3);
        assert!(
            blocks
                .blocks
                .iter()
                .all(|block| app.collapsed.contains(block.key))
        );
        let mut ctx = context();
        let mut view = sized_box(glyph_specimen(&app));
        let (pod, mut state) = view.build(&mut ctx, &mut app);
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (254, 1000),
        );
        // Each header has one marker canvas even while its proof body is folded.
        let headers = blocks.blocks.len();
        assert_eq!(canvases(harness.root_widget().as_dyn()), headers);
        for (key, expected) in [
            ("Sidebar Spacing", 1),
            ("Sidebar Waterfall", 2),
            ("Sidebar Spacing", 1),
            ("Basic Latin", 2),
        ] {
            if !app.collapsed.remove(key) {
                app.collapsed.insert(key);
            }
            let next = sized_box(glyph_specimen(&app));
            harness.edit_root_widget(|root| {
                next.rebuild(&view, &mut state, &mut ctx, root, &mut app);
            });
            view = next;
            assert_eq!(canvases(harness.root_widget().as_dyn()), headers + expected);
            let _ = harness.render();
        }
        app.selected = app.font.index_of("B");
        let next = sized_box(glyph_specimen(&app));
        harness.edit_root_widget(|root| {
            next.rebuild(&view, &mut state, &mut ctx, root, &mut app);
        });
        assert_eq!(canvases(harness.root_widget().as_dyn()), headers + 2);
        assert!(app.collapsed.contains("Sidebar Spacing"));
        assert!(!app.collapsed.contains("Sidebar Waterfall"));
    }

    fn run(width: f64) -> Run {
        Run {
            paths: Vec::new(),
            width,
            rtl: false,
        }
    }

    #[test]
    fn even_blocks_fit_the_widest_line_and_cap_the_size() {
        let block = Block {
            key: "test",
            caption: String::new(),
            lines: vec![run(2000.0), run(4000.0)],
            setting: Setting::Even { max_em: 36.0 },
        };
        let scales = line_scales(&block, 100.0, 1000.0);
        assert_eq!(scales, vec![0.025, 0.025]);
        let short = Block {
            lines: vec![run(100.0)],
            ..block
        };
        assert_eq!(line_scales(&short, 200.0, 1000.0), vec![0.036]);
    }

    #[test]
    fn a_waterfall_steps_down_and_stops_at_the_smallest_size() {
        let block = Block {
            key: "test",
            caption: String::new(),
            lines: vec![run(3000.0); 8],
            setting: Setting::Waterfall { max_em: 72.0 },
        };
        let scales = line_scales(&block, 240.0, 1000.0);
        assert_eq!(scales[0], 0.072);
        assert!(scales.windows(2).all(|pair| pair[1] < pair[0]));
        assert!(scales.iter().all(|scale| scale * 1000.0 >= SMALLEST_EM));
        assert!(scales.len() < 8);
    }

    #[test]
    fn spacing_strings_follow_the_script() {
        assert_eq!(spacing_strings('K')[0], "HHKHH");
        assert_eq!(spacing_strings('k')[3], "ookoo");
        assert_eq!(spacing_strings('5')[0], "00500");
        assert!(spacing_strings('ב')[0].contains('ה'));
        assert!(spacing_strings('ب')[0].contains('\u{0640}'));
    }
}
