// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Text: shaping through the font's own features, the mark features
//! written from its anchors, the Arabic joining rules, the buffer
//! behind the Text tool, and the samples of the proof view, with the
//! joining context strings of one letter.

pub mod arabic_features;
pub mod buffer;
pub mod contexts;
pub mod features;
pub mod generated;
pub mod joining;
mod joining_data;
pub mod kern_direction;
pub mod layout_tables;
pub mod layout_variation;
pub mod proof_samples;
pub mod shape;
