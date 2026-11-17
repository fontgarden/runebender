// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The tools of the edit canvas, one module per tool.
//!
//! Each module holds the free functions that the widget calls for its tool:
//! `press`, `hover`, `drag`, `release` and `key`, as the tool needs them.
//! The widget dispatches pointer presses by the active `Tool`, and drags and releases by the
//! `Drag` that the press started.
//! Painting stays in `paint.rs` and the overlay files beside it.

pub(super) mod hand;
pub(super) mod label;
pub(super) mod measure;
pub(super) mod metaball;
pub(super) mod pen;
pub(super) mod select;
pub(super) mod shapes;
pub(super) mod sketch;
pub(super) mod text;
