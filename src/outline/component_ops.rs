// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Components inside a canonical layer: resolving them to rendered paths and to contours for
//! decomposition.

use crate::outline::glyph_paths;

/// Canonical geometry resolved for one stable top-level component.
#[derive(Clone, Debug)]
pub struct ResolvedDocumentComponent {
    /// Stable identity of the top-level component.
    pub id: crate::font::ComponentId,
    /// Exact rendered path used for hit testing and selection feedback.
    ///
    /// This includes canonical smart-component interpolation, hyperbeziers and metaballs.
    pub path: kurbo::BezPath,
    /// Recursively resolved and rounded structural contours used by canonical decomposition.
    ///
    /// This intentionally preserves the existing decomposition contract rather than flattening
    /// rendered smart-component or metaball outlines.
    pub contours: Vec<crate::font::CopiedContour>,
}

fn collect_document_component_contours<'a>(
    layer: crate::font::LayerView<'a>,
    transform: kurbo::Affine,
    resolve: &mut impl FnMut(&str) -> Option<crate::font::LayerView<'a>>,
    stack: &mut Vec<String>,
    output: &mut Vec<crate::font::CopiedContour>,
) -> Result<(), glyph_paths::ComponentResolveError> {
    for shape in layer.shapes() {
        match shape {
            crate::font::LayerShapeView::Contour(contour) => {
                output.push(
                    contour
                        .copied()
                        .transformed_rounded(transform)
                        .ok_or(glyph_paths::ComponentResolveError::NonFinite)?,
                );
            }
            crate::font::LayerShapeView::Component(component) => {
                let name = component.reference();
                let base = resolve(name);
                glyph_paths::enter_component(stack, name, base, Ok(()), |base, stack| {
                    collect_document_component_contours(
                        base,
                        transform * component.transform(),
                        resolve,
                        stack,
                        output,
                    )
                })?;
            }
        }
    }
    Ok(())
}

/// Resolve every canonical component into transformed contour copies.
///
/// Nested components use the caller's layer resolver. Geometry is rounded to integer font units,
/// matching the existing decomposition command, while contour and point source metadata remains
/// attached for the canonical paste boundary to re-identify safely.
pub fn resolved_document_component_contours<'a>(
    layer: crate::font::LayerView<'a>,
    mut resolve: impl FnMut(&str) -> Option<crate::font::LayerView<'a>>,
) -> Result<Vec<crate::font::CopiedContour>, glyph_paths::ComponentResolveError> {
    let mut output = Vec::new();
    let mut stack = vec![layer.glyph_name().to_owned()];
    for component in layer.components() {
        let name = component.reference();
        let base = resolve(name);
        glyph_paths::enter_component(&mut stack, name, base, Ok(()), |base, stack| {
            collect_document_component_contours(
                base,
                component.transform(),
                &mut resolve,
                stack,
                &mut output,
            )
        })?;
    }
    Ok(output)
}

/// Resolve each top-level canonical component while retaining its stable identity.
///
/// The rendered path supports hit testing and selection feedback, including special outlines.
/// The contour copies separately retain the existing integer-rounded structural decomposition
/// contract and canonical source metadata.
pub fn resolved_document_components<'a>(
    layer: crate::font::LayerView<'a>,
    mut resolve: impl FnMut(&str) -> Option<crate::font::LayerView<'a>>,
    mut layers: impl FnMut(&str) -> Vec<crate::font::LayerView<'a>>,
) -> Result<Vec<ResolvedDocumentComponent>, glyph_paths::ComponentResolveError> {
    let mut output = Vec::new();
    for component in layer.components() {
        let path = glyph_paths::canonical_component_to_bezpath(
            layer,
            component,
            &mut resolve,
            &mut layers,
        )?;
        let name = component.reference();
        let base = resolve(name);
        let mut contours = Vec::new();
        let mut stack = vec![layer.glyph_name().to_owned()];
        glyph_paths::enter_component(&mut stack, name, base, Ok(()), |base, stack| {
            collect_document_component_contours(
                base,
                component.transform(),
                &mut resolve,
                stack,
                &mut contours,
            )
        })?;
        output.push(ResolvedDocumentComponent {
            id: component.id(),
            path,
            contours,
        });
    }
    Ok(output)
}

#[cfg(test)]
mod canonical_tests {
    use super::*;
    use crate::font::model::glyph_metadata::{Metaball, MetaballGroup, Metaballs};
    use crate::font::project::Project;
    use crate::font::source::SourceInput;
    use crate::font::variable::{GlyphLayerAddress, SourceId};
    use crate::formats::metadata::metaballs::write_metaballs;
    use norad::{AffineTransform, Component, Font, Glyph, Name};

    #[test]
    fn rendered_component_path_keeps_structural_decomposition_separate() {
        let mut blob = Glyph::new("blob");
        write_metaballs(
            &mut blob,
            &Metaballs {
                version: 2,
                groups: vec![MetaballGroup {
                    id: 1,
                    threshold: 0.5,
                    balls: vec![Metaball {
                        id: 1,
                        x: 40.0,
                        y: 55.0,
                        radius: 45.0,
                        reach: 2.0_f64.sqrt(),
                        weight: 0.5,
                    }],
                }],
            },
        )
        .unwrap();
        let mut user = Glyph::new("blob.user");
        user.components.push(Component::new(
            Name::new("blob").unwrap(),
            AffineTransform {
                x_scale: 1.25,
                xy_scale: 0.125,
                yx_scale: -0.25,
                y_scale: 0.875,
                x_offset: 37.0,
                y_offset: -19.0,
            },
            None,
        ));
        let mut font = Font::default();
        font.default_layer_mut().insert_glyph(blob);
        font.default_layer_mut().insert_glyph(user);
        let project =
            Project::from_source(SourceInput::from_font(font, "MetaballComponent.ufo".into()))
                .expect("valid source fixture");
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let root = project.document_layer("blob.user", &layer).unwrap();

        let resolved = resolved_document_components(
            root,
            |name| project.document_layer(name, &layer),
            |name| {
                project
                    .document_glyph(name)
                    .map(|glyph| {
                        glyph
                            .layer_ids()
                            .filter(|candidate| candidate.source == layer.source)
                            .filter_map(|candidate| glyph.layer(candidate))
                            .collect()
                    })
                    .unwrap_or_default()
            },
        )
        .unwrap();
        let expected = project
            .document_layer_path(&GlyphLayerAddress {
                glyph: "blob.user".into(),
                layer,
            })
            .unwrap();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].path, expected);
        assert!(!resolved[0].path.is_empty());
        assert!(
            resolved[0].contours.is_empty(),
            "metaball rendering must not silently change structural decomposition"
        );
    }
}
