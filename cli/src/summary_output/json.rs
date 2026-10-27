use std::{fs, path::Path};

use anyhow::Context;
use schemars::{JsonSchema, Schema, generate::SchemaSettings};
use serde::{Deserialize, Serialize};

use super::{ErrorSummary, UploadReport};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Report<'a> {
    /// The run finished and summarized its tests.
    Upload(&'a UploadReport),
    /// The run failed before it could summarize any tests.
    Error(&'a ErrorSummary),
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(
    title = "Trunk Analytics CLI summary",
    description = "What `upload` and `test` write to `--summary-output-file`, and the RSpec gem to `TRUNK_SUMMARY_OUTPUT_FILE`."
)]
struct Versioned<'a> {
    /// Goes up when a change could break a reader, such as renaming or removing a field.
    schema_version: u32,
    #[serde(flatten)]
    report: Report<'a>,
}

/// A summary read back from a file, owning its report.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Summary {
    Upload(Box<UploadReport>),
    Error(ErrorSummary),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaVersion {
    schema_version: u32,
}

pub fn schema() -> Schema {
    SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<Versioned<'static>>()
}

pub fn to_string(report: Report) -> serde_json::Result<String> {
    serde_json::to_string_pretty(&Versioned {
        schema_version: SCHEMA_VERSION,
        report,
    })
}

/// Reads a summary that this or an older CLI wrote. A newer schema version is an error, since its
/// fields may have changed meaning.
pub fn from_str(json: &str) -> anyhow::Result<Summary> {
    let SchemaVersion { schema_version } =
        serde_json::from_str(json).context("not a summary: no schemaVersion")?;
    if schema_version > SCHEMA_VERSION {
        anyhow::bail!(
            "schemaVersion {schema_version} is newer than this CLI understands ({SCHEMA_VERSION})"
        );
    }
    Ok(serde_json::from_str(json)?)
}

pub fn read_file(path: &Path) -> anyhow::Result<Summary> {
    let json =
        fs::read_to_string(path).with_context(|| format!("couldn't read {}", path.display()))?;
    from_str(&json).with_context(|| format!("couldn't read the summary in {}", path.display()))
}

/// Writes a sibling file and renames it into place, so a reader never sees half a summary.
/// Not `tempfile::NamedTempFile`, which would leave the summary readable only by its owner.
pub fn write_file(path: &Path, report: Report) -> anyhow::Result<()> {
    let json = to_string(report)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("{} is not a file path", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new(""));
    if !dir.as_os_str().is_empty() {
        fs::create_dir_all(dir)?;
    }
    let temp_path = dir.join(format!(
        ".{}.{}.tmp",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    let written = fs::write(&temp_path, json).and_then(|()| fs::rename(&temp_path, path));
    if written.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    Ok(written?)
}
