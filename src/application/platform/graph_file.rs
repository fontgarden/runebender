// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded persistence for native live graph intent.
//!
//! Source bindings and live fork handles are session state, so they are never written.
//! Saves detect observed external edits but do not claim an operating-system compare-and-swap
//! against an arbitrary writer racing the final same-directory rename.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use runebender::workflows::graph::{EXTENSION, FILE_VERSION, NodeGraph};

use super::guarded_file::{self, GuardedFileError};

const MAX_GRAPH_BYTES: usize = 2 * 1024 * 1024;

/// Exact disk identity for one live graph file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiveGraphFileMetadata {
    /// Explicit path selected by the user.
    pub(crate) path: PathBuf,
    /// SHA-256 revision of the exact file bytes.
    pub(crate) revision: String,
}

/// One bounded graph file with its session-only values removed.
#[derive(Clone, Debug)]
pub(crate) struct LiveGraphDocument {
    /// Metadata for the exact bytes read from disk.
    pub(crate) metadata: LiveGraphFileMetadata,
    /// Graph authoring intent, without a source binding or live fork handle.
    pub(crate) graph: NodeGraph,
}

/// Failure while loading or saving native live graph intent.
#[derive(Debug)]
pub(crate) enum LiveGraphFileError {
    /// The selected path is not a direct `.graph.json` file in an existing directory.
    InvalidPath,
    /// The selected entry is missing, not a regular file, or is a symbolic link.
    NotFound,
    /// The file exceeds the live graph persistence bound.
    TooLarge,
    /// The file is not UTF-8 or not a versioned node graph.
    InvalidGraph(String),
    /// The destination exists when a new graph was requested.
    AlreadyExists,
    /// The on-disk revision differs from the revision observed by the caller.
    Conflict {
        /// Revision currently observed on disk.
        actual_revision: String,
    },
    /// A filesystem operation failed.
    Io(io::Error),
}

impl fmt::Display for LiveGraphFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath => formatter.write_str(
                "live graph path must name a direct .graph.json file in an existing directory",
            ),
            Self::NotFound => formatter.write_str("live graph is missing or is not a regular file"),
            Self::TooLarge => formatter.write_str("live graph exceeds 2097152 UTF-8 bytes"),
            Self::InvalidGraph(error) => write!(formatter, "invalid live graph: {error}"),
            Self::AlreadyExists => formatter.write_str("live graph already exists"),
            Self::Conflict { actual_revision } => write!(
                formatter,
                "live graph changed on disk (current revision {}); reopen or save to a new path",
                actual_revision.chars().take(12).collect::<String>()
            ),
            Self::Io(error) => write!(formatter, "live graph I/O failed: {error}"),
        }
    }
}

impl std::error::Error for LiveGraphFileError {}

impl From<io::Error> for LiveGraphFileError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<GuardedFileError> for LiveGraphFileError {
    fn from(error: GuardedFileError) -> Self {
        match error {
            GuardedFileError::NotFound => Self::NotFound,
            GuardedFileError::TooLarge => Self::TooLarge,
            GuardedFileError::AlreadyExists => Self::AlreadyExists,
            GuardedFileError::Conflict { actual_revision } => Self::Conflict { actual_revision },
            GuardedFileError::Io(error) => Self::Io(error),
        }
    }
}

/// Load one bounded live graph and remove any stale session binding embedded by older writers.
pub(crate) fn load(path: &Path) -> Result<LiveGraphDocument, LiveGraphFileError> {
    validate_path(path)?;
    let bytes = read_path(path)?;
    document_from_bytes(path, bytes)
}

fn document_from_bytes(
    path: &Path,
    bytes: Vec<u8>,
) -> Result<LiveGraphDocument, LiveGraphFileError> {
    let revision = guarded_file::revision(&bytes);
    reject_unknown_fields(&bytes)?;
    let mut graph: NodeGraph = serde_json::from_slice(&bytes)
        .map_err(|error| LiveGraphFileError::InvalidGraph(error.to_string()))?;
    validate_identity_fields(&graph)?;
    strip_session_values(&mut graph);
    Ok(LiveGraphDocument {
        metadata: LiveGraphFileMetadata {
            path: path.to_owned(),
            revision,
        },
        graph,
    })
}

/// Save graph authoring intent at an explicit path with an observed-revision guard.
///
/// `None` requires the destination to remain absent.
/// `Some` detects an observed external change before staging and immediately before rename.
pub(crate) fn save(
    path: &Path,
    graph: &NodeGraph,
    expected_revision: Option<&str>,
) -> Result<LiveGraphDocument, LiveGraphFileError> {
    let parent = validate_path(path)?;
    guarded_file::check_expected(path, expected_revision, MAX_GRAPH_BYTES)?;
    let mut graph = graph.clone();
    strip_session_values(&mut graph);
    let bytes = serde_json::to_vec_pretty(&graph)
        .map_err(|error| LiveGraphFileError::InvalidGraph(error.to_string()))?;
    if bytes.len() > MAX_GRAPH_BYTES {
        return Err(LiveGraphFileError::TooLarge);
    }
    guarded_file::write_guarded(
        &parent,
        path,
        &bytes,
        expected_revision,
        MAX_GRAPH_BYTES,
        ".runebender-graph",
    )?;
    let written_revision = guarded_file::revision(&bytes);
    let actual_bytes = read_path(path)?;
    let actual_revision = guarded_file::revision(&actual_bytes);
    if actual_revision != written_revision {
        return Err(LiveGraphFileError::Conflict { actual_revision });
    }
    document_from_bytes(path, actual_bytes)
}

fn reject_unknown_fields(bytes: &[u8]) -> Result<(), LiveGraphFileError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| LiveGraphFileError::InvalidGraph(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| LiveGraphFileError::InvalidGraph("root must be an object".into()))?;
    if let Some(field) = object
        .keys()
        .find(|field| !matches!(field.as_str(), "version" | "nodes" | "links"))
    {
        return Err(LiveGraphFileError::InvalidGraph(format!(
            "unknown root field {field:?}"
        )));
    }
    if let Some(nodes) = object.get("nodes").and_then(serde_json::Value::as_array) {
        for node in nodes {
            let Some(node) = node.as_object() else {
                continue;
            };
            if let Some(field) = node
                .keys()
                .find(|field| !matches!(field.as_str(), "id" | "type" | "pos" | "values"))
            {
                return Err(LiveGraphFileError::InvalidGraph(format!(
                    "unknown node field {field:?}"
                )));
            }
        }
    }
    Ok(())
}

fn validate_identity_fields(graph: &NodeGraph) -> Result<(), LiveGraphFileError> {
    if graph.version != FILE_VERSION {
        return Err(LiveGraphFileError::InvalidGraph(format!(
            "unsupported version {}; expected {FILE_VERSION}",
            graph.version
        )));
    }
    let mut ids = std::collections::HashSet::new();
    if let Some(duplicate) = graph
        .nodes
        .iter()
        .map(|node| node.id)
        .find(|id| !ids.insert(*id))
    {
        return Err(LiveGraphFileError::InvalidGraph(format!(
            "duplicate node id {duplicate}"
        )));
    }
    Ok(())
}

fn strip_session_values(graph: &mut NodeGraph) {
    for node in &mut graph.nodes {
        match node.type_name.as_str() {
            "live.font" => {
                node.values.remove("source");
            }
            "live.fork" => {
                node.values.remove("branch");
            }
            _ => {}
        }
    }
}

fn validate_path(path: &Path) -> Result<PathBuf, LiveGraphFileError> {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return Err(LiveGraphFileError::InvalidPath);
    };
    if name.starts_with('.') || !name.ends_with(&format!(".{EXTENSION}")) {
        return Err(LiveGraphFileError::InvalidPath);
    }
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Err(LiveGraphFileError::InvalidPath);
    };
    let parent = parent.canonicalize().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            LiveGraphFileError::InvalidPath
        } else {
            LiveGraphFileError::Io(error)
        }
    })?;
    if !parent.is_dir() {
        return Err(LiveGraphFileError::InvalidPath);
    }
    Ok(parent)
}

fn read_path(path: &Path) -> Result<Vec<u8>, LiveGraphFileError> {
    Ok(guarded_file::read_regular(path, MAX_GRAPH_BYTES)?.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);
    use runebender::font::variable::SourceId;
    use runebender::workflows::graph_live;
    use serde_json::json;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "runebender-live-graph-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn round_trips_intent_without_session_bindings() {
        let root = TestDirectory::new("roundtrip");
        let path = root.0.join("comparison.graph.json");
        let mut graph = graph_live::comparison_starter(SourceId(3));
        let python = graph
            .nodes
            .iter_mut()
            .find(|node| node.type_name == "live.python")
            .unwrap();
        python.pos = [321.0, 654.0];
        python
            .values
            .insert("code".into(), json!("print('saved')\n"));
        python
            .values
            .insert("parameters".into(), json!({"weight": 725}));
        let source = graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.font")
            .unwrap()
            .id;
        let fork = graph_live::add_direction(&mut graph, source, [336.0, 900.0]);
        graph
            .nodes
            .iter_mut()
            .find(|node| node.id == fork)
            .unwrap()
            .values
            .insert("branch".into(), json!(99));

        let saved = save(&path, &graph, None).expect("save graph");
        let saved_python = saved
            .graph
            .nodes
            .iter()
            .find(|node| node.type_name == "live.python")
            .unwrap();
        assert_eq!(saved_python.pos, [321.0, 654.0]);
        assert_eq!(saved_python.values["code"], "print('saved')\n");
        assert_eq!(saved_python.values["parameters"], json!({"weight": 725}));
        assert!(
            !saved
                .graph
                .nodes
                .iter()
                .find(|node| node.type_name == "live.font")
                .unwrap()
                .values
                .contains_key("source")
        );
        assert!(
            !saved
                .graph
                .nodes
                .iter()
                .find(|node| node.type_name == "live.fork")
                .unwrap()
                .values
                .contains_key("branch")
        );
        let raw = fs::read_to_string(path).unwrap();
        assert!(!raw.contains("session_id"));
        assert!(!raw.contains("semantic_revision"));
        assert!(!raw.contains("receipt"));
    }

    #[test]
    fn refuses_existing_paths_and_external_edits() {
        let root = TestDirectory::new("conflict");
        let path = root.0.join("comparison.graph.json");
        let graph = graph_live::comparison_starter(SourceId(0));
        let saved = save(&path, &graph, None).expect("save new graph");
        assert!(matches!(
            save(&path, &graph, None),
            Err(LiveGraphFileError::AlreadyExists)
        ));
        fs::write(&path, "external edit\n").unwrap();
        let error = save(&path, &graph, Some(&saved.metadata.revision)).unwrap_err();
        assert!(matches!(error, LiveGraphFileError::Conflict { .. }));
        assert_eq!(fs::read_to_string(path).unwrap(), "external edit\n");
    }

    #[test]
    fn rejects_unknown_authority_version_and_duplicate_ids() {
        let root = TestDirectory::new("identity");
        let path = root.0.join("comparison.graph.json");
        let graph = graph_live::comparison_starter(SourceId(0));
        let mut value = serde_json::to_value(&graph).unwrap();
        value["session_id"] = json!("stale-authority");
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(
            load(&path),
            Err(LiveGraphFileError::InvalidGraph(_))
        ));

        value.as_object_mut().unwrap().remove("session_id");
        value["version"] = json!(FILE_VERSION + 1);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(
            load(&path),
            Err(LiveGraphFileError::InvalidGraph(_))
        ));

        value["version"] = json!(FILE_VERSION);
        let duplicate = value["nodes"][0].clone();
        value["nodes"].as_array_mut().unwrap().push(duplicate);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(
            load(&path),
            Err(LiveGraphFileError::InvalidGraph(_))
        ));
    }
}
