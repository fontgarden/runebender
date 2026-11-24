// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Everything that reads a font to answer a question.
//!
//! Measurement, curvature, and Unicode categories.
//! Nothing here changes a font.

pub mod category;
pub mod curve;
pub mod dimensions;
pub mod glyph;
pub mod measure;
pub mod report;
pub mod thickness;
pub mod weight_debt;
