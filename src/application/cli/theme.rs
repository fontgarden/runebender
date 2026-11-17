// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Theme file creation, validation, and listing.

use super::*;

#[derive(clap::Subcommand)]
pub(super) enum ThemeAction {
    /// List built-in and locally installed themes.
    List,
    /// Check a theme file and report its resolved identity.
    Validate {
        /// A .theme.toml file.
        file: PathBuf,
    },
    /// Copy a built-in theme into a new, editable file.
    Init(ThemeInitArgs),
}

/// Arguments of `theme init`.
#[derive(clap::Args)]
pub(super) struct ThemeInitArgs {
    /// Built-in starting theme; use "theme list" to see the available IDs.
    #[arg(long, default_value = "gray")]
    from: String,
    /// Stable identifier for the new theme.
    #[arg(long)]
    id: String,
    /// Human-readable display name.
    #[arg(long)]
    name: String,
    /// Destination .theme.toml file; must not already exist.
    #[arg(long)]
    out: PathBuf,
}

/// `theme`: one of its three actions.
pub(super) fn theme_command(action: &ThemeAction) -> CommandResult {
    match action {
        ThemeAction::List => theme_list(),
        ThemeAction::Validate { file } => theme_validate(file),
        ThemeAction::Init(args) => theme_init(args),
    }
}

fn theme_list() -> CommandResult {
    let catalog = crate::application::platform::themes::catalog();
    let themes: Vec<_> = catalog
        .ids()
        .iter()
        .filter_map(|id| catalog.get(id).map(|theme| (&theme.id, &theme.name)))
        .collect();
    let mut text = String::new();
    for (id, name) in &themes {
        push_line!(text, "{id}\t{name}");
    }
    let themes = themes
        .iter()
        .map(|(id, name)| json!({"id": id, "name": name}))
        .collect::<Vec<_>>();
    Ok(CommandOutput::new(
        json!({"ok": true, "themes": themes}),
        text,
    ))
}

fn theme_validate(file: &Path) -> CommandResult {
    let theme = std::fs::read_to_string(file)
        .map_err(|error| error.to_string())
        .and_then(|source| runebender::ui::theme::parse_theme(&source))
        .map_err(|error| CliError::usage(format!("{}: {error}", file.display())))?;
    Ok(CommandOutput::new(
        json!({"ok": true, "id": theme.id, "name": theme.name}),
        format!("Valid theme: {} ({})\n", theme.name, theme.id),
    ))
}

/// The text of a new theme: a built-in theme with a new id and name.
///
/// A bad id, name, output name or starting theme is a usage error.
/// A built-in theme that does not parse, or that cannot take the new identity, is a failure.
fn theme_init_text(args: &ThemeInitArgs) -> Result<String, CliError> {
    use runebender::ui::theme::{builtin_theme_source, parse_theme, validate_theme_id};
    let (from, id, name) = (&args.from, &args.id, &args.name);
    validate_theme_id(id).map_err(CliError::usage)?;
    if name.trim().is_empty() {
        return Err(CliError::usage("theme name must not be empty"));
    }
    if !args
        .out
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".theme.toml"))
    {
        return Err(CliError::usage("theme output must end in .theme.toml"));
    }
    let source = builtin_theme_source(from).map_err(CliError::usage)?;
    let base_name = parse_theme(source).map_err(CliError::failed)?.name;
    let id_value = toml::Value::String(id.clone()).to_string();
    let name_value = toml::Value::String(name.clone()).to_string();
    let text = source
        .replacen(&format!("id = \"{from}\""), &format!("id = {id_value}"), 1)
        .replacen(
            &format!("name = \"{base_name}\""),
            &format!("name = {name_value}"),
            1,
        );
    let theme = parse_theme(&text).map_err(CliError::failed)?;
    if theme.id != *id || theme.name != *name {
        return Err(CliError::failed("could not set the new theme id and name"));
    }
    Ok(text)
}

/// Write a new theme file; an output that cannot be created is a usage error.
fn theme_init(args: &ThemeInitArgs) -> CommandResult {
    use std::io::Write as _;
    let text = theme_init_text(args)?;
    let out = &args.out;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(out)
        .map_err(CliError::usage)?;
    file.write_all(text.as_bytes()).map_err(CliError::failed)?;
    Ok(CommandOutput::new(
        json!({"ok": true, "id": args.id, "name": args.name, "output": out}),
        format!("Created {}\n", out.display()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(from: &str, id: &str, name: &str, out: &str) -> ThemeInitArgs {
        ThemeInitArgs {
            from: from.into(),
            id: id.into(),
            name: name.into(),
            out: out.into(),
        }
    }

    /// A caller's mistake is a usage error; only the built-in theme itself can fail the work.
    #[test]
    fn theme_init_reports_mistakes_as_usage() {
        let text = theme_init_text(&args("gray", "base-test", "Base Test", "a.theme.toml"))
            .expect("a built-in theme takes a new identity");
        assert!(text.contains("id = \"base-test\""));
        for wrong in [
            args("gray", "Bad Id", "x", "a.theme.toml"),
            args("gray", "good", " ", "a.theme.toml"),
            args("gray", "good", "x", "a.toml"),
            args("nosuch", "good", "x", "a.theme.toml"),
        ] {
            let error = theme_init_text(&wrong).expect_err("a mistake is rejected");
            assert_eq!(error.code(), exit::USAGE);
        }
    }
}
