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
use masonry::kurbo::{Affine, BezPath, Shape, Size};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::sized_box;

use crate::application::design::recipes::{Group, Row};
use crate::application::design::{self, recipes};
use crate::application::editor::tools::text as text_tool;
use crate::application::widgets::keyed_canvas::keyed_canvas;
use crate::application::workspace::Workspace;

/// One shaped line in font units, baseline at zero, starting at x = 0.
#[derive(Clone, Debug, PartialEq)]
struct Run {
    paths: Vec<BezPath>,
    width: f64,
    rtl: bool,
}

/// How a block sizes its lines.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Setting {
    /// Every line at one size: the largest at which the widest line fits, up to `max_em`
    /// pixels of em.
    Even { max_em: f64 },
    /// The first line as large as fits, up to `max_em`, and each next line smaller.
    Waterfall { max_em: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Block {
    key: &'static str,
    caption: String,
    lines: Vec<Run>,
    setting: Setting,
}

#[derive(Debug, Default)]
struct Specimen {
    blocks: Vec<Block>,
    extra_spacing: Vec<Run>,
    ascender: f64,
    descender: f64,
}

/// The document state a specimen was shaped from.
type Key = (u64, usize, Vec<u64>, Option<usize>, [bool; 4]);

/// One specimen for each master of a large family.
const CACHE_LIMIT: usize = 16;

thread_local! {
    /// The specimens of the current document revision, newest last. A switch between
    /// masters returns to a location shaped before, and shaping every block again takes
    /// several milliseconds.
    static CACHE: RefCell<Vec<(Key, Arc<Specimen>)>> = const { RefCell::new(Vec::new()) };
}

/// The independently folding proofs below Categories, Global Scripts, and Filters.
pub(crate) fn glyph_specimen(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let specimen = specimen(app);
    let sections = specimen
        .blocks
        .iter()
        .map(|block| {
            let groups = vec![Group::new(
                "",
                vec![Row::Specimen {
                    block: block.clone(),
                    ascender: specimen.ascender,
                    descender: specimen.descender,
                }],
            )];
            if block.key == "Sidebar Spacing" && !app.collapsed.contains(block.key) {
                return sized_box(recipes::section(
                    app,
                    block.key,
                    &block.caption,
                    recipes::panel_stack(()),
                ))
                .padding(masonry::properties::Padding {
                    top: design::PANEL_SECTION_INSET.length(),
                    bottom: Length::px(0.0),
                    left: design::PANEL_SECTION_INSET.length(),
                    right: design::PANEL_SECTION_INSET.length(),
                })
                .boxed();
            }
            recipes::section_of(app, block.key, &block.caption, groups).boxed()
        })
        .collect::<Vec<_>>();
    recipes::panel_stack(sections)
}

/// One flexible canvas owns every spacing baseline, including the required examples.
pub(crate) fn spacing_canvas(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    if app.collapsed.contains("Sidebar Spacing") {
        return None;
    }
    let proof = specimen(app);
    let base = proof
        .blocks
        .last()
        .filter(|block| block.key == "Sidebar Spacing")?;
    let inset = design::PANEL_SECTION_INSET.px();
    let em = (proof.ascender - proof.descender).max(1.0);
    let natural = line_scales(base, design::DOCK_WIDTH - 2.0 * inset, em)[0]
        * em
        * LINE
        * base.lines.len() as f64;
    let ink = app.palette.proof_ink();
    Some(
        // The cached specimen is one allocation until the document or the master changes.
        keyed_canvas(
            (Arc::as_ptr(&proof) as usize, ink),
            move |_, scene, size: Size| {
                let base = proof.blocks.last().unwrap();
                let width = size.width - 2.0 * inset;
                if width <= 0.0 || size.height <= 0.0 {
                    return;
                }
                let nominal = line_scales(base, width, em)[0];
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a rounded, non-negative count of lines that fit the panel"
                )]
                let count =
                    ((size.height / (nominal * em * LINE)).round() as usize).max(base.lines.len());
                let runs: Vec<_> = base
                    .lines
                    .iter()
                    .chain(proof.extra_spacing.iter().cycle())
                    .take(count)
                    .collect();
                let scale = runs
                    .iter()
                    .fold(nominal, |s, run| s.min(width / run.width))
                    .min(size.height / (runs.len() as f64 * em));
                let last_bottom = runs
                    .last()
                    .unwrap()
                    .paths
                    .iter()
                    .filter(|p| !p.elements().is_empty())
                    .map(|p| p.bounding_box().y0)
                    .reduce(f64::min)
                    .unwrap_or(0.0);
                let first = scale * proof.ascender;
                let last = size.height + last_bottom * scale;
                let leading = (last - first) / runs.len().saturating_sub(1).max(1) as f64;
                let mut painter = Painter::new(scene);
                for (i, run) in runs.iter().enumerate() {
                    let baseline = if runs.len() == 1 {
                        last
                    } else {
                        first + i as f64 * leading
                    };
                    let transform = Affine::new([scale, 0.0, 0.0, -scale, inset, baseline]);
                    for path in &run.paths {
                        painter.fill(path, ink).transform(transform).draw();
                    }
                }
            },
        )
        .dims(Dimensions::new(
            Dim::Stretch,
            Dim::Fixed(Length::px(natural)),
        )),
    )
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
        extra_spacing: Vec::new(),
        ascender,
        descender,
    };
    let width = design::DOCK_WIDTH - 2.0 * design::PANEL_SECTION_INSET.px();
    let height = layout(None, &specimen, width, f64::INFINITY);
    let pal = app.palette.clone();
    let key = (
        specimen.blocks.clone(),
        ascender,
        descender,
        pal.proof_ink(),
    );
    sized_box(keyed_canvas(key, move |_ctx, scene, size: Size| {
        let mut painter: Painter<'_> = Painter::new(scene);
        layout(
            Some((&mut painter, pal.proof_ink())),
            &specimen,
            size.width,
            size.height,
        );
    }))
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
        // A folded block is listed without its lines, so opening it builds again.
        [
            "Sidebar Waterfall",
            "Basic Latin",
            "Basic Arabic",
            "Basic Hebrew",
        ]
        .map(|key| app.collapsed.contains(key)),
    );
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        // An edit makes every stored specimen stale.
        cache.retain(|(cached, _)| cached.0 == key.0);
        if let Some((_, specimen)) = cache.iter().find(|(cached, _)| *cached == key) {
            return specimen.clone();
        }
        let specimen = Arc::new(build(app));
        if cache.len() >= CACHE_LIMIT {
            cache.remove(0);
        }
        cache.push((key, specimen.clone()));
        specimen
    })
}

/// Shape each pair as continuous text, then fill its visual right edge.
fn spacing_rows(c: char, em: f64, shape: impl Fn(&str) -> Option<Run>) -> Vec<Run> {
    let samples = spacing_strings(c);
    let mut rows = samples
        .chunks(2)
        .filter_map(|pair| {
            let text = pair.concat();
            shape(&text).map(|run| (text, run))
        })
        .collect::<Vec<_>>();
    let measure = design::DOCK_WIDTH - 2.0 * design::PANEL_SECTION_INSET.px();
    let limit = rows
        .iter()
        .map(|(_, run)| run.width)
        .fold(measure * em / 36.0, f64::max);
    for (text, run) in &mut rows {
        while run.width < limit {
            if run.rtl {
                text.insert(0, c);
            } else {
                text.push(c);
            }
            let Some(next) = shape(text) else {
                break;
            };
            if next.width <= run.width || next.width > limit {
                break;
            }
            *run = next;
        }
        // The shaped paths already encode writing direction. Align the row to the left inset.
        run.rtl = false;
    }
    rows.into_iter().map(|(_, run)| run).collect()
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
    let is_covered = |text: &str| {
        text.chars()
            .filter(|c| !c.is_whitespace() && *c != '\u{0640}')
            .all(|c| covered.contains(&c))
    };
    let shape = |text: &str| -> Option<Run> {
        if !is_covered(text) {
            return None;
        }
        shape_line(&inputs, text)
    };
    // A folded block shows only its header. It is listed when the font covers its text,
    // and its lines are shaped when it opens: shaping every block takes several
    // milliseconds on each new master or selection.
    let folded = |key: &str| app.collapsed.contains(key);

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
    let waterfall = character.map(waterfall_string).and_then(|text| {
        if folded("Sidebar Waterfall") {
            is_covered(&text).then(Vec::new)
        } else {
            shape(&text).map(|run| vec![run; 6])
        }
    });
    if let Some(lines) = waterfall {
        blocks.push(Block {
            key: "Sidebar Waterfall",
            caption: "Waterfall".into(),
            lines,
            setting: Setting::Waterfall { max_em: 72.0 },
        });
    }
    // The selected glyph's script first, then the others the font covers.
    let mut sets = basic_sets();
    sets.sort_by_key(|(script, _, _)| Some(*script) != selected_script);
    for (_, caption, lines) in sets {
        let (count, lines) = if folded(caption) {
            (
                lines.iter().filter(|line| is_covered(line)).count(),
                Vec::new(),
            )
        } else {
            let lines: Vec<Run> = lines.iter().filter_map(|line| shape(line)).collect();
            (lines.len(), lines)
        };
        if count >= 2 {
            blocks.push(Block {
                key: caption,
                caption: caption.into(),
                lines,
                setting: Setting::Even { max_em: 24.0 },
            });
        }
    }
    if let (Some(glyph), Some(c)) = (selected, character) {
        let lines = spacing_rows(
            c,
            (app.font.ascender() - app.font.descender()).max(1.0),
            shape,
        );
        if !lines.is_empty() {
            blocks.push(Block {
                key: "Sidebar Spacing",
                caption: format!("Spacing · {}", glyph.name),
                lines,
                setting: Setting::Even { max_em: 36.0 },
            });
        }
    }
    let mut extra_spacing = Vec::new();
    if let Some(c) = character
        && blocks.last().is_some_and(|b| b.key == "Sidebar Spacing")
    {
        let controls = match script_of(c) {
            Script::Arabic => "بتمهلويد",
            Script::Hebrew => "הסתנכלרב",
            Script::Figures => "0123456789",
            _ => "HOnovaxTVe",
        };
        for (a, b) in controls.chars().zip(controls.chars().cycle().skip(1)) {
            if let Some(run) = shape(&format!("{a}{c}{b}{c}{c}{a}{c}{b}{a}{c}")) {
                extra_spacing.push(run);
            }
        }
    }
    Specimen {
        extra_spacing,
        blocks,
        ascender: app.font.ascender(),
        descender: app.font.descender(),
    }
}

/// Shape one line of `text`, moved to start at x = 0.
fn shape_line(inputs: &text_tool::TextInputs, text: &str) -> Option<Run> {
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
        paths: placed
            .into_iter()
            .map(|sort| {
                (shift * Affine::translate(sort.origin.to_vec2())) * (*sort.outline).clone()
            })
            .collect(),
        width: end - start,
        rtl: text
            .chars()
            .any(|c| matches!(script_of(c), Script::Arabic | Script::Hebrew)),
    })
}

/// The most lines the live proof sets; later words are left out.
const LIVE_MAX_LINES: usize = 8;
/// The live proof's em size range and default, in pixels.
pub(crate) const LIVE_EM_RANGE: (f64, f64) = (16.0, 120.0);
/// The size a proof starts at.
pub(crate) const LIVE_EM_DEFAULT: f64 = 40.0;

/// `text` shaped from the live outline of the open glyph, for the Proof section of the editor
/// panel, set at `size` pixels of em and wrapped at spaces to the panel's width.
/// The shaping options of the Shaping section apply. `None` when nothing has ink or advance.
pub(crate) fn live_block(app: &Workspace, text: &str, size: f64) -> Option<(Block, f64, f64)> {
    let inputs = text_tool::TextInputs::new(&app.font)
        .with_location(&app.font, &app.axis_values)
        .with_shaping_options(
            &app.text_features_disabled,
            app.text_script.as_deref(),
            app.text_language.as_deref(),
        );
    // Off a master, the interpolated font is what the designer reads.
    let inputs = if app.on_active_master() {
        let mut live = app.session.outline();
        live.extend(app.session.components.clone());
        inputs.with_live_outline(&app.session.glyph_name, Arc::new(live))
    } else {
        inputs
    };
    let ascender = app.font.ascender();
    let descender = app.font.descender();
    let em = (ascender - descender).max(1.0);
    let measure = design::DOCK_WIDTH - 2.0 * design::PANEL_SECTION_INSET.px();
    // Words fill each line at `size` pixels of em; a word wider than the panel has its own
    // line and makes every line smaller.
    let fits = |run: &Run| run.width * size / em <= measure;
    let mut lines: Vec<Run> = Vec::new();
    let mut line = String::new();
    let mut shaped: Option<Run> = None;
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        let run = shape_line(&inputs, &candidate);
        if line.is_empty() || run.as_ref().is_some_and(&fits) {
            (line, shaped) = (candidate, run);
        } else {
            lines.extend(shaped.take());
            shaped = shape_line(&inputs, word);
            line = word.to_owned();
        }
        if lines.len() >= LIVE_MAX_LINES {
            break;
        }
    }
    if lines.len() < LIVE_MAX_LINES {
        lines.extend(shaped);
    }
    if lines.is_empty() {
        return None;
    }
    Some((
        Block {
            key: "Proof",
            caption: String::new(),
            lines,
            setting: Setting::Even { max_em: size },
        },
        ascender,
        descender,
    ))
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
                    painter.fill(path, *ink).transform(t).draw();
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
    fn spacing_uses_the_extra_height_of_the_glyph_sidebar() {
        use crate::application::view::panels::tabs::sidebar;
        use crate::application::view::render::panel_resize_tests_support::context;
        use masonry::core::{Widget, WidgetRef};
        use masonry_testing::TestHarness;
        use xilem::core::View;

        fn last_canvas_height(widget: WidgetRef<'_, dyn Widget>) -> Option<(f64, f64)> {
            if widget.short_type_name() == "Canvas" {
                let ctx = widget.ctx();
                let height = ctx.border_box().height();
                return Some((height, ctx.to_window((0.0, height).into()).y));
            }
            widget
                .children()
                .into_iter()
                .filter_map(last_canvas_height)
                .last()
        }

        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/font-sources/variable-fonts/VirtuaGrotesk/VirtuaGrotesk-Regular.ufo");
        let mut app = Workspace::open(&fixture).unwrap();
        app.selected = app.font.index_of("A");
        let height = |app: &mut Workspace, window_height| {
            let (pod, _) = sized_box(sidebar(app)).build(&mut context(), app);
            let mut harness = TestHarness::create_with_size(
                crate::application::view::default_property_set(),
                pod.new_widget,
                (254, window_height),
            );
            let _ = harness.render();
            let (height, bottom) = last_canvas_height(harness.root_widget().as_dyn()).unwrap();
            assert_eq!(
                f64::from(window_height) - bottom,
                design::PANEL_SECTION_INSET.px()
            );
            height
        };
        let short = height(&mut app, 1000);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a rounded test width in logical pixels, below 1000"
        )]
        let required = 1000 - short.round() as u32;
        for slack in [160, 180, 200, 220] {
            assert!((height(&mut app, required + slack) - f64::from(slack)).abs() < 1.0);
        }
        let tall = height(&mut app, 1200);
        assert!((tall - short - 200.0).abs() < 1.0, "{short} -> {tall}");
    }

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
        assert!(!app.collapsed.contains("Sidebar Spacing"));
        app.collapsed.insert("Sidebar Spacing");
        let blocks = specimen(&app);
        assert!(blocks.blocks.len() >= 3);
        assert_eq!(blocks.blocks.last().unwrap().key, "Sidebar Spacing");
        assert!(
            blocks
                .blocks
                .iter()
                .all(|block| app.collapsed.contains(block.key))
        );
        let mut ctx = context();
        let proof_view = |app: &Workspace| {
            sized_box(recipes::panel_stack((
                glyph_specimen(app),
                spacing_canvas(app),
            )))
        };
        let mut view = proof_view(&app);
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
            let next = proof_view(&app);
            harness.edit_root_widget(|root| {
                next.rebuild(&view, &mut state, &mut ctx, root, &mut app);
            });
            view = next;
            assert_eq!(canvases(harness.root_widget().as_dyn()), headers + expected);
            let _ = harness.render();
        }
        app.selected = app.font.index_of("B");
        let next = proof_view(&app);
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
    fn the_panel_proof_follows_the_live_outline() {
        use crate::application::view::render::panel_resize_tests_support::workspace_for_layout;
        use masonry::kurbo::Shape as _;
        let mut app = workspace_for_layout();
        let top = |app: &Workspace| {
            let (block, _, _) = live_block(app, "A A", LIVE_EM_DEFAULT).expect("A has ink");
            block.lines[0]
                .paths
                .iter()
                .map(|path| path.bounding_box().y1)
                .fold(f64::MIN, f64::max)
        };
        let before = top(&app);
        let mut session = (*app.session).clone();
        session.select_all();
        assert!(session.nudge(0.0, 40.0));
        app.session = Arc::new(session);
        assert!((top(&app) - before - 40.0).abs() < 1e-6);
    }

    #[test]
    fn the_panel_proof_wraps_into_more_lines_as_its_size_grows() {
        use crate::application::view::render::panel_resize_tests_support::workspace_for_layout;
        let app = workspace_for_layout();
        let lines = |size: f64| {
            live_block(&app, "A A A A A A A A A A A A", size)
                .expect("A has ink")
                .0
                .lines
                .len()
        };
        let (small, large) = (lines(LIVE_EM_RANGE.0), lines(LIVE_EM_RANGE.1));
        assert!(
            small >= 1 && large > small,
            "{small} lines small, {large} lines large"
        );
        assert!(large <= LIVE_MAX_LINES);
        // One word wider than the panel still sets, on a line of its own.
        assert_eq!(lines(4000.0), LIVE_MAX_LINES);
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
