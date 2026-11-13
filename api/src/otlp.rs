//! The upload's own telemetry as one OTLP JSON log record, for public-api's CLI telemetry route
//! (`/v2/telemetry/v1/logs`, TRUNK-23773). The route keeps only the attribute names listed for
//! `trunk-analytics-cli` in trunk2's `ts/apps/public-api` and stamps the org itself, so a name
//! added here is dropped until it is added there too.

use proto::upload_metrics::trunk::{
    QuarantineQueryResult, QuarantineResolutionMode, UploadMetrics,
};
use serde_json::{Value, json};

pub const CLI_TELEMETRY_PATH: &str = "/v2/telemetry/v1/logs";
pub const SERVICE_NAME: &str = "trunk-analytics-cli";
pub const COMMAND: &str = "upload";

pub const COMMAND_ATTRIBUTE: &str = "trunk.cli.command";
pub const DURATION_ATTRIBUTE: &str = "trunk.cli.duration_ms";
pub const FAILED_ATTRIBUTE: &str = "trunk.analytics_cli.upload.failed";
pub const FAILURE_REASON_ATTRIBUTE: &str = "trunk.analytics_cli.upload.failure_reason";
pub const UPLOAD_DURATION_ATTRIBUTE: &str = "trunk.analytics_cli.upload.duration_ms";
pub const QUARANTINE_QUERY_RESULT_ATTRIBUTE: &str = "trunk.analytics_cli.quarantine.query_result";
pub const QUARANTINE_RESOLUTION_MODE_ATTRIBUTE: &str =
    "trunk.analytics_cli.quarantine.resolution_mode";
pub const REPOSITORY_HOST_ATTRIBUTE: &str = "trunk.vcs.repository.host";
pub const REPOSITORY_OWNER_ATTRIBUTE: &str = "trunk.vcs.repository.owner";
pub const REPOSITORY_NAME_ATTRIBUTE: &str = "trunk.vcs.repository.name";

/// OTLP's `SEVERITY_NUMBER_INFO` and `SEVERITY_NUMBER_ERROR`.
const SEVERITY_INFO: i32 = 9;
const SEVERITY_ERROR: i32 = 17;

const QUARANTINE_QUERY_RESULT_PREFIX: &str = "QUARANTINE_QUERY_RESULT_";
const QUARANTINE_RESOLUTION_MODE_PREFIX: &str = "QUARANTINE_RESOLUTION_MODE_";

/// OTLP JSON writes 64-bit integers as decimal strings.
pub fn upload_metrics_logs(metrics: &UploadMetrics, environment: &str) -> Value {
    let version = metrics
        .client_version
        .as_ref()
        .map(|version| {
            let base = format!("{}.{}.{}", version.major, version.minor, version.patch);
            if version.suffix.is_empty() {
                base
            } else {
                format!("{base}-{}", version.suffix)
            }
        })
        .unwrap_or_default();
    let resource_attributes = [
        string_attribute("service.name", SERVICE_NAME),
        string_attribute("service.version", &version),
        string_attribute("deployment.environment.name", environment),
        string_attribute("os.type", os_type(std::env::consts::OS)),
        string_attribute("host.arch", host_arch(std::env::consts::ARCH)),
    ];

    let mut attributes = vec![
        string_attribute(COMMAND_ATTRIBUTE, COMMAND),
        bool_attribute(FAILED_ATTRIBUTE, metrics.failed),
        string_attribute(
            QUARANTINE_QUERY_RESULT_ATTRIBUTE,
            &enum_name(
                QuarantineQueryResult::try_from(metrics.quarantine_query_result)
                    .unwrap_or_default()
                    .as_str_name(),
                QUARANTINE_QUERY_RESULT_PREFIX,
            ),
        ),
        string_attribute(
            QUARANTINE_RESOLUTION_MODE_ATTRIBUTE,
            &enum_name(
                QuarantineResolutionMode::try_from(metrics.quarantine_resolution_mode)
                    .unwrap_or_default()
                    .as_str_name(),
                QUARANTINE_RESOLUTION_MODE_PREFIX,
            ),
        ),
    ];
    if !metrics.failure_reason.is_empty() {
        attributes.push(string_attribute(
            FAILURE_REASON_ATTRIBUTE,
            &metrics.failure_reason,
        ));
    }
    let finished = metrics
        .upload_finished_at
        .as_ref()
        .map(|time| unix_nanos(time.seconds, time.nanos));
    let cli_started = metrics
        .cli_started_at
        .as_ref()
        .map(|time| unix_nanos(time.seconds, time.nanos));
    let upload_started = metrics
        .upload_started_at
        .as_ref()
        .map(|time| unix_nanos(time.seconds, time.nanos));
    for (key, started) in [
        (DURATION_ATTRIBUTE, cli_started),
        (UPLOAD_DURATION_ATTRIBUTE, upload_started),
    ] {
        if let (Some(started), Some(finished)) = (started, finished) {
            attributes.push(int_attribute(key, millis_between(started, finished)));
        }
    }
    if let Some(repo) = &metrics.repo {
        for (key, value) in [
            (REPOSITORY_HOST_ATTRIBUTE, &repo.host),
            (REPOSITORY_OWNER_ATTRIBUTE, &repo.owner),
            (REPOSITORY_NAME_ATTRIBUTE, &repo.name),
        ] {
            if !value.is_empty() {
                attributes.push(string_attribute(key, value));
            }
        }
    }

    let (severity_number, severity_text) = if metrics.failed {
        (SEVERITY_ERROR, "ERROR")
    } else {
        (SEVERITY_INFO, "INFO")
    };

    json!({
        "resourceLogs": [{
            "resource": { "attributes": resource_attributes },
            "scopeLogs": [{
                "scope": { "name": SERVICE_NAME, "version": version },
                "logRecords": [{
                    "timeUnixNano": finished.unwrap_or_default().to_string(),
                    "severityNumber": severity_number,
                    "severityText": severity_text,
                    "body": { "stringValue": format!("{SERVICE_NAME} {COMMAND}") },
                    "attributes": attributes,
                }],
            }],
        }],
    })
}

fn enum_name(name: &str, prefix: &str) -> String {
    name.strip_prefix(prefix).unwrap_or(name).to_lowercase()
}

fn unix_nanos(seconds: i64, nanos: i32) -> i128 {
    i128::from(seconds) * 1_000_000_000 + i128::from(nanos)
}

fn millis_between(started: i128, finished: i128) -> i64 {
    i64::try_from((finished - started).max(0) / 1_000_000).unwrap_or(i64::MAX)
}

fn string_attribute(key: &str, value: &str) -> Value {
    json!({ "key": key, "value": { "stringValue": value } })
}

fn bool_attribute(key: &str, value: bool) -> Value {
    json!({ "key": key, "value": { "boolValue": value } })
}

fn int_attribute(key: &str, value: i64) -> Value {
    json!({ "key": key, "value": { "intValue": value.to_string() } })
}

fn os_type(os: &'static str) -> &'static str {
    match os {
        "macos" => "darwin",
        other => other,
    }
}

/// OpenTelemetry semantic-convention names, as the trunk CLI sends them.
fn host_arch(arch: &'static str) -> &'static str {
    match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_names_lose_their_prefix() {
        assert_eq!(
            enum_name(
                "QUARANTINE_QUERY_RESULT_SUCCESS",
                QUARANTINE_QUERY_RESULT_PREFIX
            ),
            "success"
        );
        assert_eq!(
            enum_name(
                "QUARANTINE_RESOLUTION_MODE_TEST_COLLECTION",
                QUARANTINE_RESOLUTION_MODE_PREFIX
            ),
            "test_collection"
        );
    }

    #[test]
    fn a_clock_that_ran_backwards_reads_as_zero() {
        assert_eq!(millis_between(5_000_000, 1_000_000), 0);
        assert_eq!(millis_between(0, 2_500_000), 2);
    }
}
