//! Every exit code the CLI can return, each documented once. The "Exit codes" section of
//! `--help` and the summary's JSON Schema (`summary-output-schema`) are generated from the doc
//! comments below, so they are written for users.

use std::{borrow::Cow, fmt};

use clap::builder::{StyledStr, styling::Styles};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Serialize, Serializer};
use strum::{EnumIter, EnumMessage, IntoEnumIterator};

/// Two exit codes are equal when the process exits with the same number, so
/// `PassedThrough(1) == Failure`.
#[derive(Debug, Clone, Copy, EnumIter, EnumMessage)]
pub enum ExitCode {
    /// Tests passed, every failure was quarantined, or validate found no errors. upload also
    /// exits 0 when tests failed if quarantining is turned off (by --disable-quarantining or
    /// for the repo in Trunk) or no test reports were found. Errors connecting to Trunk, and
    /// errors the CLI doesn't recognize, also exit 0 so that they don't fail your CI job.
    Success,
    /// Tests failed and weren't quarantined, and upload wasn't given --test-process-exit-code.
    /// Also returned when validate found errors, or when test's command ended without an exit
    /// code (e.g. it was killed by a signal).
    Failure,
    /// test exits with its command's exit code, and upload with --test-process-exit-code's
    /// value, unless every failure was quarantined. test does this even when uploading the
    /// results fails.
    PassedThrough(i32),
    /// Trunk rejected the token or organization slug, the repo root isn't a git repository, or
    /// the run stopped early (e.g. no test reports were found with
    /// --allow-empty-test-results=false).
    Error,
    /// The CLI was interrupted (SIGINT, or Ctrl-C on Windows).
    Interrupted,
    /// The CLI was terminated (SIGTERM), e.g. because the CI job was canceled.
    Terminated,
    /// The command line was invalid, e.g. an unknown or missing argument.
    Usage,
    /// The CLI crashed. Please report this to us.
    Panic,
}

impl ExitCode {
    pub const fn code(self) -> i32 {
        match self {
            Self::Success => 0,
            Self::Failure => 1,
            Self::PassedThrough(code) => code,
            Self::Error => 70,
            // 130 and 143 follow the shell convention for a process killed by a signal: 128
            // plus the signal's number, which is 2 for SIGINT and 15 for SIGTERM.
            Self::Interrupted => 130,
            Self::Terminated => 143,
            Self::Usage => 2,
            Self::Panic => 101,
        }
    }

    pub fn is_success(self) -> bool {
        self.code() == Self::Success.code()
    }

    /// The "Exit codes" section of `--help`, laid out like clap's own long help.
    pub fn help() -> StyledStr {
        let styles = Styles::default();
        let header = styles.get_header();
        let literal = styles.get_literal();
        let mut help = format!("{header}Exit codes:{header:#}\n");
        for (label, lines) in Self::docs() {
            help.push_str(&format!("  {literal}{label}{literal:#}\n"));
            for line in lines {
                help.push_str(&format!("          {line}\n"));
            }
            help.push('\n');
        }
        help.truncate(help.trim_end().len());
        help.into()
    }

    /// The description of `exitCode` in the summary's JSON Schema, as a Markdown list.
    fn schema_description() -> String {
        let mut description = String::from("The code the CLI exits with:\n");
        for (label, lines) in Self::docs() {
            let label = if label.parse::<i32>().is_ok() {
                format!("`{label}`")
            } else {
                label
            };
            let text = lines.collect::<Vec<_>>().join(" ");
            description.push_str(&format!("\n- {label}: {text}"));
        }
        description
    }

    /// Each code's label and the lines of its doc comment.
    fn docs() -> impl Iterator<Item = (String, impl Iterator<Item = &'static str>)> {
        Self::iter().map(|exit_code| {
            let label = match exit_code {
                Self::PassedThrough(_) => String::from("Passed through"),
                _ => exit_code.code().to_string(),
            };
            let lines = exit_code.get_documentation().unwrap_or_default().lines();
            (label, lines.map(str::trim))
        })
    }
}

impl PartialEq for ExitCode {
    fn eq(&self, other: &Self) -> bool {
        self.code() == other.code()
    }
}

impl Eq for ExitCode {}

impl From<ExitCode> for i32 {
    fn from(exit_code: ExitCode) -> Self {
        exit_code.code()
    }
}

impl fmt::Display for ExitCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())
    }
}

impl Serialize for ExitCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i32(self.code())
    }
}

impl JsonSchema for ExitCode {
    fn schema_name() -> Cow<'static, str> {
        "ExitCode".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "integer",
            "description": Self::schema_description(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes() {
        assert_eq!(ExitCode::Success.code(), 0);
        assert_eq!(ExitCode::Failure.code(), 1);
        assert_eq!(ExitCode::PassedThrough(3).code(), 3);
        assert_eq!(ExitCode::Error.code(), 70);
        assert_eq!(ExitCode::Interrupted.code(), 130);
        assert_eq!(ExitCode::Terminated.code(), 143);
        assert_eq!(ExitCode::Usage.code(), 2);
        assert_eq!(ExitCode::Panic.code(), 101);
    }

    #[test]
    fn exit_codes_are_equal_when_their_numbers_are() {
        assert_eq!(ExitCode::PassedThrough(0), ExitCode::Success);
        assert_eq!(ExitCode::PassedThrough(1), ExitCode::Failure);
        assert_ne!(ExitCode::PassedThrough(2), ExitCode::Failure);
        assert!(ExitCode::PassedThrough(0).is_success());
    }

    #[test]
    fn serializes_as_its_number() {
        assert_eq!(serde_json::to_string(&ExitCode::Error).unwrap(), "70");
        assert_eq!(
            serde_json::to_string(&ExitCode::PassedThrough(3)).unwrap(),
            "3"
        );
    }

    #[test]
    fn help_lists_every_code() {
        let help = ExitCode::help().to_string();
        for label in ["0", "1", "Passed through", "70", "130", "143", "2", "101"] {
            assert!(
                help.lines().any(|line| line.trim() == label),
                "{label} missing from:\n{help}"
            );
        }
    }

    #[test]
    fn schema_description_lists_every_code() {
        let description = ExitCode::schema_description();
        for label in [
            "`0`",
            "`1`",
            "Passed through",
            "`70`",
            "`130`",
            "`143`",
            "`2`",
            "`101`",
        ] {
            assert!(
                description.contains(&format!("\n- {label}: ")),
                "{label} missing from:\n{description}"
            );
        }
    }
}
