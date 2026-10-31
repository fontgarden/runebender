// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Specimen scripts: designer-written drawing code rendered against a compiled font.
//!
//! A script is Designbot Rust (the default) or `DrawBot` Python (the fallback).
//! `DrawBot` comes from `DRAWBOT_BIN`, the `[specimen]` config, `~/Runebender/python`, or `PATH`.
//! The renderer writes the compiled font to a temporary TTF, prepends `FONT_PATH` and
//! `FONT_FAMILY`, and runs the installed `designbot` or `drawbot` command with a time limit.
//! The temporary directory is removed on success or failure.
//!
//! Known gaps:
//! - Scriptes run with the designer's own permissions; they are trusted code, like a
//!   Python recipe, and are not sandboxed.
//! - Designbot compiles the script with Cargo. The first render on a machine takes minutes
//!   while its dependency cache warms; later edits recompile only the script.

use std::io::Read as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::designbot::{RendererIdentity, hash_executable, resolve_tool};

/// Largest script source accepted, in bytes.
pub const MAX_SCRIPT_BYTES: usize = 64 * 1024;
/// Largest image a script may produce, in bytes.
const MAX_SCRIPT_PNG_BYTES: u64 = 32 * 1024 * 1024;
/// Designbot compiles the script; a cold Cargo cache needs minutes.
const DESIGNBOT_TIME_LIMIT: Duration = Duration::from_secs(300);
/// `DrawBot` interprets the script.
const DRAWBOT_TIME_LIMIT: Duration = Duration::from_secs(60);
/// Diagnostics kept from a failed render.
const MAX_DIAGNOSTIC_BYTES: u64 = 4096;

/// The language a script is written in.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum ScriptLanguage {
    /// Designbot Rust, run by the `designbot` command.
    #[default]
    Designbot,
    /// `DrawBot` Python, run by the `drawbot` command from drawbot-skia.
    Drawbot,
}

impl ScriptLanguage {
    /// The language named by a node value, or Designbot.
    pub fn from_name(name: Option<&str>) -> Self {
        match name {
            Some(name) if name.eq_ignore_ascii_case("drawbot") => Self::Drawbot,
            _ => Self::Designbot,
        }
    }

    /// The node value naming this language.
    pub fn name(self) -> &'static str {
        match self {
            Self::Designbot => "designbot",
            Self::Drawbot => "drawbot",
        }
    }

    /// The starter script a new node shows in this language.
    pub fn starter(self) -> &'static str {
        match self {
            Self::Designbot => DESIGNBOT_STARTER,
            Self::Drawbot => DRAWBOT_STARTER,
        }
    }
}

/// Drawing code and its language, carried in a proof recipe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpecimenScript {
    /// Which renderer runs the code.
    #[serde(default)]
    pub language: ScriptLanguage,
    /// The script source, without the `FONT_PATH` and `FONT_FAMILY` preamble.
    pub code: String,
}

impl SpecimenScript {
    /// Reject an empty or oversized script.
    pub fn validate(&self) -> Result<(), String> {
        if self.code.trim().is_empty() {
            return Err("the script has no code".into());
        }
        if self.code.len() > MAX_SCRIPT_BYTES {
            return Err(format!("a script holds at most {MAX_SCRIPT_BYTES} bytes"));
        }
        Ok(())
    }
}

/// Renderer locations from the application config, used when no environment variable names one.
static CONFIGURED: std::sync::OnceLock<ConfiguredTools> = std::sync::OnceLock::new();

#[derive(Debug, Default)]
struct ConfiguredTools {
    designbot: Option<std::path::PathBuf>,
    drawbot: Option<std::path::PathBuf>,
}

/// Set where `designbot` and `drawbot` are, from the application config. Each path may name the
/// executable or a Python virtual environment holding it in `bin/`. Only the first call counts.
pub fn configure_tools(designbot: Option<std::path::PathBuf>, drawbot: Option<std::path::PathBuf>) {
    let _ = CONFIGURED.set(ConfiguredTools { designbot, drawbot });
}

/// The renderer for a language: its environment variable, then the config, then Runebender's
/// Python environment for `DrawBot`, then `PATH`.
fn renderer(language: ScriptLanguage) -> Result<std::path::PathBuf, String> {
    let (variable, name, label) = match language {
        ScriptLanguage::Designbot => ("DESIGNBOT_BIN", "designbot", "Designbot"),
        ScriptLanguage::Drawbot => ("DRAWBOT_BIN", "drawbot", "DrawBot"),
    };
    let configured = CONFIGURED.get().and_then(|tools| match language {
        ScriptLanguage::Designbot => tools.designbot.clone(),
        ScriptLanguage::Drawbot => tools.drawbot.clone(),
    });
    if std::env::var_os(variable).is_none()
        && let Some(path) = configured
    {
        // A virtual environment names its folder; its commands are in `bin/`.
        let path = if path.is_dir() {
            path.join("bin").join(name)
        } else {
            path
        };
        return if path.is_file() {
            Ok(path)
        } else {
            Err(format!(
                "{label} unavailable: the configured path {} is not a file",
                path.display()
            ))
        };
    }
    if std::env::var_os(variable).is_none()
        && language == ScriptLanguage::Drawbot
        && let Some(path) = crate::workflows::python_environment::command(name)
    {
        return Ok(path);
    }
    resolve_tool(variable, name, label)
}

/// Rendered script image and the renderer that drew it.
#[derive(Debug)]
pub struct RenderedSpecimenScript {
    /// PNG bytes.
    pub png: Vec<u8>,
    /// Entrypoint file identity for this invocation.
    pub renderer: RendererIdentity,
}

/// Render a script against compiled font bytes. Runs off the UI thread.
pub fn render(script: &SpecimenScript, font: &[u8]) -> Result<RenderedSpecimenScript, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    script.validate()?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "runebender-specimen-script-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).map_err(|error| error.to_string())?;
    struct Cleanup<'a>(&'a Path);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0);
        }
    }
    let _cleanup = Cleanup(&dir);
    let font_path = dir.join("font.ttf");
    std::fs::write(&font_path, font).map_err(|error| error.to_string())?;
    let family = family_name(font)?;
    let output = dir.join("specimen.png");
    let (label, executable, source, arguments, limit) = match script.language {
        ScriptLanguage::Designbot => {
            // Designbot keys its compile cache by the script's path, so every Designbot script
            // is written to one fixed path: an edit then recompiles only the script, not every
            // dependency. The lock below keeps two renders from sharing that file at once.
            let stable = std::env::temp_dir().join("runebender-designbot-specimen");
            std::fs::create_dir_all(&stable).map_err(|error| error.to_string())?;
            let source_path = stable.join("specimen.rs");
            let preamble = format!(
                "#[allow(dead_code)]\nconst FONT_PATH: &str = {};\n#[allow(dead_code)]\nconst FONT_FAMILY: &str = {};\n\n",
                rust_string(&font_path.to_string_lossy()),
                rust_string(&family),
            );
            (
                "Designbot",
                renderer(ScriptLanguage::Designbot)?,
                (source_path.clone(), preamble),
                vec![
                    "--render".into(),
                    source_path.into_os_string(),
                    "--output".into(),
                    output.clone().into_os_string(),
                    "--raw".into(),
                ],
                DESIGNBOT_TIME_LIMIT,
            )
        }
        ScriptLanguage::Drawbot => {
            let source_path = dir.join("specimen.py");
            let preamble = format!(
                "FONT_PATH = {}\nFONT_FAMILY = {}\n\n",
                python_string(&font_path.to_string_lossy()),
                python_string(&family),
            );
            (
                "DrawBot",
                renderer(ScriptLanguage::Drawbot).map_err(|error| {
                    format!(
                        "{error}; install drawbot-skia in ~/Runebender/python, or set drawbot under [specimen] in ~/.config/runebender/config.toml"
                    )
                })?,
                (source_path.clone(), preamble),
                vec![source_path.into_os_string(), output.clone().into_os_string()],
                DRAWBOT_TIME_LIMIT,
            )
        }
    };
    let (source_path, preamble) = source;
    static DESIGNBOT: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _exclusive = (script.language == ScriptLanguage::Designbot).then(|| {
        DESIGNBOT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    });
    std::fs::write(&source_path, format!("{preamble}{}", script.code))
        .map_err(|error| error.to_string())?;
    let executable_sha256 = hash_executable(&executable)?;
    let errors = dir.join("errors.txt");
    let mut child = Command::new(&executable)
        .args(&arguments)
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&errors).map_err(|error| error.to_string())?)
        .spawn()
        .map_err(|error| format!("{label} unavailable: {error}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) if status.success() => break,
            Some(_) => return Err(format!("{label} script failed: {}", diagnostics(&errors))),
            None if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{label} script exceeded {} seconds",
                    limit.as_secs()
                ));
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let size = std::fs::metadata(&output)
        .map_err(|_| format!("{label} script wrote no image: {}", diagnostics(&errors)))?
        .len();
    if size > MAX_SCRIPT_PNG_BYTES {
        return Err("the script image exceeds 32 MiB".into());
    }
    let png = std::fs::read(&output).map_err(|error| error.to_string())?;
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(format!("{label} script did not write a PNG"));
    }
    if hash_executable(&executable)? != executable_sha256 {
        return Err(format!("{label} executable changed during rendering"));
    }
    Ok(RenderedSpecimenScript {
        png,
        renderer: RendererIdentity {
            executable_path: executable.to_string_lossy().into_owned(),
            executable_sha256,
            command: match script.language {
                ScriptLanguage::Designbot => "--render specimen.rs --output specimen.png --raw",
                ScriptLanguage::Drawbot => "specimen.py specimen.png",
            }
            .into(),
        },
    })
}

/// The family name a renderer selects the compiled font by.
fn family_name(font: &[u8]) -> Result<String, String> {
    use skrifa::MetadataProvider as _;
    let font = skrifa::FontRef::new(font).map_err(|error| error.to_string())?;
    let name = |id| {
        font.localized_strings(id)
            .english_or_first()
            .map(|name| name.chars().collect::<String>())
            .filter(|name| !name.is_empty())
    };
    name(skrifa::string::StringId::TYPOGRAPHIC_FAMILY_NAME)
        .or_else(|| name(skrifa::string::StringId::FAMILY_NAME))
        .ok_or_else(|| "the compiled font has no family name".into())
}

/// The last diagnostics a failed renderer wrote.
fn diagnostics(path: &Path) -> String {
    let mut text = String::new();
    if let Ok(file) = std::fs::File::open(path) {
        let length = file.metadata().map_or(0, |metadata| metadata.len());
        let mut file = file;
        if length > MAX_DIAGNOSTIC_BYTES {
            use std::io::Seek as _;
            let _ = file.seek(std::io::SeekFrom::Start(length - MAX_DIAGNOSTIC_BYTES));
        }
        let mut bytes = Vec::new();
        let _ = file.take(MAX_DIAGNOSTIC_BYTES).read_to_end(&mut bytes);
        text = String::from_utf8_lossy(&bytes).trim().to_owned();
    }
    if text.is_empty() {
        "no diagnostics".into()
    } else {
        text
    }
}

fn rust_string(value: &str) -> String {
    format!("{value:?}")
}

fn python_string(value: &str) -> String {
    let mut quoted = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

/// The Designbot starter: a waterfall in the open font.
pub const DESIGNBOT_STARTER: &str = r#"use designbot::prelude::*;

fn main() {
    let (width, height) = (2048.0, 1024.0);
    let mut ctx = Canvas::new(width, height);
    let mut renderer = Renderer::new(width as u32, height as u32);
    renderer.load_font(FONT_PATH).expect("the open font");

    ctx.background(Color::rgb(10, 10, 10));
    ctx.fill(Color::rgb(245, 245, 245)).font(FONT_FAMILY);

    let sample = "The quick brown fox jumps over the lazy dog";
    let margin = width * 0.04;
    let mut y = height - margin * 2.0;
    for size in [96.0, 72.0, 60.0, 48.0, 36.0, 28.0, 24.0, 18.0, 14.0, 12.0] {
        if y < margin {
            break;
        }
        ctx.font_size(size).text(sample, margin, y);
        y -= size * 1.8;
    }

    renderer.render_to_png(&ctx, "specimen.png").unwrap();
}
"#;

/// The `DrawBot` starter: the same waterfall in Python.
pub const DRAWBOT_STARTER: &str = r#"WIDTH, HEIGHT = 2048, 1024
newPage(WIDTH, HEIGHT)

fill(0.04)
rect(0, 0, WIDTH, HEIGHT)

sample = "The quick brown fox jumps over the lazy dog"
margin = WIDTH * 0.04
y = HEIGHT - margin * 2.0
fill(0.96)
for size in [96, 72, 60, 48, 36, 28, 24, 18, 14, 12]:
    if y < margin:
        break
    font(FONT_PATH, size)
    text(sample, (margin, y))
    y -= size * 1.8
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_names_round_trip_and_default_to_designbot() {
        assert_eq!(ScriptLanguage::from_name(None), ScriptLanguage::Designbot);
        assert_eq!(
            ScriptLanguage::from_name(Some("DrawBot")),
            ScriptLanguage::Drawbot
        );
        for language in [ScriptLanguage::Designbot, ScriptLanguage::Drawbot] {
            assert_eq!(ScriptLanguage::from_name(Some(language.name())), language);
            assert!(!language.starter().is_empty());
        }
    }

    #[test]
    fn scriptes_reject_empty_and_oversized_code() {
        let mut script = SpecimenScript {
            language: ScriptLanguage::Designbot,
            code: "  \n".into(),
        };
        assert!(script.validate().is_err());
        script.code = "x".repeat(MAX_SCRIPT_BYTES + 1);
        assert!(script.validate().is_err());
        script.code = DESIGNBOT_STARTER.into();
        assert!(script.validate().is_ok());
    }

    /// Renders each starter with the installed tools. Set `RUNEBENDER_SCRIPT_FONT` to a TTF.
    #[test]
    #[ignore = "needs the designbot and drawbot commands and a font"]
    fn starters_render_with_the_installed_tools() {
        let font = std::fs::read(std::env::var_os("RUNEBENDER_SCRIPT_FONT").expect("a TTF path"))
            .expect("the font reads");
        for language in [ScriptLanguage::Designbot, ScriptLanguage::Drawbot] {
            let script = SpecimenScript {
                language,
                code: language.starter().into(),
            };
            match render(&script, &font) {
                Ok(rendered) => assert!(rendered.png.starts_with(b"\x89PNG")),
                Err(error) => panic!("{}: {error}", language.name()),
            }
        }
    }

    #[test]
    fn preamble_strings_escape_paths_and_names() {
        assert_eq!(rust_string("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(python_string("a\"b\\c"), r#""a\"b\\c""#);
    }
}
