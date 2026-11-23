// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Color the live Bézier fill with local stroke measurements.
//! Native windows measure one snapshot at a time and wake only when its brush is ready.

use super::*;
use runebender::analysis::thickness::{ThicknessField, sampled_glyph_thickness};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use xilem::core::{MessageProxy, ViewPathTracker as _};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Slot {
    Editable(String, usize),
    Sort(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quality {
    Preview,
    Refined,
}

type HeatBrush = (ImageData, Affine);

#[derive(Default)]
struct Entry {
    visible: bool,
    result: Option<(u64, Quality, Option<HeatBrush>)>,
}

#[derive(Debug)]
pub(super) struct Completed {
    slot: Slot,
    key: u64,
    quality: Quality,
    cancel: Arc<AtomicBool>,
    brush: Option<HeatBrush>,
}

struct Work {
    result: Completed,
    outline: kurbo::BezPath,
    upm: f64,
    reference: Option<f64>,
    ramp: [[u8; 4]; 256],
}

impl Work {
    fn measure(mut self) -> Completed {
        let density = match self.result.quality {
            Quality::Preview => 128,
            Quality::Refined => 512,
        };
        if let Some(mut field) = sampled_glyph_thickness(&self.outline, self.upm, density, &|| {
            self.result.cancel.load(Ordering::Relaxed)
        }) {
            if let Some(reference) = self.reference {
                field.reference = reference;
            }
            self.result.brush = Some(color_brush(&field, &self.ramp));
        }
        self.result
    }
}

/// Extend the nearest ink color through counters and past the edge.
/// The renderer takes coverage from the outline; a bilinear footprint crossing
/// the boundary must still sample ink color on both sides.
fn color_brush(field: &ThicknessField, ramp: &[[u8; 4]; 256]) -> HeatBrush {
    let width = field.width;
    let mut distance = vec![u32::MAX; field.thickness.len()];
    let mut pixels = vec![ramp[128]; distance.len()];
    for (i, &value) in field.thickness.iter().enumerate() {
        if value > 0.0 {
            distance[i] = 0;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the normalized ramp is clamped to the lookup table"
            )]
            let shade = (field.ramp(f64::from(value)) * 255.0).round() as usize;
            pixels[i] = ramp[shade];
        }
    }
    // Two Manhattan-distance sweeps propagate colors in linear time.
    let mut extend = |to: usize, from: usize| {
        let step = distance[from].saturating_add(1);
        if step < distance[to] {
            distance[to] = step;
            pixels[to] = pixels[from];
        }
    };
    for i in 0..field.thickness.len() {
        if i % width != 0 {
            extend(i, i - 1);
        }
        if i >= width {
            extend(i, i - width);
        }
    }
    for i in (0..field.thickness.len()).rev() {
        if i % width + 1 < width {
            extend(i, i + 1);
        }
        if i + width < field.thickness.len() {
            extend(i, i + width);
        }
    }
    let image = ImageData {
        data: Blob::new(Arc::new(pixels.into_flattened())),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: u32::try_from(width).expect("measurement dimensions fit u32"),
        height: u32::try_from(field.height).expect("measurement dimensions fit u32"),
    };
    (image, field.pixel_to_design())
}

#[derive(Default)]
pub(super) struct HeatCache {
    entries: HashMap<Slot, Entry>,
    pending: Option<(Slot, u64, Quality, Arc<AtomicBool>)>,
    proxy: Option<MessageProxy<Completed>>,
    worker: Option<mpsc::Sender<Work>>,
}

impl HeatCache {
    pub(super) fn for_view(ctx: &mut ViewCtx, synchronous: bool) -> Self {
        let mut cache = Self::default();
        if !cfg!(target_arch = "wasm32") && !synchronous && !cfg!(test) {
            cache.proxy = Some(MessageProxy::new(ctx.proxy(), ctx.view_path().into()));
        }
        cache
    }

    pub(super) fn begin_frame(&mut self) {
        self.entries
            .retain(|_, entry| std::mem::take(&mut entry.visible));
    }

    pub(super) fn accept(&mut self, result: Completed) -> bool {
        let Some((_, _, _, token)) = &self.pending else {
            return false;
        };
        if !Arc::ptr_eq(token, &result.cancel) {
            return false;
        }
        self.pending = None;
        if !result.cancel.load(Ordering::Relaxed)
            && let Some(entry) = self.entries.get_mut(&result.slot)
        {
            entry.result = Some((result.key, result.quality, result.brush));
        }
        true
    }

    fn brush(
        &mut self,
        slot: Slot,
        key: u64,
        outline: &kurbo::BezPath,
        pal: &Palette,
        upm: f64,
        reference: Option<f64>,
        dragging: bool,
    ) -> Option<HeatBrush> {
        let entry = self.entries.entry(slot.clone()).or_default();
        entry.visible = true;
        let current = entry.result.as_ref().filter(|(k, _, _)| *k == key);
        let previous = entry
            .result
            .as_ref()
            .and_then(|(_, _, brush)| brush.clone());
        if current.is_some_and(|(_, quality, _)| *quality == Quality::Refined || dragging) {
            return previous;
        }
        if let Some((work_slot, work_key, quality, cancel)) = &self.pending {
            // Let short previews finish so there is fresh color even during continuous input.
            if *quality == Quality::Refined
                && ((*work_slot == slot && *work_key != key)
                    || (dragging && matches!(slot, Slot::Editable(..)) && *work_slot != slot))
            {
                cancel.store(true, Ordering::Relaxed);
            }
            return previous;
        }

        if dragging && matches!(slot, Slot::Sort(_)) {
            return previous;
        }
        let preview = matches!(slot, Slot::Editable(..))
            && (dragging || (self.proxy.is_some() && current.is_none()));
        let quality = if preview {
            Quality::Preview
        } else {
            Quality::Refined
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.pending = Some((slot.clone(), key, quality, cancel.clone()));
        let work = Work {
            result: Completed {
                slot: slot.clone(),
                key,
                quality,
                cancel,
                brush: None,
            },
            outline: outline.clone(),
            upm,
            reference,
            ramp: std::array::from_fn(|i| {
                let color = pal.thickness_fill(i as f64 / 255.0).to_rgba8();
                [color.r, color.g, color.b, color.a]
            }),
        };
        if let Some(proxy) = &self.proxy {
            let worker = self.worker.get_or_insert_with(|| {
                let (tx, rx) = mpsc::channel::<Work>();
                let proxy = proxy.clone();
                std::thread::spawn(move || {
                    for work in rx {
                        if proxy.message(work.measure()).is_err() {
                            break;
                        }
                    }
                });
                tx
            });
            if worker.send(work).is_err() {
                self.worker = None;
                self.pending = None;
            }
            previous
        } else {
            let result = work.measure();
            let brush = result.brush.clone();
            self.accept(result);
            brush
        }
    }
}

impl Drop for HeatCache {
    fn drop(&mut self) {
        if let Some((_, _, _, cancel)) = &self.pending {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl EditorWidget {
    pub(super) fn paint_heat(
        &self,
        painter: &mut Painter<'_>,
        outline: &kurbo::BezPath,
        affine: Affine,
        editable: bool,
    ) {
        let upm = self.session.metrics.upm;
        let key = thickness_key(outline, &self.palette, upm);
        let slot = if editable {
            Slot::Editable(self.session.glyph_name.clone(), self.sketch_source)
        } else {
            Slot::Sort(key)
        };
        let dragging = matches!(
            self.drag,
            Drag::Points { .. } | Drag::Pen { dragging: true, .. } | Drag::Metaballs { .. }
        );
        if let Some((image, placement)) = self.heat.borrow_mut().brush(
            slot,
            key,
            outline,
            &self.palette,
            upm,
            self.view.thickness_reference,
            dragging,
        ) {
            let brush = masonry::peniko::ImageBrush::new(image);
            painter
                .fill(outline, &brush)
                .transform(affine)
                .brush_transform(Some(placement))
                .draw();
            if editable && !self.view.grid_lines && grid_alphas(self.session.viewport.zoom).0 > 0.0
            {
                painter.push_fill_clip_transformed(outline, affine);
                self.paint_design_grid(
                    painter,
                    &self.palette,
                    affine,
                    f64::from(crate::application::design::Alpha::Underlay.value()),
                );
                painter.pop_clip();
            }
        }
    }
}
/// Cache key for the thickness image: the outline's geometry and the ramp's colors.
fn thickness_key(outline: &kurbo::BezPath, pal: &Palette, upm: f64) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    upm.to_bits().hash(&mut hasher);
    for element in outline.elements() {
        std::mem::discriminant(element).hash(&mut hasher);
        let points: &[Point] = match element {
            kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => std::slice::from_ref(p),
            kurbo::PathEl::QuadTo(a, b) => &[*a, *b],
            kurbo::PathEl::CurveTo(a, b, c) => &[*a, *b, *c],
            kurbo::PathEl::ClosePath => &[],
        };
        for p in points {
            p.x.to_bits().hash(&mut hasher);
            p.y.to_bits().hash(&mut hasher);
        }
    }
    for t in [0.0, 0.5, 1.0] {
        for component in pal.thickness_fill(t).components {
            component.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use xilem::core::{ProxyError, RawProxy, SendMessage, ViewId};

    #[derive(Debug)]
    struct Deliver(mpsc::Sender<Completed>);

    impl RawProxy for Deliver {
        fn send_message(&self, _: Arc<[ViewId]>, message: SendMessage) -> Result<(), ProxyError> {
            self.0
                .send(*message.downcast::<Completed>().unwrap())
                .unwrap();
            Ok(())
        }
        fn dyn_debug(&self) -> &dyn std::fmt::Debug {
            self
        }
    }

    #[test]
    fn edits_coalesce_while_color_is_measured_then_refine_on_release() {
        let (tx, rx) = mpsc::channel();
        let mut cache = HeatCache::default();
        cache.proxy = Some(MessageProxy::new(Arc::new(Deliver(tx)), Arc::from([])));
        let slot = Slot::Editable("stem".into(), 0);
        let pal = Palette::load("gray");
        let stem = |x| Rect::new(0.0, 0.0, x, 400.0).to_path(0.1);
        let paint = |cache: &mut HeatCache, width: u32, drag| {
            let path = stem(f64::from(width));
            cache.begin_frame();
            cache.brush(
                slot.clone(),
                thickness_key(&path, &pal, 1024.0),
                &path,
                &pal,
                1024.0,
                None,
                drag,
            )
        };
        let receive = || rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(paint(&mut cache, 64, true).is_none());
        for width in 65..100 {
            assert!(paint(&mut cache, width, true).is_none());
        }
        let first = receive();
        assert_eq!(first.quality, Quality::Preview);
        cache.accept(first);
        assert!(paint(&mut cache, 100, true).is_some());
        let latest = receive();
        assert_eq!(latest.key, thickness_key(&stem(100.0), &pal, 1024.0));
        cache.accept(latest);
        assert!(paint(&mut cache, 100, false).is_some());
        // New geometry supersedes refinement as soon as the next drag starts.
        paint(&mut cache, 108, true);
        let cancelled = receive();
        assert!(cancelled.cancel.load(Ordering::Relaxed));
        cache.accept(cancelled);
        paint(&mut cache, 108, true);
        cache.accept(receive());
        paint(&mut cache, 108, false);
        let refined = receive();
        assert_eq!(refined.quality, Quality::Refined);
        let (image, _) = refined.brush.as_ref().unwrap();
        assert!(image.data.data().chunks_exact(4).all(|p| p[3] == 255));
        cache.accept(refined);
        for _ in 0..5 {
            assert!(paint(&mut cache, 108, false).is_some());
            assert!(
                cache.pending.is_none(),
                "hover and zoom reuse the measured brush"
            );
        }
    }
}
