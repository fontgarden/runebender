// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Helpers the tests share. Compiled only for `cargo test`.
//!
//! `fonts` knows where the fixture fonts are.
//! `ufo_oracle` keeps the historical UFO outline conversions that parity tests compare with. Fixture builders and
//! comparison helpers belong here too, not in the module under test.

pub(crate) mod fonts;
pub(crate) mod ufo_oracle;
