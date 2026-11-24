// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The result of a headless command, and the one function that prints it.
//!
//! A command function computes and returns; it does not print and it does not exit.
//! The shell prints the result with [`emit`], and the disk agent tools read the JSON document
//! of a result in process with [`document`].

use std::fmt::Display;

use serde_json::{Value, json};

/// Exit codes, matching font-ml so a caller can branch on them.
pub(super) mod exit {
    /// Ran, and the answer is yes or the work is done.
    pub(crate) const OK: i32 = 0;
    /// The command was wrong: bad path, unknown glyph, missing flag.
    pub(crate) const USAGE: i32 = 2;
    /// The command was right and the tool it needs is not built yet.
    pub(crate) const NOT_BUILT: i32 = 3;
    /// The command was right and the work failed.
    pub(crate) const FAILED: i32 = 4;
}

/// What an error says: a line for a person and the `error` member of the JSON document.
#[derive(Debug)]
pub(super) struct ErrorDetail {
    text: String,
    value: Value,
}

impl ErrorDetail {
    /// An error that is one sentence in both forms.
    fn message(message: impl Display) -> Self {
        let text = message.to_string();
        Self {
            value: Value::String(text.clone()),
            text,
        }
    }

    /// An error with a structured JSON form, such as a tagged enum.
    fn typed(error: &(impl Display + serde::Serialize)) -> Self {
        Self {
            text: error.to_string(),
            value: json!(error),
        }
    }
}

/// Why a command did not produce a result.
///
/// Each variant has one exit code.
#[derive(Debug)]
pub(super) enum CliError {
    /// The command was wrong: bad path, unknown glyph, missing flag.
    Usage(ErrorDetail),
    /// The command was right and the tool it needs is not built yet.
    NotBuilt(ErrorDetail),
    /// The command was right and the work failed.
    Failed(ErrorDetail),
}

impl CliError {
    /// A usage error with a plain message.
    pub(super) fn usage(message: impl Display) -> Self {
        Self::Usage(ErrorDetail::message(message))
    }

    /// A missing-tool error with a plain message.
    pub(super) fn not_built(message: impl Display) -> Self {
        Self::NotBuilt(ErrorDetail::message(message))
    }

    /// A failure with a plain message.
    pub(super) fn failed(message: impl Display) -> Self {
        Self::Failed(ErrorDetail::message(message))
    }

    /// A usage error that keeps the structured form of `error` in the JSON document.
    pub(super) fn usage_typed(error: &(impl Display + serde::Serialize)) -> Self {
        Self::Usage(ErrorDetail::typed(error))
    }

    /// A failure that keeps the structured form of `error` in the JSON document.
    pub(super) fn failed_typed(error: &(impl Display + serde::Serialize)) -> Self {
        Self::Failed(ErrorDetail::typed(error))
    }

    fn detail(&self) -> &ErrorDetail {
        match self {
            Self::Usage(detail) | Self::NotBuilt(detail) | Self::Failed(detail) => detail,
        }
    }

    /// The exit code of the variant.
    pub(super) fn code(&self) -> i32 {
        match self {
            Self::Usage(_) => exit::USAGE,
            Self::NotBuilt(_) => exit::NOT_BUILT,
            Self::Failed(_) => exit::FAILED,
        }
    }

    /// The JSON document `--json` prints for this error.
    pub(super) fn document(&self) -> Value {
        json!({ "ok": false, "error": self.detail().value })
    }
}

/// What a finished command prints and the code it exits with.
///
/// A command can finish and still report a failure in its document, as `graph check` does
/// for a file with problems; `code` carries the exit code for that case.
#[derive(Debug)]
pub(super) struct CommandOutput {
    /// The JSON document, printed on one line with `--json`.
    pub(super) value: Value,
    /// Standard output without `--json`, with its line ends.
    pub(super) text: String,
    /// Standard error without `--json`, with its line ends.
    pub(super) diagnostics: String,
    /// The exit code.
    pub(super) code: i32,
}

impl CommandOutput {
    /// A successful result with a JSON form and a text form.
    pub(super) fn new(value: Value, text: String) -> Self {
        Self {
            value,
            text,
            diagnostics: String::new(),
            code: exit::OK,
        }
    }

    /// A successful result that prints its JSON document with or without `--json`.
    pub(super) fn json(value: Value) -> Self {
        let text = format!("{value}\n");
        Self::new(value, text)
    }

    /// The same result with another exit code.
    pub(super) fn with_code(self, code: i32) -> Self {
        Self { code, ..self }
    }

    /// The same result with text for standard error.
    pub(super) fn with_diagnostics(self, diagnostics: String) -> Self {
        Self {
            diagnostics,
            ..self
        }
    }
}

/// The result of every command function.
pub(super) type CommandResult = Result<CommandOutput, CliError>;

/// The JSON document of a result, as an agent tool returns it.
pub(super) fn document(result: CommandResult) -> Value {
    match result {
        Ok(output) => output.value,
        Err(error) => error.document(),
    }
}

/// Prints a result and returns the exit code.
///
/// With `json`, the document goes to standard output, for a success or an error.
/// Without it, text goes to standard output and errors and diagnostics to standard error.
pub(super) fn emit(json: bool, result: CommandResult) -> i32 {
    match result {
        Ok(output) => {
            if json {
                println!("{}", output.value);
            } else {
                print!("{}", output.text);
                eprint!("{}", output.diagnostics);
            }
            output.code
        }
        Err(error) => {
            if json {
                println!("{}", error.document());
            } else {
                eprintln!("{}", error.detail().text);
            }
            error.code()
        }
    }
}

/// Appends one formatted line to a `String`.
macro_rules! push_line {
    ($text:expr, $($arg:tt)*) => {{
        use std::fmt::Write as _;
        writeln!($text, $($arg)*).expect("writing to a String does not fail");
    }};
}
pub(super) use push_line;

#[cfg(test)]
mod tests {
    use super::*;

    /// Exit codes are the interface for a script, so they are pinned.
    #[test]
    fn exit_codes_are_distinct() {
        let codes = [exit::OK, exit::USAGE, exit::NOT_BUILT, exit::FAILED];
        let mut sorted = codes.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "exit codes must not collide");
        assert_eq!(exit::OK, 0, "0 must mean success");
    }

    #[test]
    fn each_error_variant_has_its_exit_code() {
        assert_eq!(CliError::usage("x").code(), exit::USAGE);
        assert_eq!(CliError::not_built("x").code(), exit::NOT_BUILT);
        assert_eq!(CliError::failed("x").code(), exit::FAILED);
    }

    #[test]
    fn an_error_document_keeps_a_structured_error() {
        let error = runebender::font::proposal::ProposalError::NoProposal {
            task: "bolden".into(),
        };
        let document = CliError::usage_typed(&error).document();
        assert_eq!(document["ok"], false);
        assert_eq!(document["error"]["kind"], "no_proposal");
        assert_eq!(CliError::failed("broken").document()["error"], "broken");
    }

    #[test]
    fn a_json_only_result_prints_its_document_as_text() {
        let output = CommandOutput::json(json!({"ok": true}));
        assert_eq!(output.text, "{\"ok\":true}\n");
        assert_eq!(output.code, exit::OK);
    }
}
