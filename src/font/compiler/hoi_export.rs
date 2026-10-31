// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Disposable glyph-local HOI samples and directly evaluated static compiler snapshots.

use std::collections::BTreeSet;

use babelfont::LayerType;
use fontdrasil::coords::{DesignCoord, DesignLocation};

use super::{CompiledFont, metadata};
use crate::font::hoi::curvature_bound;
use crate::font::project::Project;
use crate::font::var_model::{Location, VariationModel, normalize_value};
use crate::font::variable::GlyphLayerAddress;
use crate::font::variable::source_builder::interpolated_source_layer;

/// Limits for the first bounded HOI variable-font sampler.
#[derive(Clone, Copy, Debug)]
pub struct HoiBakeOptions {
    /// Maximum positional error of linear source interpolation, in font units.
    /// This excludes compiler quantization, curve conversion, and axis quantization.
    pub source_tolerance: f64,
    /// Maximum generated glyph layers across the font, at most 4096.
    pub max_generated_layers: u32,
}

impl Default for HoiBakeOptions {
    fn default() -> Self {
        Self {
            source_tolerance: 0.25,
            max_generated_layers: 4096,
        }
    }
}

/// Evidence about disposable sources; not a compiled-outline accuracy certificate.
#[derive(Clone, Debug, Default)]
pub struct HoiBakeReport {
    /// Number of generated glyph-local layers; full-font masters are unchanged.
    pub generated_layers: u32,
    /// Largest analytical positional-error bound before compiler conversion.
    pub source_error_bound: f64,
}

impl Project {
    /// Build a variable compiler snapshot with a bounded, disposable HOI bake.
    ///
    /// Authoring records and source drawings are never modified by this operation.
    pub fn babelfont_snapshot_with_hoi(
        &self,
        options: HoiBakeOptions,
    ) -> Result<(babelfont::Font, HoiBakeReport), String> {
        let mut font = self.source_snapshot_with_features(None)?;
        let report = self.bake_hoi_snapshot(&mut font, options)?;
        Ok((font, report))
    }

    pub(super) fn bake_hoi_snapshot(
        &self,
        font: &mut babelfont::Font,
        options: HoiBakeOptions,
    ) -> Result<HoiBakeReport, String> {
        if !options.source_tolerance.is_finite()
            || options.source_tolerance <= 0.0
            || options.max_generated_layers > 4096
        {
            return Err(
                "HOI bake needs a positive finite tolerance and a layer limit at most 4096".into(),
            );
        }
        self.require_evaluable_hoi(None)?;
        let records = self.hoi_trajectories()?;
        let mut report = HoiBakeReport::default();
        for (index, record) in records.iter().enumerate() {
            let same_interval = |other: &&crate::font::model::hoi_trajectories::HoiTrajectory| {
                other.glyph == record.glyph
                    && other.lower.location == record.lower.location
                    && other.upper.location == record.upper.location
            };
            if records[..index].iter().any(|other| same_interval(&other)) {
                continue;
            }
            let bound = records
                .iter()
                .filter(same_interval)
                .map(|other| {
                    let bound = curvature_bound(self.hoi_curve_points(other)?, other.easing);
                    if !bound.is_finite() {
                        return Err("HOI curvature exceeds the finite sampling range".to_owned());
                    }
                    Ok(bound)
                })
                .collect::<Result<Vec<_>, String>>()?
                .into_iter()
                .fold(0.0, f64::max);
            if !bound.is_finite() {
                return Err("HOI curvature exceeds the finite sampling range".into());
            }
            let mut segments = 1_u32;
            while bound / (8.0 * f64::from(segments).powi(2)) > options.source_tolerance {
                segments = segments.checked_mul(2).ok_or("HOI sample limit exceeded")?;
                if segments - 1
                    > options
                        .max_generated_layers
                        .saturating_sub(report.generated_layers)
                {
                    return Err(format!(
                        "HOI sample limit exceeded for {}; increase tolerance",
                        record.glyph
                    ));
                }
            }
            report.source_error_bound = report
                .source_error_bound
                .max(bound / (8.0 * f64::from(segments).powi(2)));
            let ds = self.document_designspace().expect("HOI has a Designspace");
            let axis = &ds.axes()[0]; // Binding already established a single explicit axis.
            let tag = babelfont::Tag::new(
                record
                    .axis
                    .tag
                    .as_bytes()
                    .try_into()
                    .map_err(|_| "invalid HOI axis tag")?,
            );
            for step in 1..segments {
                let design = record.lower.location
                    + (record.upper.location - record.lower.location) * f64::from(step)
                        / f64::from(segments);
                let normalized = normalize_value(
                    design,
                    axis.design_minimum(),
                    axis.design_default(),
                    axis.design_maximum(),
                );
                let location = Location::from([(axis.coordinates.name.clone(), normalized)]);
                let mut layer = self.evaluated_backend_layer(&record.glyph, &location)?;
                let glyph = font
                    .glyphs
                    .0
                    .iter_mut()
                    .find(|glyph| glyph.name == record.glyph)
                    .ok_or("HOI compiler glyph is missing")?;
                let mut suffix = report.generated_layers;
                let id = loop {
                    let candidate = format!("runebender-hoi-sample-{suffix}");
                    if !glyph
                        .layers
                        .iter()
                        .any(|layer| layer.id.as_ref() == Some(&candidate))
                    {
                        break candidate;
                    }
                    suffix += 1;
                };
                layer.id = Some(id.clone());
                layer.name = Some(id);
                layer.master = LayerType::FreeFloating;
                layer.location = Some([(tag, DesignCoord::new(design))].into_iter().collect());
                glyph.layers.push(layer);
                report.generated_layers += 1;
            }
        }
        Ok(report)
    }

    fn evaluated_backend_layer(
        &self,
        glyph: &str,
        location: &Location,
    ) -> Result<babelfont::Layer, String> {
        let output = self.try_interpolated_layer_at(glyph, location)?;
        let source = self
            .glyph_sources(glyph)?
            .into_iter()
            .find(|source| source.location.values().all(|value| value.abs() < 1e-9))
            .ok_or("evaluated glyph has no default source")?;
        let address = GlyphLayerAddress {
            glyph: glyph.to_owned(),
            layer: source.layer,
        };
        let base = self
            .capture_document_layer(&address)
            .ok_or("evaluated default layer is missing")?;
        let snapshot = interpolated_source_layer(base, &output, address)?;
        Ok(snapshot.into_parts().0)
    }

    /// Build a single-master snapshot using the exact shared evaluator at a normalized location.
    ///
    /// Metrics and kerning interpolate with the full sources. This first API keeps default
    /// naming and rejects Designspace rules and cross-axis mappings rather than dropping them.
    /// It does not use the variable-font sampler.
    pub fn static_babelfont_snapshot(
        &self,
        location: &Location,
    ) -> Result<babelfont::Font, String> {
        self.require_evaluable_hoi(None)?;
        let mut font = self.source_snapshot_with_features(None)?;
        if self
            .document_designspace()
            .is_some_and(|ds| !ds.rules().is_empty())
            || !font.cross_axis_mappings.is_empty()
        {
            return Err(
                "static snapshots do not yet flatten Designspace rules or cross-axis mappings"
                    .into(),
            );
        }
        let axes = self
            .document_designspace()
            .map(|ds| ds.axes())
            .unwrap_or_default();
        if location.iter().any(|(name, value)| {
            !value.is_finite()
                || !(-1.0..=1.0).contains(value)
                || !axes.iter().any(|axis| &axis.coordinates.name == name)
        }) {
            return Err(
                "static snapshot needs known axes and finite normalized coordinates in -1..1"
                    .into(),
            );
        }
        let locations = if let Some(ds) = self.document_designspace() {
            ds.sources()
                .iter()
                .map(|source| source.location.to_normalized(ds.axes()))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![Location::new()]
        };
        let model = VariationModel::new(&locations)?;
        let default = locations
            .iter()
            .position(|loc| loc.values().all(|value| value.abs() < 1e-9))
            .ok_or("static snapshot has no default source")?;
        let mut master = font
            .masters
            .get(default)
            .ok_or("static default master is missing")?
            .clone();
        if font
            .masters
            .iter()
            .any(|source| source.custom_ot_values != master.custom_ot_values)
        {
            return Err(
                "static snapshots cannot interpolate differing custom OpenType values".into(),
            );
        }
        // Missing source metrics have compiler-specific fallbacks; never invent those here.
        if font.masters.iter().any(|source| {
            source.metrics.len() != master.metrics.len()
                || source
                    .metrics
                    .keys()
                    .any(|key| !master.metrics.contains_key(key))
        }) {
            return Err("static snapshots need the same explicit metrics in every source".into());
        }
        for (key, value) in &mut master.metrics {
            let values = font
                .masters
                .iter()
                .map(|source| vec![f64::from(source.metrics[key])])
                .collect::<Vec<_>>();
            *value = metadata::metric(
                &format!("{key:?}"),
                model.interpolate(&values, location)?[0],
            )?;
        }
        let pairs: BTreeSet<_> = font
            .masters
            .iter()
            .flat_map(|source| source.kerning.keys().cloned())
            .collect();
        master.kerning.clear();
        for pair in pairs {
            let values = font
                .masters
                .iter()
                .map(|source| vec![f64::from(source.kerning.get(&pair).copied().unwrap_or(0))])
                .collect::<Vec<_>>();
            master.kerning.insert(
                pair,
                metadata::kerning(model.interpolate(&values, location)?[0])?,
            );
        }
        master.location = DesignLocation::default();
        for glyph in font.glyphs.0.iter_mut().filter(|glyph| glyph.exported) {
            let mut layer = self.evaluated_backend_layer(&glyph.name, location)?;
            layer.location = None;
            layer.master = LayerType::DefaultForMaster(master.id.clone());
            glyph.layers = vec![layer];
        }
        font.masters = vec![master];
        font.axes.clear();
        font.instances.clear();
        Ok(font)
    }

    /// Compile a static instance directly from exact evaluated outlines, then convert to OpenType.
    pub fn compile_static(&self, location: &Location) -> Result<CompiledFont, String> {
        CompiledFont::build(self.static_babelfont_snapshot(location)?)
    }
}
