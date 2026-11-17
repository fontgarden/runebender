// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Everything that changes a shape.
//!
//! Point and segment edits, the knife, primitives, effects,
//! emboldening, and canonical-to-kurbo path builders.
//! The segment maths (cubic, quadratic, hyperbezier) is in `path`.

pub mod component_ops;
pub mod drawing;
pub mod effects;
pub mod embolden;
#[cfg(test)]
mod glyph_ops_tests;
pub mod glyph_paths;
pub mod knife;
pub mod label_pieces;
pub mod path;
pub mod piece_assembly;
pub mod point_ops;
pub mod segment_ops;

pub mod metaballs;
