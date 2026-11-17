// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UFO `public.markColor` serialization helpers.
//!
//! The typed value lives with canonical glyph metadata.
//! The palette itself lives in the shared theme (`ui::theme`), which owns label reading,
//! hue-snapping and writing.

pub use crate::font::model::glyph_metadata::{MARK_COLOR_KEY, MarkColor};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ufo_rgba_with_whitespace() {
        assert_eq!(
            MarkColor::parse(" 1, 0.3, 0.3, 1 "),
            Some(MarkColor {
                red: 1.0,
                green: 0.3,
                blue: 0.3,
                alpha: 1.0,
            })
        );
    }

    #[test]
    fn rejects_invalid_ufo_rgba() {
        assert_eq!(MarkColor::parse("1,0.3,1"), None);
        assert_eq!(MarkColor::parse("1,0.3,0.3,2"), None);
        assert_eq!(MarkColor::parse("1,0.3,0.3,nan"), None);
    }
}
