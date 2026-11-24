// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Variable values of a feature file, resolved at one designspace location.
//!
//! A feature file of a variable font can give a value for each master, for example
//! `<anchor (wght=400:360 wght=700:400) 384>`.
//! A font compiler turns such a value into a default and deltas.
//! The layout tables of the text line are static tables for the displayed location, so
//! [`LocationVariationInfo`] gives `fea-rs` the axes of the font, which makes the syntax valid,
//! and returns each value interpolated at that location, with no deltas.
//! The interpolation is the variation model that fontc uses for the same values.

use std::collections::{HashMap, HashSet};

use fea_rs::compile::VariationInfo;
use fontdrasil::{
    coords::NormalizedLocation,
    types::Axis,
    variations::{DeltaError, VariationModel},
};
use write_fonts::{OtRound, tables::variations::VariationRegion, types::Tag};

/// The axes of a font and the location at which variable feature values are read.
#[derive(Debug, Clone)]
pub struct LocationVariationInfo {
    axes: Vec<Axis>,
    location: NormalizedLocation,
}

/// Why a variable value could not be resolved.
#[derive(Debug)]
pub struct VariableValueError(String);

impl std::fmt::Display for VariableValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for VariableValueError {}

impl From<DeltaError> for VariableValueError {
    fn from(error: DeltaError) -> Self {
        Self(error.to_string())
    }
}

impl LocationVariationInfo {
    /// Resolve variable values at `location`, in a font with `axes` in their font order.
    ///
    /// An axis that `location` does not name is read at its default.
    pub fn new(axes: Vec<Axis>, location: NormalizedLocation) -> Self {
        Self { axes, location }
    }

    /// The same axes at the default location.
    pub fn at_default(axes: Vec<Axis>) -> Self {
        Self::new(axes, NormalizedLocation::new())
    }

    /// Whether the font has no axes, so the feature file can have no variable values.
    pub fn is_static(&self) -> bool {
        self.axes.is_empty()
    }
}

impl VariationInfo for LocationVariationInfo {
    type Error = VariableValueError;

    fn axis_count(&self) -> u16 {
        u16::try_from(self.axes.len()).unwrap_or(u16::MAX)
    }

    fn axis(&self, axis_tag: Tag) -> Option<(usize, &Axis)> {
        self.axes
            .iter()
            .enumerate()
            .find(|(_, axis)| axis.tag == axis_tag)
    }

    fn resolve_variable_metric(
        &self,
        locations: &HashMap<NormalizedLocation, i16>,
    ) -> Result<(i16, Vec<(VariationRegion, i16)>), Self::Error> {
        if locations.is_empty() {
            return Err(VariableValueError(
                "a variable value has no locations".into(),
            ));
        }
        // A model of the locations this value names, so a value given at some of the
        // masters is read as fontc reads it.
        let model = VariationModel::new(
            locations.keys().cloned().collect::<HashSet<_>>(),
            self.axes.iter().map(|axis| axis.tag).collect(),
        );
        let points: HashMap<_, _> = locations
            .iter()
            .map(|(at, value)| (at.clone(), vec![f64::from(*value)]))
            .collect();
        let value: f64 = model
            .deltas::<f64, f64>(&points)?
            .into_iter()
            .map(|(region, deltas)| {
                deltas.first().copied().unwrap_or(0.0)
                    * region.scalar_at(&self.location).into_inner()
            })
            .sum();
        // A static table has the value itself and no deltas.
        Ok((value.ot_round(), Vec::new()))
    }

    fn resolve_glyphs_number_value(
        &self,
        name: &str,
    ) -> Result<HashMap<NormalizedLocation, f64>, Self::Error> {
        Err(VariableValueError(format!(
            "the number value ${name} is not defined in this source"
        )))
    }
}

#[cfg(test)]
mod tests {
    use fontdrasil::coords::NormalizedCoord;

    use super::*;

    fn at(weight: f64) -> NormalizedLocation {
        let mut location = NormalizedLocation::new();
        location.insert(Tag::new(b"wght"), NormalizedCoord::new(weight));
        location
    }

    fn info(weight: f64) -> LocationVariationInfo {
        LocationVariationInfo::new(vec![Axis::for_test("wght")], at(weight))
    }

    #[test]
    fn a_variable_value_is_read_at_the_location_with_no_deltas() {
        let values = HashMap::from([(at(0.0), 360), (at(1.0), 400)]);
        for (weight, expected) in [(0.0, 360), (0.5, 380), (1.0, 400)] {
            let (value, deltas) = info(weight).resolve_variable_metric(&values).unwrap();
            assert_eq!(value, expected, "at {weight}");
            assert!(deltas.is_empty());
        }
    }

    #[test]
    fn the_axes_of_the_font_are_known_by_tag() {
        let info = info(0.0);
        assert_eq!(info.axis_count(), 1);
        assert!(info.axis(Tag::new(b"wght")).is_some());
        assert!(info.axis(Tag::new(b"wdth")).is_none());
        assert!(!info.is_static());
    }
}
