// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Launch options: the `RUNEBENDER_*` environment variables of the editor, parsed once.
//!
//! [`LaunchOptions::from_env`] is the only place where the editor reads these variables.
//! The rest of the application reads the typed fields of [`LaunchOptions::current`].
//! [`VARIABLES`] names each variable with its help text, and a test compares that table with the
//! table in `docs/UI-DESIGN.md`.

use crate::application::widgets::frame_stats;
use std::ffi::OsString;
use std::sync::OnceLock;

/// One environment variable that the editor reads at launch.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "only the documentation test reads the value syntax and the help text"
    )
)]
pub(crate) struct Variable {
    /// The name of the variable.
    pub(crate) name: &'static str,
    /// The value syntax that the documentation shows after the name.
    pub(crate) value: &'static str,
    /// What the variable does, as one line of documentation.
    pub(crate) help: &'static str,
}

const fn variable(name: &'static str, value: &'static str, help: &'static str) -> Variable {
    Variable { name, value, help }
}

/// Every variable that [`LaunchOptions`] holds, in the order of the documentation.
pub(crate) const VARIABLES: &[Variable] = &[
    variable(
        "RUNEBENDER_SCREENSHOT",
        "<path>",
        "Render one frame to this PNG file and exit without a window.",
    ),
    variable(
        "RUNEBENDER_SIZE",
        "1100x720",
        "Logical size of the capture; the default is 1100 by 720.",
    ),
    variable(
        "RUNEBENDER_SCALE",
        "2",
        "Device-pixel scale of the capture at the same logical layout.",
    ),
    variable(
        "RUNEBENDER_FRAME_STATS",
        "1",
        "In a window, render every frame and report the frame cadence on stderr.",
    ),
];

/// Where one headless capture goes and how large it is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CaptureOptions {
    /// The PNG file to write.
    pub(crate) path: String,
    /// The logical size of the frame.
    pub(crate) size: (u32, u32),
    /// The device-pixel scale at that logical size.
    pub(crate) scale: f64,
}

/// The values of the `RUNEBENDER_*` variables at launch.
pub(crate) struct LaunchOptions {
    /// Render one frame to a file and exit, instead of opening a window.
    pub(crate) capture: Option<CaptureOptions>,
    /// Render every frame of the window and report the cadence on stderr.
    pub(crate) frame_stats: bool,
}

/// A source of variable values: the process environment, or a table in a test.
struct Variables<'a>(&'a dyn Fn(&str) -> Option<OsString>);

impl Variables<'_> {
    /// The value as `std::env::var_os` returns it.
    fn os(&self, name: &str) -> Option<OsString> {
        debug_assert!(
            VARIABLES.iter().any(|variable| variable.name == name),
            "{name} has no row in VARIABLES"
        );
        (self.0)(name)
    }

    /// The value as `std::env::var` returns it: a value that is not Unicode counts as unset.
    fn text(&self, name: &str) -> Option<String> {
        self.os(name)?.into_string().ok()
    }

    /// Whether the variable is set to any value, Unicode or not.
    fn is_set(&self, name: &str) -> bool {
        self.os(name).is_some()
    }
}

impl LaunchOptions {
    /// The options of this process, read from the environment on the first call.
    pub(crate) fn current() -> &'static Self {
        static CURRENT: OnceLock<LaunchOptions> = OnceLock::new();
        CURRENT.get_or_init(Self::from_env)
    }

    /// Parse the options from the process environment.
    pub(crate) fn from_env() -> Self {
        Self::from_variables(&Variables(&|name| std::env::var_os(name)))
    }

    fn from_variables(variables: &Variables<'_>) -> Self {
        let capture = variables
            .text("RUNEBENDER_SCREENSHOT")
            .map(|path| CaptureOptions {
                path,
                size: variables
                    .text("RUNEBENDER_SIZE")
                    .and_then(|spec| {
                        let (width, height) = spec.split_once('x')?;
                        Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
                    })
                    .unwrap_or((1100, 720)),
                scale: variables
                    .text("RUNEBENDER_SCALE")
                    .and_then(|value| value.parse::<f64>().ok())
                    .filter(|value| value.is_finite() && *value > 0.0)
                    .unwrap_or(1.0),
            });
        Self {
            capture,
            frame_stats: variables.is_set(frame_stats::ENV),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;

    /// Parse the options from name and value pairs instead of the environment.
    fn options(pairs: &[(&str, &str)]) -> LaunchOptions {
        LaunchOptions::from_variables(&Variables(&|name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }))
    }

    fn documentation() -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/UI-DESIGN.md");
        std::fs::read_to_string(path).expect("docs/UI-DESIGN.md is readable")
    }

    /// The table rows under one heading of the documentation.
    fn table_rows(documentation: &str, heading: &str) -> Vec<String> {
        documentation
            .lines()
            .skip_while(|line| *line != heading)
            .skip(1)
            .take_while(|line| !line.starts_with('#'))
            .filter(|line| line.starts_with("| `RUNEBENDER_"))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn the_variable_table_matches_the_documentation() {
        let expected: Vec<String> = VARIABLES
            .iter()
            .map(|variable| {
                format!(
                    "| `{}={}` | {} |",
                    variable.name,
                    variable.value.replace('|', "\\|"),
                    variable.help
                )
            })
            .collect();
        let documented = table_rows(&documentation(), "### Launch variables");
        assert_eq!(
            documented, expected,
            "the Launch variables table in docs/UI-DESIGN.md must repeat VARIABLES row for row"
        );
    }

    #[test]
    fn variable_names_are_unique() {
        let names: BTreeSet<_> = VARIABLES.iter().map(|variable| variable.name).collect();
        assert_eq!(names.len(), VARIABLES.len());
    }

    #[test]
    fn every_variable_in_the_source_is_documented() {
        fn collect(directory: &Path, names: &mut BTreeSet<String>) {
            for entry in std::fs::read_dir(directory).expect("the source tree is readable") {
                let path = entry.expect("a directory entry is readable").path();
                if path.is_dir() {
                    collect(&path, names);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    let text = std::fs::read_to_string(&path).expect("a source file is readable");
                    for (start, _) in text.match_indices("RUNEBENDER_") {
                        let name: String = text[start..]
                            .chars()
                            .take_while(|c| {
                                c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_'
                            })
                            .collect();
                        // A name that ends with an underscore is a prefix in prose, not a variable.
                        if !name.ends_with('_') {
                            names.insert(name);
                        }
                    }
                }
            }
        }

        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut names = BTreeSet::new();
        for directory in ["src", "benches"] {
            collect(&root.join(directory), &mut names);
        }
        let documentation = documentation();
        let documented: BTreeSet<String> = [
            "### Launch variables",
            "### Variables that other code reads",
        ]
        .into_iter()
        .flat_map(|heading| table_rows(&documentation, heading))
        .filter_map(|row| {
            let name = row.strip_prefix("| `")?;
            let end = name.find(['=', '`'])?;
            Some(name[..end].to_owned())
        })
        .collect();
        let missing: Vec<_> = names.difference(&documented).collect();
        assert!(
            missing.is_empty(),
            "docs/UI-DESIGN.md has no row for {missing:?}"
        );
        let stale: Vec<_> = documented.difference(&names).collect();
        assert!(
            stale.is_empty(),
            "docs/UI-DESIGN.md documents {stale:?}, which the source does not name"
        );
    }

    #[test]
    fn no_variable_gives_a_window_and_no_capture() {
        let options = options(&[]);
        assert_eq!(options.capture, None);
        assert!(!options.frame_stats);
    }

    #[test]
    fn a_capture_reads_its_size_and_scale() {
        let capture = options(&[("RUNEBENDER_SCREENSHOT", "/tmp/frame.png")]).capture;
        assert_eq!(
            capture,
            Some(CaptureOptions {
                path: "/tmp/frame.png".into(),
                size: (1100, 720),
                scale: 1.0,
            })
        );
        let capture = options(&[
            ("RUNEBENDER_SCREENSHOT", "/tmp/frame.png"),
            ("RUNEBENDER_SIZE", " 1500 x 2200 "),
            ("RUNEBENDER_SCALE", "2"),
        ])
        .capture
        .expect("the capture path is set");
        assert_eq!(capture.size, (1500, 2200));
        assert_eq!(capture.scale, 2.0);
        // A size or scale that does not parse falls back to the default.
        let capture = options(&[
            ("RUNEBENDER_SCREENSHOT", "/tmp/frame.png"),
            ("RUNEBENDER_SIZE", "wide"),
            ("RUNEBENDER_SCALE", "-1"),
        ])
        .capture
        .expect("the capture path is set");
        assert_eq!(capture.size, (1100, 720));
        assert_eq!(capture.scale, 1.0);
    }

    #[test]
    fn frame_statistics_need_only_a_set_variable() {
        assert!(options(&[("RUNEBENDER_FRAME_STATS", "")]).frame_stats);
    }
}
