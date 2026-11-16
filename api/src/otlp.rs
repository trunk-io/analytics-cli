//! The upload's own telemetry as OTLP JSON metrics, for public-api's CLI telemetry metrics route
//! (`/v2/telemetry/v1/metrics`, TRUNK-23773). The route keeps only the metric and attribute names
//! it lists for `trunk-analytics-cli` in trunk2's `ts/apps/public-api/src/products/telemetry` and
//! stamps the org itself, so a name added here is dropped until it is added there too. Names,
//! labels and buckets match trunk1's telemetry service so its dashboards carry over.

use proto::upload_metrics::trunk::{QuarantineQueryResult, UploadMetrics};
use serde_json::{Value, json};

pub const CLI_TELEMETRY_METRICS_PATH: &str = "/v2/telemetry/v1/metrics";
pub const SERVICE_NAME: &str = "trunk-analytics-cli";

pub const UPLOADS_METRIC: &str = "trunk.analytics_cli.uploads";
pub const UPLOAD_DURATION_METRIC: &str = "trunk.analytics_cli.upload.duration";
pub const RUN_DURATION_METRIC: &str = "trunk.analytics_cli.run.duration";
pub const UPLOADS_UNIT: &str = "{upload}";
pub const DURATION_UNIT: &str = "s";

pub const STATUS_ATTRIBUTE: &str = "trunk.analytics_cli.upload.status";
pub const FAILURE_REASON_ATTRIBUTE: &str = "trunk.analytics_cli.upload.failure_reason";
pub const QUARANTINE_QUERY_RESULT_ATTRIBUTE: &str = "trunk.analytics_cli.quarantine.query_result";
pub const REPOSITORY_OWNER_ATTRIBUTE: &str = "trunk.vcs.repository.owner";
pub const REPOSITORY_NAME_ATTRIBUTE: &str = "trunk.vcs.repository.name";

pub const STATUS_SUCCESS: &str = "success";
pub const STATUS_FAILED: &str = "failed";
pub const UNKNOWN_LABEL: &str = "unknown";
pub const INVALID_LABEL: &str = "invalid";

/// trunk1's `DURATION_BUCKETS_SECONDS`.
pub const DURATION_BOUNDS_SECONDS: [f64; 13] = [
    0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0,
];

/// OTLP's `AGGREGATION_TEMPORALITY_DELTA`.
const DELTA: i32 = 1;

const LABEL_MAX_CHARS: usize = 32;
/// GitHub's own limits.
const OWNER_MAX_CHARS: usize = 39;
const NAME_MAX_CHARS: usize = 100;

/// OTLP JSON writes 64-bit integers as decimal strings.
pub fn upload_metrics_request(
    metrics: &UploadMetrics,
    environment: &str,
    now_unix_nanos: i128,
) -> Value {
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

    let status = bounded_label(
        if metrics.failed {
            STATUS_FAILED
        } else {
            STATUS_SUCCESS
        },
        LABEL_MAX_CHARS,
    );
    let repo = metrics.repo.clone().unwrap_or_default();
    let quarantine_query_result =
        QuarantineQueryResult::try_from(metrics.quarantine_query_result).unwrap_or_default();
    let upload_attributes = [
        string_attribute(STATUS_ATTRIBUTE, &status),
        string_attribute(
            FAILURE_REASON_ATTRIBUTE,
            &bounded_label(&metrics.failure_reason, LABEL_MAX_CHARS),
        ),
        string_attribute(
            QUARANTINE_QUERY_RESULT_ATTRIBUTE,
            &bounded_label(
                quarantine_query_result_label(quarantine_query_result),
                LABEL_MAX_CHARS,
            ),
        ),
        string_attribute(
            REPOSITORY_OWNER_ATTRIBUTE,
            &bounded_label(&repo.owner, OWNER_MAX_CHARS),
        ),
        string_attribute(
            REPOSITORY_NAME_ATTRIBUTE,
            &bounded_label(&repo.name, NAME_MAX_CHARS),
        ),
    ];

    let [cli_started, upload_started, upload_finished] = [
        &metrics.cli_started_at,
        &metrics.upload_started_at,
        &metrics.upload_finished_at,
    ]
    .map(|time| {
        time.as_ref()
            .map(|time| unix_nanos(time.seconds, time.nanos))
    });
    let start_time = cli_started.unwrap_or(now_unix_nanos).to_string();
    let time = now_unix_nanos.to_string();

    let mut otlp_metrics = vec![json!({
        "name": UPLOADS_METRIC,
        "unit": UPLOADS_UNIT,
        "sum": {
            "aggregationTemporality": DELTA,
            "isMonotonic": true,
            "dataPoints": [{
                "attributes": upload_attributes,
                "startTimeUnixNano": start_time,
                "timeUnixNano": time,
                "asInt": "1",
            }],
        },
    })];
    for (name, started) in [
        (UPLOAD_DURATION_METRIC, upload_started),
        (RUN_DURATION_METRIC, cli_started),
    ] {
        let Some(seconds) = duration_seconds(started, upload_finished) else {
            continue;
        };
        let mut bucket_counts = vec!["0"; DURATION_BOUNDS_SECONDS.len() + 1];
        bucket_counts[bucket_index(seconds, &DURATION_BOUNDS_SECONDS)] = "1";
        otlp_metrics.push(json!({
            "name": name,
            "unit": DURATION_UNIT,
            "histogram": {
                "aggregationTemporality": DELTA,
                "dataPoints": [{
                    "attributes": [string_attribute(STATUS_ATTRIBUTE, &status)],
                    "startTimeUnixNano": start_time,
                    "timeUnixNano": time,
                    "count": "1",
                    "sum": seconds,
                    "min": seconds,
                    "max": seconds,
                    "explicitBounds": DURATION_BOUNDS_SECONDS,
                    "bucketCounts": bucket_counts,
                }],
            },
        }));
    }

    json!({
        "resourceMetrics": [{
            "resource": { "attributes": resource_attributes },
            "scopeMetrics": [{
                "scope": { "name": SERVICE_NAME, "version": version },
                "metrics": otlp_metrics,
            }],
        }],
    })
}

/// trunk1's `quarantineQueryResultToLabel`.
pub fn quarantine_query_result_label(result: QuarantineQueryResult) -> &'static str {
    match result {
        QuarantineQueryResult::Unspecified => "unspecified",
        QuarantineQueryResult::Success => "success",
        QuarantineQueryResult::Failure => "failure",
        QuarantineQueryResult::Disabled => "disabled",
        QuarantineQueryResult::Skipped => "skipped",
        QuarantineQueryResult::Cached => "cached",
    }
}

/// trunk1's `boundedLabel`: the first `max_chars` characters, or `invalid` when they leave the
/// label charset. An empty value is `unknown`.
pub fn bounded_label(value: &str, max_chars: usize) -> String {
    if value.is_empty() {
        return UNKNOWN_LABEL.into();
    }
    let trimmed: String = value.chars().take(max_chars).collect();
    if trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '/' | '-'))
    {
        trimmed
    } else {
        INVALID_LABEL.into()
    }
}

/// OTLP bucket `i` counts values in `(bounds[i - 1], bounds[i]]`; the last one counts values
/// above every bound.
pub fn bucket_index(value: f64, bounds: &[f64]) -> usize {
    bounds
        .iter()
        .position(|bound| value <= *bound)
        .unwrap_or(bounds.len())
}

fn duration_seconds(started: Option<i128>, finished: Option<i128>) -> Option<f64> {
    let (started, finished) = (started?, finished?);
    if started < 0 || finished < started {
        return None;
    }
    Some((finished - started) as f64 / 1e9)
}

pub fn unix_nanos(seconds: i64, nanos: i32) -> i128 {
    i128::from(seconds) * 1_000_000_000 + i128::from(nanos)
}

fn string_attribute(key: &str, value: &str) -> Value {
    json!({ "key": key, "value": { "stringValue": value } })
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
    use proto::upload_metrics::trunk::Repo;

    use super::*;

    const SECOND: i64 = 1_000_000_000;

    #[test]
    fn a_value_on_a_bound_lands_in_that_bound_s_bucket() {
        let bounds = DURATION_BOUNDS_SECONDS;
        assert_eq!(bucket_index(0.0, &bounds), 0);
        assert_eq!(bucket_index(0.05, &bounds), 0);
        assert_eq!(bucket_index(0.050_001, &bounds), 1);
        assert_eq!(bucket_index(1.0, &bounds), 4);
        assert_eq!(bucket_index(1.000_001, &bounds), 5);
        assert_eq!(bucket_index(600.0, &bounds), 12);
        assert_eq!(bucket_index(600.000_001, &bounds), 13);
    }

    #[test]
    fn labels_are_bounded_like_trunk1() {
        assert_eq!(bounded_label("", 32), "unknown");
        assert_eq!(bounded_label("400_bad_request", 32), "400_bad_request");
        assert_eq!(bounded_label("a.b_c+d/e-F9", 32), "a.b_c+d/e-F9");
        assert_eq!(bounded_label("has space", 32), "invalid");
        assert_eq!(bounded_label("émoji", 32), "invalid");
        assert_eq!(bounded_label(&"a".repeat(33), 32), "a".repeat(32));
        assert_eq!(
            bounded_label(&format!("{} x", "a".repeat(32)), 32),
            "a".repeat(32)
        );
        assert_eq!(bounded_label(&"o".repeat(40), 39), "o".repeat(39));
    }

    #[test]
    fn missing_or_backwards_durations_are_skipped() {
        assert_eq!(duration_seconds(None, Some(1)), None);
        assert_eq!(duration_seconds(Some(1), None), None);
        assert_eq!(duration_seconds(Some(-1), Some(1)), None);
        assert_eq!(duration_seconds(Some(5), Some(1)), None);
        assert_eq!(duration_seconds(Some(0), Some(2_500_000_000)), Some(2.5));
    }

    #[test]
    fn a_failed_upload_sends_a_count_and_both_durations() {
        let now: i64 = 1_700_000_100 * SECOND;
        let metrics = UploadMetrics {
            repo: Some(Repo {
                host: "github.com".into(),
                owner: "my-org".into(),
                name: "my repo".into(),
            }),
            cli_started_at: Some(chrono::DateTime::from_timestamp_nanos(now - 12 * SECOND).into()),
            upload_started_at: Some(chrono::DateTime::from_timestamp_nanos(now - SECOND).into()),
            upload_finished_at: Some(chrono::DateTime::from_timestamp_nanos(now).into()),
            failed: true,
            failure_reason: "connection".into(),
            quarantine_query_result: QuarantineQueryResult::Cached.into(),
            ..UploadMetrics::default()
        };
        let body = upload_metrics_request(&metrics, "staging", now.into());
        let metrics = body["resourceMetrics"][0]["scopeMetrics"][0]["metrics"]
            .as_array()
            .unwrap();
        assert_eq!(metrics.len(), 3);

        let uploads = &metrics[0];
        assert_eq!(uploads["name"], UPLOADS_METRIC);
        assert_eq!(uploads["unit"], UPLOADS_UNIT);
        assert_eq!(uploads["sum"]["aggregationTemporality"], 1);
        assert_eq!(uploads["sum"]["isMonotonic"], true);
        let point = &uploads["sum"]["dataPoints"][0];
        assert_eq!(point["asInt"], "1");
        assert_eq!(point["timeUnixNano"], now.to_string());
        assert_eq!(point["startTimeUnixNano"], (now - 12 * SECOND).to_string());
        let attributes: Vec<(&str, &str)> = point["attributes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                (
                    a["key"].as_str().unwrap(),
                    a["value"]["stringValue"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            attributes,
            [
                (STATUS_ATTRIBUTE, "failed"),
                (FAILURE_REASON_ATTRIBUTE, "connection"),
                (QUARANTINE_QUERY_RESULT_ATTRIBUTE, "cached"),
                (REPOSITORY_OWNER_ATTRIBUTE, "my-org"),
                (REPOSITORY_NAME_ATTRIBUTE, "invalid"),
            ]
        );

        for (metric, name, seconds, bucket) in [
            (&metrics[1], UPLOAD_DURATION_METRIC, 1.0, 4),
            (&metrics[2], RUN_DURATION_METRIC, 12.0, 8),
        ] {
            assert_eq!(metric["name"], name);
            assert_eq!(metric["unit"], DURATION_UNIT);
            assert_eq!(metric["histogram"]["aggregationTemporality"], 1);
            let point = &metric["histogram"]["dataPoints"][0];
            assert_eq!(point["count"], "1");
            assert_eq!(point["sum"], seconds);
            assert_eq!(point["min"], seconds);
            assert_eq!(point["max"], seconds);
            let counts = point["bucketCounts"].as_array().unwrap();
            assert_eq!(counts.len(), DURATION_BOUNDS_SECONDS.len() + 1);
            for (index, count) in counts.iter().enumerate() {
                assert_eq!(count, if index == bucket { "1" } else { "0" });
            }
        }
    }

    #[test]
    fn a_duration_without_its_start_is_not_sent() {
        let metrics = UploadMetrics {
            upload_finished_at: Some(chrono::DateTime::from_timestamp_nanos(5).into()),
            ..UploadMetrics::default()
        };
        let body = upload_metrics_request(&metrics, "production", 10);
        let metrics = body["resourceMetrics"][0]["scopeMetrics"][0]["metrics"]
            .as_array()
            .unwrap();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0]["name"], UPLOADS_METRIC);
        let values: Vec<&str> = metrics[0]["sum"]["dataPoints"][0]["attributes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["value"]["stringValue"].as_str().unwrap())
            .collect();
        assert_eq!(
            values,
            ["success", "unknown", "unspecified", "unknown", "unknown"]
        );
    }
}
