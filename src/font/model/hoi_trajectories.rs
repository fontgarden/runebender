// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Versioned, format-neutral authoring records for locally scoped point trajectories.
//!
//! This experimental Designspace extension is not a cross-editor HOI standard.
//! The owner retains its original plist value; unsupported payloads are never consumed.

use serde::{Deserialize, Serialize};

/// Experimental Designspace-lib key for scoped trajectory authoring records.
pub const HOI_TRAJECTORIES_KEY: &str = "com.runebender.hoiTrajectories";

/// Monotone progression presets; their evaluation belongs to the trajectory evaluator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HoiEasing {
    /// Uniform progression through the interval.
    #[default]
    Linear,
    /// Slow departure: `s = t * t`.
    EaseIn,
    /// Slow arrival: `s = 1 - (1 - t) * (1 - t)`.
    EaseOut,
    /// Slow departure and arrival: `s = t * t * (3 - 2 * t)`.
    EaseInOut,
}

/// Persistent axis coordinates guarding a record against reinterpretation after an axis edit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoiAxis {
    /// Axis tag, independent of session-local axis identity.
    pub tag: String,
    /// Axis name used by source locations.
    pub name: String,
    /// User-space minimum.
    pub minimum: f64,
    /// User-space default.
    pub default: f64,
    /// User-space maximum.
    pub maximum: f64,
    /// Ordered user-to-design mapping pairs.
    pub mapping: Vec<(f64, f64)>,
}

/// Persistent point correspondence within one endpoint source.
///
/// Indices are usable only when the complete outline fingerprint still matches.
/// Source identifiers are retained without inserting identifiers into the glyph file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoiEndpoint {
    /// Source filename as recorded by the Designspace, independent of session identity.
    pub source: String,
    /// UFO layer name, including the default layer when applicable.
    pub layer: String,
    /// Mapped design-axis coordinate at capture time.
    pub location: f64,
    /// Contour index guarded by `outline_sha256`.
    pub contour: usize,
    /// Point index guarded by `outline_sha256`.
    pub point: usize,
    /// Existing source contour identifier, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contour_identifier: Option<String>,
    /// Existing source point identifier, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub point_identifier: Option<String>,
    /// SHA-256 of ordered contour geometry, point roles, smoothness, and source identifiers.
    pub outline_sha256: String,
}

/// One on-curve node's cubic trajectory between two fixed neighboring source drawings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoiTrajectory {
    /// Glyph whose corresponding nodes this record addresses.
    pub glyph: String,
    /// Axis identity and coordinates at capture time.
    pub axis: HoiAxis,
    /// Lower source and node correspondence.
    pub lower: HoiEndpoint,
    /// Upper source and node correspondence.
    pub upper: HoiEndpoint,
    /// Two absolute cubic control positions in font units; endpoints come from the sources.
    pub controls: [[f64; 2]; 2],
    /// Progression along the trajectory, local to this record.
    pub easing: HoiEasing,
}

impl HoiTrajectory {
    /// Whether two records address the same node and interval, ignoring control edits.
    pub fn same_scope(&self, other: &Self) -> bool {
        let endpoint_key = |a: &HoiEndpoint, b: &HoiEndpoint| {
            a.source == b.source
                && a.layer == b.layer
                && a.contour == b.contour
                && a.point == b.point
        };
        self.glyph == other.glyph
            && self.axis.tag == other.axis.tag
            && endpoint_key(&self.lower, &other.lower)
            && endpoint_key(&self.upper, &other.upper)
    }

    /// Check codec invariants without assuming that source drawings are currently available.
    pub fn validate(&self) -> Result<(), String> {
        if self.glyph.is_empty() || !self.controls.iter().flatten().all(|v| v.is_finite()) {
            return Err("HOI needs a glyph name and finite trajectory controls".into());
        }
        super::super::axis::Axis {
            name: self.axis.name.clone(),
            tag: self.axis.tag.clone(),
            min: self.axis.minimum,
            default: self.axis.default,
            max: self.axis.maximum,
            map: self.axis.mapping.clone(),
        }
        .validate()?;
        for endpoint in [&self.lower, &self.upper] {
            if endpoint.source.is_empty()
                || endpoint.layer.is_empty()
                || !endpoint.location.is_finite()
                || endpoint.outline_sha256.len() != 64
                || !endpoint
                    .outline_sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit())
            {
                return Err(
                    "HOI needs a source, layer, finite location, and outline fingerprint".into(),
                );
            }
        }
        if self.lower.location >= self.upper.location {
            return Err("HOI endpoints need an increasing, nonzero design-axis interval".into());
        }
        if (self.lower.contour, self.lower.point) != (self.upper.contour, self.upper.point) {
            return Err("HOI v1 requires corresponding contour and point order".into());
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    version: u32,
    trajectories: Vec<HoiTrajectory>,
}

/// Decode supported records without consuming or changing the source lib.
pub fn read_trajectories(lib: &plist::Dictionary) -> Result<Vec<HoiTrajectory>, String> {
    let Some(value) = lib.get(HOI_TRAJECTORIES_KEY) else {
        return Ok(Vec::new());
    };
    let version = value
        .as_dictionary()
        .and_then(|dict| dict.get("version"))
        .and_then(plist::Value::as_unsigned_integer);
    if version != Some(1) {
        return Err("unsupported HOI metadata version; original payload retained".into());
    }
    let payload: Payload = plist::from_value(value)
        .map_err(|error| format!("unsupported or malformed HOI metadata; retained: {error}"))?;
    validate_records(&payload.trajectories)?;
    Ok(payload.trajectories)
}

fn validate_records(records: &[HoiTrajectory]) -> Result<(), String> {
    for (index, record) in records.iter().enumerate() {
        record.validate()?;
        if records[..index].iter().any(|old| old.same_scope(record)) {
            return Err("duplicate HOI trajectory for one node and interval".into());
        }
    }
    Ok(())
}

/// Replace supported authoring records atomically, retaining every unrelated lib entry.
///
/// Unknown or malformed existing payloads cannot be overwritten through this API.
/// Removing the final supported record removes the extension key.
pub fn write_trajectories(
    lib: &mut plist::Dictionary,
    records: &[HoiTrajectory],
) -> Result<bool, String> {
    let current = read_trajectories(lib)?;
    validate_records(records)?;
    if current == records {
        return Ok(false);
    }
    if records.is_empty() {
        return Ok(lib.remove(HOI_TRAJECTORIES_KEY).is_some());
    }
    let value = plist::to_value(&Payload {
        version: 1,
        trajectories: records.to_vec(),
    })
    .map_err(|error| format!("encode HOI metadata: {error}"))?;
    lib.insert(HOI_TRAJECTORIES_KEY.into(), value);
    Ok(true)
}
