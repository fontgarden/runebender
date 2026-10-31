// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Screen-space trajectory picking and source-safe canvas gestures.

use super::*;
use crate::application::design::{Alpha, Space};
use crate::application::editor::tools::interpolation::InterpolationEvent;
use kurbo::{CubicBez, ParamCurveNearest};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Hit {
    index: usize,
    control: Option<usize>,
    t: f64,
}

pub(super) struct InterpolationDrag {
    start: Point,
    hit: Hit,
    controls: Vec<(usize, [[f64; 2]; 2])>,
}

fn cubic(points: [Point; 4]) -> CubicBez {
    CubicBez::new(points[0], points[1], points[2], points[3])
}

impl EditorWidget {
    pub(super) fn fit_interpolation(&mut self) -> bool {
        let Some(data) = &self.hoi else {
            return false;
        };
        let bounds = data.reference_bounds();
        let padding = Space::Xl.px() * 4.0;
        let zoom = ((self.size.width - padding) / bounds.width().max(1.0))
            .min((self.size.height - padding) / bounds.height().max(1.0))
            .clamp(0.02, 8.0);
        self.session.viewport.zoom = zoom;
        let center = bounds.center();
        self.session.viewport.offset = kurbo::Vec2::new(
            self.size.width / 2.0 - center.x * zoom,
            self.size.height / 2.0 + center.y * zoom,
        );
        self.session.fitted = true;
        true
    }

    pub(super) fn paint_interpolation(&self, painter: &mut Painter<'_>) {
        let Some(data) = &self.hoi else {
            return;
        };
        let pal = &self.palette;
        let affine = self.session.viewport.affine();
        if data.preview_only {
            painter
                .fill(&(affine * &*data.preview), pal.editor_ink())
                .draw();
            if let Some(compiled) = &data.compiled {
                painter
                    .stroke(
                        &(affine * &**compiled),
                        &Stroke::new(pal.stroke),
                        pal.role("warning"),
                    )
                    .draw();
            }
            return;
        }
        for endpoint in &data.endpoints {
            painter
                .stroke(
                    &(affine * &**endpoint),
                    &Stroke::new(pal.stroke),
                    pal.role("reference"),
                )
                .draw();
        }
        painter
            .fill(
                &(affine * &*data.preview),
                pal.editor_ink().with_alpha(Alpha::Underlay.value()),
            )
            .draw();
        painter
            .stroke(
                &(affine * &*data.preview),
                &Stroke::new(pal.stroke_emphasis),
                pal.role("pathStroke"),
            )
            .draw();
        if let Some(compiled) = &data.compiled {
            painter
                .stroke(
                    &(affine * &**compiled),
                    &Stroke::new(pal.stroke),
                    pal.role("warning"),
                )
                .draw();
        }
        for (index, connection) in data.connections.iter().enumerate() {
            let points = connection.points.map(|point| affine * point);
            let selected = data.selected.contains(&index);
            let color = if selected {
                pal.role("pointSelected")
            } else {
                pal.text_muted
            };
            painter
                .stroke(
                    cubic(points),
                    &Stroke::new(if selected {
                        pal.stroke_emphasis
                    } else {
                        pal.stroke
                    }),
                    color,
                )
                .draw();
            for endpoint in [points[0], points[3]] {
                painter
                    .fill(Circle::new(endpoint, POINT_CURVE_RADIUS), pal.canvas)
                    .draw();
                painter
                    .stroke(
                        Circle::new(endpoint, POINT_CURVE_RADIUS),
                        &Stroke::new(pal.stroke),
                        pal.role("reference"),
                    )
                    .draw();
            }
            if selected {
                for (start, control) in [(points[0], points[1]), (points[3], points[2])] {
                    painter
                        .stroke(
                            Line::new(start, control),
                            &Stroke::new(pal.stroke),
                            pal.handle_line,
                        )
                        .draw();
                    let marker = Rect::from_center_size(
                        control,
                        Size::new(POINT_CORNER_RADIUS * 2.0, POINT_CORNER_RADIUS * 2.0),
                    );
                    painter.fill(marker, pal.canvas).draw();
                    painter
                        .stroke(
                            marker,
                            &Stroke::new(pal.stroke_emphasis),
                            pal.role("pointOffcurve"),
                        )
                        .draw();
                }
            }
        }
    }

    pub(super) fn interpolation_hit(&self, at: Point) -> Option<Hit> {
        let data = self.hoi.as_ref()?;
        if data.preview_only {
            return None;
        }
        let affine = self.session.viewport.affine();
        // Selected controls take precedence over the trajectory underneath them.
        for &index in &data.selected {
            for control in 0..2 {
                if (affine * data.connections[index].points[control + 1]).distance(at)
                    <= HIT_RADIUS_PX
                {
                    return Some(Hit {
                        index,
                        control: Some(control),
                        t: 0.5,
                    });
                }
            }
        }
        data.connections
            .iter()
            .enumerate()
            .filter_map(|(index, connection)| {
                let nearest = cubic(connection.points.map(|point| affine * point)).nearest(at, 0.1);
                (nearest.distance_sq <= HIT_RADIUS_PX * HIT_RADIUS_PX).then_some((
                    nearest.distance_sq,
                    Hit {
                        index,
                        control: None,
                        t: nearest.t,
                    },
                ))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, hit)| hit)
    }

    fn emit_interpolation(&self, ctx: &mut EventCtx<'_>, event: InterpolationEvent) {
        ctx.submit_action::<EditorEvent>(EditorEvent::Interpolation(event));
        ctx.request_render();
        ctx.set_handled();
    }

    pub(super) fn interpolation_pointer(
        &mut self,
        ctx: &mut EventCtx<'_>,
        event: &PointerEvent,
    ) -> bool {
        if self.hoi.as_ref().is_some_and(|data| data.preview_only) {
            if matches!(
                event,
                PointerEvent::Down(PointerButtonEvent {
                    button: Some(PointerButton::Primary | PointerButton::Secondary),
                    ..
                })
            ) {
                ctx.request_focus();
                ctx.set_handled();
                return true;
            }
            return false;
        }
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                ctx.request_focus();
                let at = ctx.local_position(state.position);
                let Some(data) = &self.hoi else {
                    ctx.set_handled();
                    return true;
                };
                let mut selected = data.selected.clone();
                if let Some(hit) = self.interpolation_hit(at) {
                    if state.modifiers.shift()
                        && selected.contains(&hit.index)
                        && hit.control.is_none()
                    {
                        selected.remove(&hit.index);
                        self.emit_interpolation(
                            ctx,
                            InterpolationEvent::Select(selected.into_iter().collect()),
                        );
                        return true;
                    }
                    if !selected.contains(&hit.index) {
                        if !state.modifiers.shift() {
                            selected.clear();
                        }
                        selected.insert(hit.index);
                    }
                    let controls = selected
                        .iter()
                        .map(|&index| (index, data.connections[index].record.controls))
                        .collect();
                    self.hoi_drag = Some(InterpolationDrag {
                        start: self.session.viewport.affine().inverse() * at,
                        hit,
                        controls,
                    });
                    self.hoi_control = hit.control;
                    ctx.capture_pointer();
                    self.emit_interpolation(
                        ctx,
                        InterpolationEvent::Begin(selected.into_iter().collect()),
                    );
                } else {
                    self.hoi_control = None;
                    if !state.modifiers.shift() {
                        self.emit_interpolation(ctx, InterpolationEvent::Select(Vec::new()));
                    }
                    ctx.set_handled();
                }
                true
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Secondary),
                ..
            }) => {
                ctx.set_handled();
                true
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                if let Some(drag) = &self.hoi_drag {
                    let at = self.session.viewport.affine().inverse()
                        * ctx.local_position(current.position);
                    let mut delta = at - drag.start;
                    if current.modifiers.shift() {
                        if delta.x.abs() > delta.y.abs() {
                            delta.y = 0.0;
                        } else {
                            delta.x = 0.0;
                        }
                    }
                    // Translating both controls moves the picked path point by 3t(1-t) times that delta.
                    if drag.hit.control.is_none() {
                        let t = if !(0.05..=0.95).contains(&drag.hit.t) {
                            0.5
                        } else {
                            drag.hit.t.clamp(0.15, 0.85)
                        };
                        delta /= 3.0 * t * (1.0 - t);
                    }
                    let updates = drag
                        .controls
                        .iter()
                        .map(|&(index, mut controls)| {
                            for (i, control) in controls.iter_mut().enumerate() {
                                if drag.hit.control.is_none_or(|picked| picked == i) {
                                    control[0] += delta.x;
                                    control[1] += delta.y;
                                }
                            }
                            (index, controls)
                        })
                        .collect();
                    self.emit_interpolation(ctx, InterpolationEvent::Controls(updates));
                    true
                } else {
                    false
                }
            }
            PointerEvent::Up(_) | PointerEvent::Cancel(_) if self.hoi_drag.is_some() => {
                self.hoi_drag = None;
                self.emit_interpolation(
                    ctx,
                    if matches!(event, PointerEvent::Cancel(_)) {
                        InterpolationEvent::Cancel
                    } else {
                        InterpolationEvent::Finish
                    },
                );
                true
            }
            _ => false,
        }
    }

    pub(super) fn interpolation_key(&mut self, ctx: &mut EventCtx<'_>, event: &TextEvent) -> bool {
        let TextEvent::Keyboard(key) = event else {
            return false;
        };
        if key.state != KeyState::Down {
            return false;
        }
        let command = key.modifiers.meta() || key.modifiers.ctrl();
        if command {
            match &key.key {
                Key::Character(letter) if letter.eq_ignore_ascii_case("a") => {
                    self.emit_interpolation(ctx, InterpolationEvent::SelectAll);
                    return true;
                }
                Key::Character(letter)
                    if letter.eq_ignore_ascii_case("z") || letter.eq_ignore_ascii_case("y") =>
                {
                    if self.hoi_drag.take().is_some() {
                        self.emit_interpolation(ctx, InterpolationEvent::Cancel);
                    } else {
                        ctx.submit_action::<EditorEvent>(
                            if key.modifiers.shift() || letter.eq_ignore_ascii_case("y") {
                                EditorEvent::Redo
                            } else {
                                EditorEvent::Undo
                            },
                        );
                        ctx.set_handled();
                    }
                    return true;
                }
                _ => return false,
            }
        }
        if self.hoi.as_ref().is_some_and(|data| data.preview_only) {
            return false;
        }
        let amount = if key.modifiers.shift() { 10.0 } else { 1.0 };
        let delta = match key.key {
            Key::Named(NamedKey::ArrowLeft) => Some(kurbo::Vec2::new(-amount, 0.0)),
            Key::Named(NamedKey::ArrowRight) => Some(kurbo::Vec2::new(amount, 0.0)),
            Key::Named(NamedKey::ArrowUp) => Some(kurbo::Vec2::new(0.0, amount)),
            Key::Named(NamedKey::ArrowDown) => Some(kurbo::Vec2::new(0.0, -amount)),
            Key::Named(NamedKey::Escape) => {
                self.hoi_drag = None;
                self.emit_interpolation(ctx, InterpolationEvent::Cancel);
                return true;
            }
            Key::Named(NamedKey::Backspace | NamedKey::Delete) => {
                self.hoi_drag = None;
                self.emit_interpolation(ctx, InterpolationEvent::Reset);
                return true;
            }
            _ => None,
        };
        if let Some(delta) = delta {
            self.emit_interpolation(
                ctx,
                InterpolationEvent::Nudge {
                    delta,
                    control: self.hoi_control,
                },
            );
            return true;
        }
        false
    }
}
