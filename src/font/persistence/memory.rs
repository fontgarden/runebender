// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Explicit in-memory UFO decoding boundary.
//!
//! Hosts with no filesystem, such as the web builds, receive font
//! data as `(path, bytes)` pairs over fetch or embedded in the
//! binary. This module assembles a font from those pairs.
//!
//! [`decode_ufo_files`] and [`ufo_files_from_font`] handle a complete UFO 3 package for hosts
//! that read and write a folder themselves, such as the browser with the File System Access API;
//! [`open_memory_project`] and [`memory_save_files`] do the same for a whole project.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use norad::designspace::DesignSpaceDocument;
use norad::{Font, Glyph};
use serde::Deserialize;

/// Parse a designspace document from XML text.
pub fn designspace_from_str(xml: &str) -> Result<DesignSpaceDocument, String> {
    crate::formats::designspace::parse(xml)
}

#[derive(Deserialize)]
struct EmbeddedGlifFont {
    info: EmbeddedGlifFontInfo,
    glyphs: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmbeddedGlifFontInfo {
    family_name: String,
    style_name: String,
    units_per_em: f64,
    ascender: f64,
    descender: f64,
    cap_height: f64,
    x_height: f64,
}

/// Decode an embedded font-info plus raw-GLIF JSON bundle into canonical Project ownership.
///
/// This is the browser demo's compact transport format, not a persisted font format.
pub fn project_from_embedded_glif_json(
    source_path: PathBuf,
    json: &str,
) -> Result<crate::font::project::Project, String> {
    let embedded: EmbeddedGlifFont =
        serde_json::from_str(json).map_err(|error| format!("embedded font JSON: {error}"))?;
    let mut font = Font::new();
    font.font_info.family_name = Some(embedded.info.family_name);
    font.font_info.style_name = Some(embedded.info.style_name);
    font.font_info.units_per_em = Some(
        embedded
            .info
            .units_per_em
            .try_into()
            .map_err(|error| format!("embedded font unitsPerEm: {error}"))?,
    );
    font.font_info.ascender = Some(embedded.info.ascender);
    font.font_info.descender = Some(embedded.info.descender);
    font.font_info.cap_height = Some(embedded.info.cap_height);
    font.font_info.x_height = Some(embedded.info.x_height);

    let mut names = HashSet::new();
    let layer = font.default_layer_mut();
    for (index, raw_glif) in embedded.glyphs.into_iter().enumerate() {
        let glyph = Glyph::parse_raw(raw_glif.as_bytes())
            .map_err(|error| format!("embedded glyph {index}: {error}"))?;
        let name = glyph.name().as_str();
        crate::font::canonical_metadata::validate_name(name).map_err(|error| error.to_string())?;
        if !names.insert(name.to_owned()) {
            return Err(format!("duplicate embedded glyph name {name:?}"));
        }
        if glyph.image.is_some() {
            return Err(format!(
                "embedded glyph {name:?}: images are not supported in memory"
            ));
        }
        layer.insert_glyph(glyph);
    }
    crate::font::project::Project::from_ufo_boundary(source_path, &font)
}

fn validate_file_inventory<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<HashMap<&'a str, &'a [u8]>, String> {
    let mut validated = HashMap::new();
    for (path, bytes) in files {
        if !safe_relative_path(path) {
            return Err(format!("unsafe UFO path {path:?}"));
        }
        if validated.insert(path, bytes).is_some() {
            return Err(format!("duplicate UFO path {path:?}"));
        }
    }
    Ok(validated)
}

fn safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn is_supported_root_file(path: &str) -> bool {
    matches!(
        path,
        "metainfo.plist"
            | "fontinfo.plist"
            | "lib.plist"
            | "groups.plist"
            | "kerning.plist"
            | "features.fea"
            | "layercontents.plist"
    )
}

/// Serialize one glyph to glif XML bytes, for hosts saving over
/// HTTP instead of a filesystem.
pub fn glif_bytes(glyph: &Glyph) -> Result<Vec<u8>, String> {
    glyph.encode_xml().map_err(|e| format!("encode glif: {e}"))
}

/// A complete UFO decoded from memory: every layer, image and data file.
///
/// This boundary accepts a whole UFO 3 package.
/// Files it does not interpret are kept unchanged so a host can write them back.
#[derive(Debug)]
pub struct DecodedUfo {
    /// The assembled font, with every layer loaded.
    pub font: Font,
    /// Layer name to the directory it was read from, where norad chose another name.
    pub layer_directories: HashMap<String, String>,
    /// (layer name, glyph name) to the GLIF file name the package used.
    pub glif_paths: HashMap<(String, String), String>,
    /// Files outside the UFO specification, kept unchanged for the host to write back.
    pub preserved: Vec<(String, Vec<u8>)>,
}

/// The UFO files a host writes, relative to the UFO root.
#[derive(Debug, Default)]
pub struct EncodedUfo {
    /// Every file of the package, as (path, bytes) pairs.
    pub files: Vec<(String, Vec<u8>)>,
    /// Glyph directories whose previously owned GLIFs can be removed when absent from `files`.
    pub layer_directories: Vec<String>,
}

#[derive(Debug)]
struct OriginalMetadataFile {
    original: Option<Vec<u8>>,
    canonical: Option<Vec<u8>>,
}

/// What a host keeps beside a project opened from memory, so it can write the project back.
#[derive(Debug, Default)]
pub struct MemoryBookkeeping {
    /// Per source path, the layer directories norad renamed on import.
    pub layer_directories: HashMap<PathBuf, HashMap<String, String>>,
    /// Per source path, the GLIF file name each (layer, glyph) pair used.
    pub glif_paths: HashMap<PathBuf, HashMap<(String, String), String>>,
    /// Per source path, files outside the UFO specification, written back unchanged.
    pub preserved: HashMap<PathBuf, Vec<(String, Vec<u8>)>>,
    original_glifs: HashMap<PathBuf, HashMap<String, super::OriginalGlif>>,
    // Only differences from the opening Project's metadata serialization need storage.
    original_metadata: HashMap<PathBuf, BTreeMap<String, OriginalMetadataFile>>,
    // Files beside the managed sources travel with copies, but in-place saves leave them alone.
    project_files: Vec<(String, Vec<u8>)>,
    /// Opening XML and its canonical value, to retain comments and formatting in a copy.
    original_designspace: Option<(
        crate::font::model::designspace::CanonicalDesignspace,
        Vec<u8>,
    )>,
}

/// A project opened from in-memory files plus the host's save bookkeeping.
#[derive(Debug)]
pub struct MemoryProject {
    /// The canonical project; its source paths are relative to the opened folder.
    pub project: crate::font::project::Project,
    /// What [`memory_save_files`] needs to write the project back exactly.
    pub bookkeeping: MemoryBookkeeping,
}

/// The files a host writes to persist a project, relative to the folder it was opened from.
#[derive(Debug, Default)]
pub struct MemorySave {
    /// Every file to write, as (path, bytes) pairs.
    pub files: Vec<(String, Vec<u8>)>,
    /// Glyph directories whose previously owned GLIFs can be removed when absent from `files`.
    pub glif_directories: Vec<String>,
}

#[derive(Deserialize)]
struct LayerInfo {
    color: Option<norad::Color>,
    #[serde(default)]
    lib: plist::Dictionary,
}

fn read_root_files(files: &HashMap<&str, &[u8]>, font: &mut Font) -> Result<(), String> {
    if let Some(bytes) = files.get("metainfo.plist") {
        font.meta = plist::from_bytes(bytes).map_err(|e| format!("metainfo.plist: {e}"))?;
    }
    if let Some(bytes) = files.get("fontinfo.plist") {
        font.font_info = plist::from_bytes(bytes).map_err(|e| format!("fontinfo.plist: {e}"))?;
    }
    if let Some(bytes) = files.get("lib.plist") {
        let value: plist::Dictionary =
            plist::from_bytes(bytes).map_err(|e| format!("lib.plist: {e}"))?;
        font.lib = value;
    }
    if let Some(bytes) = files.get("groups.plist") {
        font.groups = plist::from_bytes(bytes).map_err(|e| format!("groups.plist: {e}"))?;
    }
    if let Some(bytes) = files.get("kerning.plist") {
        font.kerning = plist::from_bytes(bytes).map_err(|e| format!("kerning.plist: {e}"))?;
    }
    if let Some(bytes) = files.get("features.fea") {
        font.features = std::str::from_utf8(bytes)
            .map_err(|error| format!("features.fea: {error}"))?
            .to_owned();
    }
    Ok(())
}

/// The layers a package declares, as (name, directory) pairs, default layer first.
fn declared_layers(files: &HashMap<&str, &[u8]>) -> Result<Vec<(String, String)>, String> {
    let Some(bytes) = files.get("layercontents.plist") else {
        return Ok(vec![("public.default".into(), "glyphs".into())]);
    };
    let entries: Vec<(String, String)> =
        plist::from_bytes(bytes).map_err(|error| format!("layercontents.plist: {error}"))?;
    if entries.is_empty() {
        return Err("layercontents.plist: no layers".into());
    }
    let mut names = HashSet::new();
    let mut directories = HashSet::new();
    for (name, directory) in &entries {
        if !names.insert(name.as_str()) {
            return Err(format!("layercontents.plist: duplicate layer {name:?}"));
        }
        if directory.is_empty()
            || directory.contains(['/', '\\'])
            || directory == "."
            || directory == ".."
            || !directories.insert(directory.to_lowercase())
        {
            return Err(format!(
                "layercontents.plist: unusable directory {directory:?}"
            ));
        }
    }
    let Some(default) = entries
        .iter()
        .position(|(_, directory)| directory == "glyphs")
    else {
        return Err("layercontents.plist: no layer uses the glyphs directory".into());
    };
    // Only the default layer moves; the other layers keep their order.
    let mut ordered = entries;
    let default = ordered.remove(default);
    ordered.insert(0, default);
    Ok(ordered)
}

fn load_layer_glyphs<'a>(
    files: &HashMap<&'a str, &[u8]>,
    directory: &str,
    layer: &mut norad::Layer,
    consumed: &mut HashSet<&'a str>,
    glif_paths: &mut HashMap<(String, String), String>,
) -> Result<(), String> {
    let contents_path = format!("{directory}/contents.plist");
    let contents: plist::Dictionary = match files.get_key_value(contents_path.as_str()) {
        Some((key, bytes)) => {
            consumed.insert(key);
            plist::from_bytes(bytes).map_err(|e| format!("{contents_path}: {e}"))?
        }
        None if directory == "glyphs" => plist::Dictionary::new(),
        None => return Err(format!("missing {contents_path}")),
    };
    let info_path = format!("{directory}/layerinfo.plist");
    if let Some((key, bytes)) = files.get_key_value(info_path.as_str()) {
        consumed.insert(key);
        let info: LayerInfo =
            plist::from_bytes(bytes).map_err(|error| format!("{info_path}: {error}"))?;
        layer.color = info.color;
        layer.lib = info.lib;
    }
    for (name, value) in &contents {
        crate::font::canonical_metadata::validate_name(name).map_err(|error| error.to_string())?;
        let file = value
            .as_string()
            .ok_or_else(|| format!("{contents_path}: {name:?} path is not a string"))?;
        if file.contains(['/', '\\']) || file.is_empty() || file == "." || file == ".." {
            return Err(format!("unsafe GLIF path for {name:?}: {file:?}"));
        }
        let path = format!("{directory}/{file}");
        let Some((key, bytes)) = files.get_key_value(path.as_str()) else {
            return Err(format!("missing glif: {path}"));
        };
        if !consumed.insert(key) {
            return Err(format!("duplicate GLIF path {path:?}"));
        }
        let glyph = Glyph::parse_raw(bytes).map_err(|e| format!("{path}: {e}"))?;
        if glyph.name().as_str() != name {
            return Err(format!(
                "{path}: contents name {name:?} does not match GLIF name {:?}",
                glyph.name().as_str()
            ));
        }
        glif_paths.insert((layer.name().to_string(), name.clone()), file.to_owned());
        layer.insert_glyph(glyph);
    }
    Ok(())
}

/// Decode a complete UFO package from (path, bytes) pairs relative to the UFO root.
///
/// Every layer in `layercontents.plist` loads, images and data enter the font's stores,
/// and files the specification does not name are returned in `preserved`.
pub fn decode_ufo_files<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<DecodedUfo, String> {
    let files = validate_file_inventory(files)?;
    let mut font = Font::new();
    read_root_files(&files, &mut font)?;
    let mut consumed: HashSet<&str> = files
        .keys()
        .copied()
        .filter(|path| is_supported_root_file(path))
        .collect();
    let mut layer_directories = HashMap::new();
    let mut glif_paths = HashMap::new();
    for (name, directory) in declared_layers(&files)? {
        if directory == "glyphs" && name != "public.default" {
            font.layers
                .rename_layer("public.default", &name, false)
                .map_err(|error| format!("layer {name:?}: {error}"))?;
        }
        let layer = if directory == "glyphs" {
            font.layers.default_layer_mut()
        } else {
            font.layers
                .new_layer(&name)
                .map_err(|error| format!("layer {name:?}: {error}"))?
        };
        if layer.path() != Path::new(&directory) {
            layer_directories.insert(name.clone(), directory.clone());
        }
        load_layer_glyphs(&files, &directory, layer, &mut consumed, &mut glif_paths)?;
    }
    let mut preserved = Vec::new();
    let mut remaining: Vec<_> = files
        .iter()
        .filter(|(path, _)| !consumed.contains(*path))
        .collect();
    remaining.sort();
    for (path, bytes) in remaining {
        if let Some(rest) = path.strip_prefix("images/") {
            font.images
                .insert(PathBuf::from(rest), bytes.to_vec())
                .map_err(|error| format!("{path}: {error}"))?;
        } else if let Some(rest) = path.strip_prefix("data/") {
            font.data
                .insert(PathBuf::from(rest), bytes.to_vec())
                .map_err(|error| format!("{path}: {error}"))?;
        } else {
            preserved.push(((*path).to_owned(), bytes.to_vec()));
        }
    }
    Ok(DecodedUfo {
        font,
        layer_directories,
        glif_paths,
        preserved,
    })
}

fn plist_xml<T: serde::Serialize>(value: &T, label: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    plist::to_writer_xml(&mut out, value).map_err(|error| format!("{label}: {error}"))?;
    Ok(out)
}

fn sort_plist_keys(value: &mut plist::Value) {
    match value {
        plist::Value::Dictionary(dictionary) => {
            dictionary.sort_keys();
            for (_, value) in dictionary.iter_mut() {
                sort_plist_keys(value);
            }
        }
        plist::Value::Array(values) => values.iter_mut().for_each(sort_plist_keys),
        _ => {}
    }
}

/// `lib.plist` as norad writes it: the font lib plus guideline object libs, keys sorted.
fn lib_plist(font: &Font) -> Option<plist::Dictionary> {
    let mut lib = font.lib.clone();
    let mut object_libs = plist::Dictionary::new();
    for guideline in font.font_info.guidelines.iter().flatten() {
        if let (Some(identifier), Some(guideline_lib)) = (guideline.identifier(), guideline.lib()) {
            object_libs.insert(identifier.as_str().to_owned(), guideline_lib.clone().into());
        }
    }
    if !object_libs.is_empty() {
        lib.insert("public.objectLibs".into(), object_libs.into());
    }
    if lib.is_empty() {
        return None;
    }
    let mut value = plist::Value::Dictionary(lib);
    sort_plist_keys(&mut value);
    value.into_dictionary()
}

/// `kerning.plist` as norad writes it: whole values as integers.
fn kerning_plist(font: &Font) -> plist::Dictionary {
    let mut kerning = plist::Dictionary::new();
    for (first, pairs) in &font.kerning {
        let mut inner = plist::Dictionary::new();
        for (second, value) in pairs {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a kerning value is a small whole number of font units"
            )]
            let encoded = if (value - value.round()).abs() < f64::EPSILON {
                plist::Value::Integer((value.round() as i64).into())
            } else {
                plist::Value::Real(*value)
            };
            inner.insert(second.to_string(), encoded);
        }
        kerning.insert(first.to_string(), inner.into());
    }
    kerning
}

/// Serialize a font to the UFO files norad writes to disk, as (path, bytes) pairs.
///
/// `layer_directories` restores the directory a layer was read from when norad renamed it,
/// and `glif_paths` the GLIF file name each (layer, glyph) pair had, so a save keeps the
/// package's own file names.
pub fn ufo_files_from_font(
    font: &Font,
    layer_directories: &HashMap<String, String>,
    glif_paths: &HashMap<(String, String), String>,
) -> Result<EncodedUfo, String> {
    encode_ufo(font, layer_directories, glif_paths, true)
}

fn encode_ufo(
    font: &Font,
    layer_directories: &HashMap<String, String>,
    glif_paths: &HashMap<(String, String), String>,
    include_payloads: bool,
) -> Result<EncodedUfo, String> {
    let mut encoded = EncodedUfo::default();
    encoded.files.push((
        "metainfo.plist".into(),
        plist_xml(&font.meta, "metainfo.plist")?,
    ));
    if !font.font_info.is_empty() {
        encoded.files.push((
            "fontinfo.plist".into(),
            plist_xml(&font.font_info, "fontinfo.plist")?,
        ));
    }
    if let Some(lib) = lib_plist(font) {
        encoded
            .files
            .push(("lib.plist".into(), plist_xml(&lib, "lib.plist")?));
    }
    if !font.groups.is_empty() {
        encoded.files.push((
            "groups.plist".into(),
            plist_xml(&font.groups, "groups.plist")?,
        ));
    }
    if !font.kerning.is_empty() {
        encoded.files.push((
            "kerning.plist".into(),
            plist_xml(&kerning_plist(font), "kerning.plist")?,
        ));
    }
    if !font.features.is_empty() {
        let features = font.features.replace("\r\n", "\n");
        encoded
            .files
            .push(("features.fea".into(), features.into_bytes()));
    }
    let mut layer_contents = Vec::new();
    for layer in font.layers.iter() {
        let name = layer.name().to_string();
        let directory = layer_directories
            .get(&name)
            .cloned()
            .unwrap_or_else(|| layer.path().to_string_lossy().into_owned());
        let mut contents = BTreeMap::new();
        let mut taken: HashSet<String> = layer
            .iter()
            .filter_map(|glyph| glif_paths.get(&(name.clone(), glyph.name().to_string())))
            .map(|file| file.to_lowercase())
            .collect();
        for glyph in layer.iter() {
            let glyph_name = glyph.name().as_str();
            let file = match glif_paths.get(&(name.clone(), glyph_name.to_owned())) {
                Some(file) => file.clone(),
                None => {
                    let generated = layer
                        .get_path(glyph_name)
                        .ok_or_else(|| format!("{directory}: glyph {glyph_name:?} has no file"))?
                        .to_string_lossy()
                        .into_owned();
                    let mut candidate = generated.clone();
                    let mut counter = 1;
                    while !taken.insert(candidate.to_lowercase()) {
                        let stem = generated.strip_suffix(".glif").unwrap_or(&generated);
                        candidate = format!("{stem}{counter:03}.glif");
                        counter += 1;
                    }
                    candidate
                }
            };
            if include_payloads {
                encoded
                    .files
                    .push((format!("{directory}/{file}"), glif_bytes(glyph)?));
            }
            contents.insert(glyph_name.to_owned(), file);
        }
        encoded.files.push((
            format!("{directory}/contents.plist"),
            plist_xml(&contents, "contents.plist")?,
        ));
        if layer.color.is_some() || !layer.lib.is_empty() {
            let mut info = plist::Dictionary::new();
            if let Some(color) = &layer.color {
                info.insert("color".into(), color.to_rgba_string().into());
            }
            if !layer.lib.is_empty() {
                info.insert("lib".into(), layer.lib.clone().into());
            }
            let mut value = plist::Value::Dictionary(info);
            sort_plist_keys(&mut value);
            encoded.files.push((
                format!("{directory}/layerinfo.plist"),
                plist_xml(&value, "layerinfo.plist")?,
            ));
        }
        layer_contents.push((name, directory.clone()));
        encoded.layer_directories.push(directory);
    }
    encoded.files.push((
        "layercontents.plist".into(),
        plist_xml(&layer_contents, "layercontents.plist")?,
    ));
    if include_payloads {
        store_files("data", font.data.iter(), &mut encoded.files)?;
        store_files("images", font.images.iter(), &mut encoded.files)?;
    }
    Ok(encoded)
}

fn store_files<'a>(
    prefix: &str,
    store: impl Iterator<
        Item = (
            &'a PathBuf,
            Result<std::sync::Arc<[u8]>, norad::error::StoreError>,
        ),
    >,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    let mut entries: Vec<_> = store.collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (path, entry) in entries {
        let bytes = entry.map_err(|error| format!("{prefix}/{}: {error}", path.display()))?;
        files.push((format!("{prefix}/{}", path.display()), bytes.to_vec()));
    }
    Ok(())
}

fn first_component(path: &str) -> &str {
    path.split('/').next().unwrap_or(path)
}

/// Open a UFO or designspace from the files of one folder.
///
/// `ufo_name` names the folder when it is the UFO itself; the paths are then relative to the
/// UFO root. Otherwise the folder holds one designspace with its UFOs, or one UFO, and every
/// path is relative to the folder.
pub fn open_memory_project(
    ufo_name: Option<&str>,
    files: &[(String, Vec<u8>)],
) -> Result<MemoryProject, String> {
    use crate::font::project::{Project, SourceInput};
    let mut bookkeeping = MemoryBookkeeping::default();
    let decode_ufo = |source_path: PathBuf,
                      prefix: &str,
                      bookkeeping: &mut MemoryBookkeeping|
     -> Result<SourceInput, String> {
        let selected: Vec<(&str, &[u8])> = files
            .iter()
            .filter_map(|(path, bytes)| {
                path.strip_prefix(prefix)
                    .map(|relative| (relative, bytes.as_slice()))
            })
            .collect();
        if selected.is_empty() {
            return Err(format!("{}: no files", source_path.display()));
        }
        let decoded = decode_ufo_files(selected.iter().copied())
            .map_err(|error| format!("{}: {error}", source_path.display()))?;
        let mut originals = HashMap::new();
        for ((layer_name, glyph_name), file) in &decoded.glif_paths {
            let layer = decoded
                .font
                .layers
                .get(layer_name)
                .ok_or("missing decoded layer")?;
            let directory = decoded
                .layer_directories
                .get(layer_name)
                .cloned()
                .unwrap_or_else(|| layer.path().to_string_lossy().into_owned());
            let path = format!("{directory}/{file}");
            let bytes = selected
                .iter()
                .find(|(name, _)| *name == path)
                .map(|(_, bytes)| bytes.to_vec())
                .ok_or("missing original GLIF bytes")?;
            let glyph = layer.get_glyph(glyph_name).ok_or("missing decoded glyph")?;
            if let Some(original) = super::OriginalGlif::capture(bytes, glyph)? {
                originals.insert(path, original);
            }
        }
        bookkeeping
            .original_glifs
            .insert(source_path.clone(), originals);
        bookkeeping
            .layer_directories
            .insert(source_path.clone(), decoded.layer_directories);
        bookkeeping
            .glif_paths
            .insert(source_path.clone(), decoded.glif_paths);
        bookkeeping
            .preserved
            .insert(source_path.clone(), decoded.preserved);
        Ok(SourceInput::from_font(decoded.font, source_path))
    };
    if let Some(name) = ufo_name {
        if !safe_relative_path(name) || name.contains('/') {
            return Err(format!("unsafe UFO name {name:?}"));
        }
        let input = decode_ufo(PathBuf::from(name), "", &mut bookkeeping)?;
        let mut project = Project::from_source(input).map_err(|error| error.to_string())?;
        project.export_source = Some(PathBuf::from(name));
        return retain_opening_metadata(project, bookkeeping, files, true);
    }
    let mut designspaces: Vec<&str> = files
        .iter()
        .map(|(path, _)| path.as_str())
        .filter(|path| !path.contains('/') && path.ends_with(".designspace"))
        .collect();
    designspaces.sort_unstable();
    let project = match designspaces.as_slice() {
        [] => {
            let mut ufos: Vec<&str> = files
                .iter()
                .map(|(path, _)| first_component(path))
                .filter(|name| name.to_lowercase().ends_with(".ufo"))
                .collect();
            ufos.sort_unstable();
            ufos.dedup();
            let name = match ufos.as_slice() {
                [name] => *name,
                [] => return Err("this folder holds no UFO or designspace".into()),
                _ => {
                    return Err(
                        "this folder holds several UFOs and no designspace; open one UFO".into(),
                    );
                }
            };
            let input = decode_ufo(PathBuf::from(name), &format!("{name}/"), &mut bookkeeping)?;
            let mut project = Project::from_source(input).map_err(|error| error.to_string())?;
            project.export_source = Some(PathBuf::from(name));
            project
        }
        [designspace] => {
            let bytes = files
                .iter()
                .find(|(path, _)| path == designspace)
                .map(|(_, bytes)| bytes)
                .ok_or("designspace vanished")?;
            let xml = String::from_utf8_lossy(bytes);
            let document =
                designspace_from_str(&xml).map_err(|error| format!("{designspace}: {error}"))?;
            let mut project = Project::from_designspace(document, |filename| {
                if !safe_relative_path(filename) {
                    return Err(format!("{designspace}: unsafe source path {filename:?}"));
                }
                decode_ufo(
                    PathBuf::from(filename),
                    &format!("{filename}/"),
                    &mut bookkeeping,
                )
            })?;
            project.export_source = Some(PathBuf::from(designspace));
            project.compute_compat();
            bookkeeping.original_designspace = Some((
                project
                    .document_designspace()
                    .ok_or("missing imported designspace")?
                    .clone(),
                bytes.clone(),
            ));
            project
        }
        _ => {
            return Err("this folder holds more than one designspace; open one UFO instead".into());
        }
    };
    let mut paths = HashSet::new();
    for (path, bytes) in files_outside_sources(&project, files) {
        if !safe_relative_path(path) || !paths.insert(path) {
            return Err(format!("invalid or duplicate project file path {path:?}"));
        }
        bookkeeping
            .project_files
            .push((path.clone(), bytes.clone()));
    }
    retain_opening_metadata(project, bookkeeping, files, false)
}

fn retain_opening_metadata(
    project: crate::font::project::Project,
    mut bookkeeping: MemoryBookkeeping,
    files: &[(String, Vec<u8>)],
    opened_ufo_root: bool,
) -> Result<MemoryProject, String> {
    for source in project.document_sources() {
        let path = source.path();
        let font = project
            .encode_ufo_source(source.id())
            .ok_or_else(|| format!("{}: source data is missing", path.display()))?;
        let (Some(directories), Some(glif_paths)) = (
            bookkeeping.layer_directories.get(path),
            bookkeeping.glif_paths.get(path),
        ) else {
            return Err(format!(
                "{}: imported source paths are missing",
                path.display()
            ));
        };
        let metadata = encode_ufo(&font, directories, glif_paths, false)?;
        let prefix = if opened_ufo_root {
            String::new()
        } else {
            format!("{}/", path.display())
        };
        let mut originals: HashMap<&str, &[u8]> = files
            .iter()
            .filter_map(|(name, bytes)| {
                let relative = name.strip_prefix(&prefix)?;
                let layer_metadata = relative.rsplit_once('/').is_some_and(|(dir, file)| {
                    matches!(file, "contents.plist" | "layerinfo.plist")
                        && metadata.layer_directories.iter().any(|layer| layer == dir)
                });
                (is_supported_root_file(relative) || layer_metadata)
                    .then_some((relative, bytes.as_slice()))
            })
            .collect();
        let mut retained = BTreeMap::new();
        for (file, canonical) in metadata.files {
            let original = originals.remove(file.as_str());
            // A minimally readable input may omit a required package file. Still write that
            // file when making a complete copy. Optional metadata keeps its original presence.
            let required = matches!(file.as_str(), "metainfo.plist" | "layercontents.plist")
                || file.ends_with("/contents.plist");
            if original != Some(canonical.as_slice()) && (original.is_some() || !required) {
                retained.insert(
                    file,
                    OriginalMetadataFile {
                        original: original.map(<[u8]>::to_vec),
                        canonical: Some(canonical),
                    },
                );
            }
        }
        // An empty optional file can be present even when the encoder emits nothing for it.
        for (file, bytes) in originals {
            retained.insert(
                file.to_owned(),
                OriginalMetadataFile {
                    original: Some(bytes.to_vec()),
                    canonical: None,
                },
            );
        }
        bookkeeping
            .original_metadata
            .insert(path.to_owned(), retained);
    }
    Ok(MemoryProject {
        project,
        bookkeeping,
    })
}

fn restore_unchanged_metadata(
    encoded: &mut EncodedUfo,
    original: &BTreeMap<String, OriginalMetadataFile>,
) {
    let mut emitted = HashSet::new();
    encoded.files.retain_mut(|(file, bytes)| {
        let Some((name, entry)) = original.get_key_value(file) else {
            return true;
        };
        emitted.insert(name);
        if entry.canonical.as_ref() != Some(bytes) {
            return true;
        }
        if let Some(raw) = &entry.original {
            bytes.clone_from(raw);
            true
        } else {
            false
        }
    });
    for (file, entry) in original {
        if emitted.contains(file) || entry.canonical.is_some() {
            continue;
        }
        // A removed layer takes its empty layerinfo.plist with it.
        if let Some((directory, _)) = file.rsplit_once('/')
            && !encoded
                .layer_directories
                .iter()
                .any(|layer| layer == directory)
        {
            continue;
        }
        if let Some(raw) = &entry.original {
            encoded.files.push((file.clone(), raw.clone()));
        }
    }
}

fn files_outside_sources<'a>(
    project: &crate::font::project::Project,
    files: &'a [(String, Vec<u8>)],
) -> impl Iterator<Item = &'a (String, Vec<u8>)> {
    let managed: HashSet<&Path> = project
        .document_sources()
        .map(|source| source.path())
        .chain(project.export_source.as_deref())
        .collect();
    files.iter().filter(move |(path, _)| {
        !Path::new(path)
            .ancestors()
            .any(|part| managed.contains(part))
    })
}

/// The files that persist `project`, relative to the folder it was opened from.
///
/// Every source is written in full, retaining original bytes for unchanged glyphs and metadata.
/// The designspace is written only when its metadata changed.
/// Call [`crate::font::project::Project::mark_saved`] once the host has written them.
pub fn memory_save_files(
    project: &crate::font::project::Project,
    bookkeeping: &MemoryBookkeeping,
) -> Result<MemorySave, String> {
    encode_memory_project(project, bookkeeping, false)
}

/// Encode every source, the current Designspace descriptor, and sidecar files for a new destination.
///
/// A standalone UFO keeps its own directory and has no Designspace descriptor.
pub fn memory_save_as_files(
    project: &crate::font::project::Project,
    bookkeeping: &MemoryBookkeeping,
) -> Result<MemorySave, String> {
    encode_memory_project(project, bookkeeping, true)
}

fn encode_memory_project(
    project: &crate::font::project::Project,
    bookkeeping: &MemoryBookkeeping,
    into_new_folder: bool,
) -> Result<MemorySave, String> {
    let mut save = MemorySave::default();
    let empty_directories = HashMap::new();
    let empty_glif_paths = HashMap::new();
    for source in project.document_sources() {
        let id = source.id();
        let path = source.path().to_path_buf();
        let font = project
            .encode_ufo_source(id)
            .ok_or_else(|| format!("{}: source data is missing", path.display()))?;
        let layer_directories = bookkeeping
            .layer_directories
            .get(&path)
            .unwrap_or(&empty_directories);
        let glif_paths = bookkeeping
            .glif_paths
            .get(&path)
            .unwrap_or(&empty_glif_paths);
        let mut encoded = ufo_files_from_font(&font, layer_directories, glif_paths)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if let Some(original) = bookkeeping.original_metadata.get(&path) {
            restore_unchanged_metadata(&mut encoded, original);
        }
        let prefix = path.to_string_lossy();
        for (file, bytes) in encoded.files {
            let retained = bookkeeping
                .original_glifs
                .get(&path)
                .and_then(|files| files.get(&file))
                .and_then(|original| original.unchanged_bytes(&bytes));
            let bytes = if let Some(original) = retained {
                original.to_vec()
            } else {
                bytes
            };
            save.files.push((format!("{prefix}/{file}"), bytes));
        }
        for directory in encoded.layer_directories {
            save.glif_directories.push(format!("{prefix}/{directory}"));
        }
        for (file, bytes) in bookkeeping.preserved.get(&path).into_iter().flatten() {
            save.files.push((format!("{prefix}/{file}"), bytes.clone()));
        }
    }
    if project.ds_dirty || (into_new_folder && project.document_designspace().is_some()) {
        let destination = project
            .export_source
            .as_ref()
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "designspace")
            })
            .ok_or("the designspace has no destination file")?;
        let canonical = project
            .document_designspace()
            .ok_or("designspace document is unavailable")?;
        let bytes = match &bookkeeping.original_designspace {
            Some((original, bytes)) if into_new_folder && original == canonical => bytes.clone(),
            _ => crate::formats::designspace::to_xml(&canonical.to_norad()?)?.into_bytes(),
        };
        save.files
            .push((destination.to_string_lossy().into_owned(), bytes));
    }
    if into_new_folder {
        save.files
            .extend(files_outside_sources(project, &bookkeeping.project_files).cloned());
    }
    Ok(save)
}

#[cfg(test)]
mod tests {
    use super::*;

    const METAINFO: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>creator</key><string>org.linebender.runebender.tests</string>
<key>formatVersion</key><integer>3</integer>
</dict></plist>"#;
    const FONTINFO: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>familyName</key><string>MemTest</string>
<key>styleName</key><string>Regular</string>
<key>unitsPerEm</key><integer>1000</integer>
<key>ascender</key><integer>800</integer>
<key>descender</key><integer>-200</integer>
</dict></plist>"#;
    const LIB: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>com.linebender.test</key><string>preserved</string>
</dict></plist>"#;
    const GROUPS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>public.kern1.Left</key><array><string>A</string></array>
</dict></plist>"#;
    const KERNING: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>public.kern1.Left</key><dict><key>A</key><integer>-25</integer></dict>
</dict></plist>"#;
    const CONTENTS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>A</key><string>A_.glif</string>
</dict></plist>"#;
    const GLIF: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<glyph name="A" format="2">
<advance width="600"/>
<unicode hex="0041"/>
<outline>
<contour>
<point x="0" y="0" type="line"/>
<point x="100" y="0" type="line"/>
<point x="50" y="700" type="line"/>
</contour>
</outline>
</glyph>"#;
    const FEATURES: &[u8] = b"feature liga { sub A A by A; } liga;\n";

    #[test]
    fn embedded_glif_json_enters_project_canonically() {
        let project = project_from_embedded_glif_json(
            PathBuf::from("VirtuaGrotesk-Regular.ufo"),
            include_str!("../../../web/demo-font.json"),
        )
        .expect("the browser demo imports");
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        assert_eq!(project.glyph_names().count(), 863);
        assert_eq!(
            project
                .document_font_info(source)
                .unwrap()
                .names
                .family_name
                .as_deref(),
            Some("Virtua Grotesk")
        );
        assert_eq!(
            project
                .document_font_info(source)
                .unwrap()
                .metrics
                .units_per_em,
            Some(1024.0)
        );
        let a = project.document_layer("A", &layer).unwrap();
        assert_eq!(a.width(), 716.0);
        assert_eq!(a.codepoints().collect::<Vec<_>>(), ['A']);
        assert!(!project.is_modified());
    }

    #[test]
    fn rejects_unsafe_and_duplicate_memory_paths() {
        let duplicate = decode_ufo_files([
            ("glyphs/contents.plist", CONTENTS),
            ("glyphs/contents.plist", CONTENTS),
        ])
        .unwrap_err();
        assert!(duplicate.contains("duplicate UFO path"));
        let unsafe_path = decode_ufo_files([("../fontinfo.plist", FONTINFO)]).unwrap_err();
        assert!(unsafe_path.contains("unsafe UFO path"));
    }

    #[test]
    fn parses_designspace_text() {
        let ds = r#"<?xml version='1.0' encoding='UTF-8'?>
<designspace format="4.0">
  <axes><axis name="Weight" tag="wght" minimum="400" default="400" maximum="700"/></axes>
  <sources>
    <source familyname="T" stylename="Regular" filename="T-Regular.ufo">
      <location><dimension name="Weight" xvalue="400"/></location>
    </source>
  </sources>
</designspace>"#;
        let doc = designspace_from_str(ds).expect("parses");
        assert_eq!(doc.axes.len(), 1);
        assert_eq!(doc.sources.len(), 1);
    }

    fn read_tree(root: &Path, directory: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<_> = std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                read_tree(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.push((relative, std::fs::read(&path).unwrap()));
            }
        }
    }

    fn pairs(files: &[(String, Vec<u8>)]) -> Vec<(&str, &[u8])> {
        files
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
            .collect()
    }

    const BACKGROUND_LAYERCONTENTS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><array>
<array><string>public.default</string><string>glyphs</string></array>
<array><string>public.background</string><string>glyphs.background</string></array>
</array></plist>"#;
    const BACKGROUND_INFO: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>color</key><string>1,0,0,1</string>
</dict></plist>"#;
    const IMAGE_GLIF: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<glyph name="A" format="2">
<advance width="600"/>
<image fileName="reference.png"/>
<outline>
<contour>
<point x="0" y="0" type="line"/>
<point x="100" y="0" type="line"/>
<point x="50" y="700" type="line"/>
</contour>
</outline>
</glyph>"#;

    fn complete_files() -> Vec<(&'static str, &'static [u8])> {
        let png = include_bytes!("../../../tests/fixtures/variable/reference.png");
        vec![
            ("metainfo.plist", METAINFO),
            ("fontinfo.plist", FONTINFO),
            ("lib.plist", LIB),
            ("groups.plist", GROUPS),
            ("kerning.plist", KERNING),
            ("features.fea", FEATURES),
            ("layercontents.plist", BACKGROUND_LAYERCONTENTS),
            ("glyphs/contents.plist", CONTENTS),
            ("glyphs/A_.glif", IMAGE_GLIF),
            ("glyphs.background/contents.plist", CONTENTS),
            ("glyphs.background/layerinfo.plist", BACKGROUND_INFO),
            ("glyphs.background/A_.glif", GLIF),
            ("images/reference.png", png),
            ("data/com.example/payload.bin", b"payload"),
            ("notes.txt", b"kept as is"),
        ]
    }

    #[test]
    fn complete_package_round_trips_through_memory() {
        let decoded = decode_ufo_files(complete_files()).expect("complete package decodes");
        assert_eq!(decoded.font.layers.len(), 2);
        assert_eq!(
            decoded
                .layer_directories
                .get("public.background")
                .map(String::as_str),
            Some("glyphs.background")
        );
        assert_eq!(
            decoded.preserved,
            vec![("notes.txt".to_owned(), b"kept as is".to_vec())]
        );
        assert!(
            decoded
                .font
                .images
                .get(Path::new("reference.png"))
                .is_some()
        );
        assert!(
            decoded
                .font
                .data
                .get(Path::new("com.example/payload.bin"))
                .is_some()
        );
        let background = decoded.font.layers.get("public.background").unwrap();
        assert!(background.color.is_some());

        let encoded = ufo_files_from_font(
            &decoded.font,
            &decoded.layer_directories,
            &decoded.glif_paths,
        )
        .unwrap();
        let mut written: Vec<&str> = encoded
            .files
            .iter()
            .map(|(path, _)| path.as_str())
            .collect();
        written.sort_unstable();
        let mut expected: Vec<&str> = complete_files()
            .iter()
            .map(|(path, _)| *path)
            .filter(|path| *path != "notes.txt")
            .collect();
        expected.sort_unstable();
        assert_eq!(written, expected);
        assert_eq!(encoded.layer_directories, ["glyphs", "glyphs.background"]);

        let again = decode_ufo_files(pairs(&encoded.files)).expect("written package decodes");
        assert_eq!(again.font, decoded.font);
        assert_eq!(again.layer_directories, decoded.layer_directories);
    }

    #[test]
    fn folder_with_one_ufo_opens_as_a_project() {
        let files: Vec<(String, Vec<u8>)> = complete_files()
            .into_iter()
            .map(|(path, bytes)| (format!("Mem.ufo/{path}"), bytes.to_vec()))
            .collect();
        let opened = open_memory_project(None, &files).expect("folder opens");
        assert_eq!(opened.project.glyph_names().count(), 1);
        assert_eq!(
            opened.project.export_source.as_deref(),
            Some(Path::new("Mem.ufo"))
        );
        let save = memory_save_files(&opened.project, &opened.bookkeeping).unwrap();
        assert!(
            save.files
                .iter()
                .any(|(path, _)| path == "Mem.ufo/glyphs/A_.glif")
        );
        assert!(
            save.files
                .iter()
                .any(|(path, _)| path == "Mem.ufo/notes.txt")
        );
        assert!(
            save.files
                .iter()
                .any(|(path, _)| path == "Mem.ufo/glyphs.background/A_.glif")
        );
        assert_eq!(
            save.glif_directories,
            ["Mem.ufo/glyphs", "Mem.ufo/glyphs.background"]
        );

        let as_root: Vec<(String, Vec<u8>)> = complete_files()
            .into_iter()
            .map(|(path, bytes)| (path.to_owned(), bytes.to_vec()))
            .collect();
        let opened = open_memory_project(Some("Mem.ufo"), &as_root).expect("ufo opens");
        let save = memory_save_files(&opened.project, &opened.bookkeeping).unwrap();
        assert!(
            save.files
                .iter()
                .all(|(path, _)| path.starts_with("Mem.ufo/"))
        );
    }

    #[test]
    fn ordinary_saves_and_copies_preserve_unchanged_ufo_metadata_and_empty_files() {
        let mut files: Vec<(String, Vec<u8>)> = complete_files()
            .into_iter()
            .map(|(file, bytes)| (file.into(), bytes.to_vec()))
            .collect();
        files.retain(|(name, _)| name != "groups.plist" && name != "kerning.plist");
        let empty =
            b"<?xml version=\"1.0\"?>\n<plist version=\"1.0\"><dict><!-- empty --></dict></plist>";
        files.push(("groups.plist".into(), empty.to_vec()));
        files.push(("glyphs/layerinfo.plist".into(), empty.to_vec()));
        let mut opened = open_memory_project(Some("Face.ufo"), &files).unwrap();
        let copy = memory_save_as_files(&opened.project, &opened.bookkeeping).unwrap();
        let metadata = |save: MemorySave| -> BTreeMap<_, _> {
            save.files
                .into_iter()
                .map(|(path, bytes)| (path.strip_prefix("Face.ufo/").unwrap().to_owned(), bytes))
                .collect()
        };
        let copy = metadata(copy);
        let saved = metadata(memory_save_files(&opened.project, &opened.bookkeeping).unwrap());
        assert_eq!(saved, copy);
        for (file, bytes) in &files {
            if file.ends_with(".plist") {
                assert_eq!(copy.get(file), Some(bytes), "{file} keeps its input bytes");
            }
        }
        assert!(!copy.contains_key("kerning.plist"));
        assert!(opened.project.set_feature_text(String::new()));
        let cleared = metadata(memory_save_files(&opened.project, &opened.bookkeeping).unwrap());
        assert!(!cleared.contains_key("features.fea"));
        assert_eq!(cleared["groups.plist"], empty);
        let source = opened.project.source_id(0).unwrap();
        opened
            .project
            .edit_document_source_metadata(source, |draft| {
                let mut info = draft.font_info().clone();
                info.metrics.ascender = Some(900.0);
                draft.set_font_info(info);
                Ok(())
            })
            .unwrap();
        let edited = metadata(memory_save_as_files(&opened.project, &opened.bookkeeping).unwrap());
        assert_ne!(copy["fontinfo.plist"], edited["fontinfo.plist"]);
        let info: norad::FontInfo = plist::from_bytes(&edited["fontinfo.plist"]).unwrap();
        assert_eq!(info.ascender, Some(900.0));
        assert_eq!(edited["groups.plist"], empty);
        assert_eq!(edited["glyphs/layerinfo.plist"], empty);
        assert_eq!(
            edited["glyphs/contents.plist"],
            copy["glyphs/contents.plist"]
        );
    }

    #[test]
    fn designspace_folder_round_trips_through_memory() {
        fn descriptor(files: &[(String, Vec<u8>)]) -> &[u8] {
            &files
                .iter()
                .find(|(name, _)| name == "Core.designspace")
                .unwrap()
                .1
        }
        let root = crate::testing::fonts::dir();
        let mut files = Vec::new();
        read_tree(&root, &root, &mut files);
        // Arrange the source folders below the descriptor, as in a family with nested masters.
        files.retain(|(path, _)| {
            (!path.contains('/') && path.ends_with(".designspace"))
                || first_component(path).to_lowercase().ends_with(".ufo")
        });
        for (path, _) in &mut files {
            if path.contains('/') {
                path.insert_str(0, "masters/");
            }
        }
        let sidecar = (
            "includes/ligatures.fea".to_owned(),
            b"# keep verbatim\r\n".to_vec(),
        );
        files.push(sidecar.clone());
        let (_, xml) = files
            .iter_mut()
            .find(|(name, _)| name == "Core.designspace")
            .unwrap();
        *xml = std::str::from_utf8(xml)
            .unwrap()
            .replace("filename=\"Core-", "filename=\"masters/Core-")
            .into_bytes();
        xml.extend_from_slice(b"<!-- Preserve the designer's note in an unchanged copy. -->\n");
        let mut opened = open_memory_project(None, &files).expect("designspace folder opens");
        assert_eq!(opened.project.document_sources().count(), 2);
        assert!(!opened.project.is_modified());
        assert_eq!(
            opened.project.export_source.as_deref(),
            Some(Path::new("Core.designspace"))
        );

        let save = memory_save_files(&opened.project, &opened.bookkeeping).unwrap();
        assert!(
            !save
                .files
                .iter()
                .any(|(path, _)| path.ends_with(".designspace")),
            "an unchanged designspace is not rewritten"
        );
        let mut written: Vec<&str> = save.files.iter().map(|(path, _)| path.as_str()).collect();
        written.sort_unstable();
        let mut original: Vec<&str> = files
            .iter()
            .map(|(path, _)| path.as_str())
            .filter(|path| path.starts_with("masters/"))
            .collect();
        original.sort_unstable();
        assert_eq!(
            written, original,
            "an in-place save writes only managed sources"
        );

        let copy = memory_save_as_files(&opened.project, &opened.bookkeeping).unwrap();
        assert_eq!(descriptor(&copy.files), descriptor(&files));
        assert!(copy.files.contains(&sidecar));
        let reopened = open_memory_project(None, &copy.files).expect("the copied family reopens");
        assert_eq!(
            reopened.project.document_designspace(),
            opened.project.document_designspace()
        );
        assert_eq!(reopened.project.document_sources().count(), 2);

        for source in opened.project.document_sources() {
            let prefix = format!("{}/", source.path().display());
            let before: Vec<(&str, &[u8])> = files
                .iter()
                .filter_map(|(path, bytes)| {
                    path.strip_prefix(&prefix)
                        .map(|rest| (rest, bytes.as_slice()))
                })
                .collect();
            let after: Vec<(&str, &[u8])> = save
                .files
                .iter()
                .filter_map(|(path, bytes)| {
                    path.strip_prefix(&prefix)
                        .map(|rest| (rest, bytes.as_slice()))
                })
                .collect();
            let before = decode_ufo_files(before).unwrap();
            let after = decode_ufo_files(after).unwrap();
            assert_eq!(
                after.font, before.font,
                "{prefix} is unchanged by a round trip"
            );
        }

        let bold = opened.project.source_id(1).unwrap();
        let location = opened.project.master_locations()[1].clone();
        opened
            .project
            .update_source(bold, "Heavy", &location)
            .unwrap();
        let edited = memory_save_files(&opened.project, &opened.bookkeeping).unwrap();
        assert_ne!(descriptor(&edited.files), descriptor(&files));
        opened.project.mark_saved();
        let copy = memory_save_as_files(&opened.project, &opened.bookkeeping).unwrap();
        assert_eq!(descriptor(&copy.files), descriptor(&edited.files));
        let reopened = open_memory_project(None, &copy.files).expect("the edited family reopens");
        assert_eq!(
            reopened.project.document_designspace().unwrap().sources()[1]
                .style_name
                .as_deref(),
            Some("Heavy")
        );
    }
}
