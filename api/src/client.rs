use std::path::Path;
use std::sync::mpsc::Sender;

use constants::{DEFAULT_ORIGIN, TRUNK_PUBLIC_API_ADDRESS_ENV};
use display::message::DisplayMessage;
use http::{header::HeaderMap, HeaderValue};
use reqwest::{header, Client, Response, StatusCode};
use serde::de::DeserializeOwned;
use tokio::fs;

use crate::call_api::CallApi;
use crate::message;

pub struct ApiClient {
    pub api_host: String,
    pub telemetry_host: String,
    s3_client: Client,
    trunk_api_client: Client,
    telemetry_client: Client,
    version_path_prefix: String,
    org_url_slug: String,
    render_sender: Option<Sender<DisplayMessage>>,
}

pub fn get_api_host() -> String {
    std::env::var(TRUNK_PUBLIC_API_ADDRESS_ENV)
        .ok()
        .and_then(|s| if s.is_empty() { None } else { Some(s) })
        .unwrap_or_else(|| DEFAULT_ORIGIN.to_string())
}

impl ApiClient {
    const TRUNK_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
    // This should always be fast
    const TRUNK_TELEMETRY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);
    const TRUNK_API_TOKEN_HEADER: &'static str = "x-api-token";

    pub fn new<T: AsRef<str>>(
        api_token: T,
        org_url_slug: T,
        render_sender: Option<Sender<DisplayMessage>>,
    ) -> anyhow::Result<ApiClient> {
        let org_url_slug = String::from(org_url_slug.as_ref());
        let api_token = api_token.as_ref();
        if api_token.trim().is_empty() {
            return Err(anyhow::anyhow!("Trunk API token is required."));
        }
        let api_token_header_value = HeaderValue::from_str(api_token)
            .map_err(|_| anyhow::Error::msg("Trunk API token is not ASCII"))?;

        let api_host = get_api_host();
        tracing::debug!("Using public api address {}", api_host);

        let telemetry_host = if api_host.contains("https://") {
            format!(
                "https://telemetry.{}",
                api_host.split("https://").nth(1).unwrap_or_default()
            )
        } else {
            // If the api_host is not https, we default to the api host
            // this happens when the api_host is localhost and we are running
            // inside of test environments
            api_host.clone()
        };

        let mut trunk_api_client_default_headers = HeaderMap::new();
        trunk_api_client_default_headers.append(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        trunk_api_client_default_headers
            .append(Self::TRUNK_API_TOKEN_HEADER, api_token_header_value.clone());

        let trunk_api_client = Client::builder()
            .timeout(Self::TRUNK_API_TIMEOUT)
            .default_headers(trunk_api_client_default_headers)
            .build()?;

        let mut telemetry_client_default_headers = HeaderMap::new();
        telemetry_client_default_headers.append(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-protobuf"),
        );
        telemetry_client_default_headers
            .append(Self::TRUNK_API_TOKEN_HEADER, api_token_header_value);

        let telemetry_client = Client::builder()
            .timeout(Self::TRUNK_TELEMETRY_TIMEOUT)
            .default_headers(telemetry_client_default_headers)
            .build()?;

        let mut s3_client_default_headers = HeaderMap::new();
        s3_client_default_headers.append(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        let s3_client = Client::builder()
            .default_headers(s3_client_default_headers)
            .build()?;

        let version_path_prefix = if std::env::var("DEBUG_STRIP_VERSION_PREFIX").is_ok() {
            String::from("")
        } else {
            String::from("/v1")
        };

        Ok(ApiClient {
            telemetry_host,
            api_host,
            s3_client,
            trunk_api_client,
            telemetry_client,
            version_path_prefix,
            org_url_slug,
            render_sender,
        })
    }

    pub async fn create_bundle_upload(
        &self,
        request: &message::CreateBundleUploadRequest,
    ) -> anyhow::Result<message::CreateBundleUploadResponse> {
        CallApi {
            action: || async {
                let response = self
                    .trunk_api_client
                    .post(format!("{}{}/metrics/createBundleUpload", self.api_host, self.version_path_prefix))
                    .json(&request)
                    .send()
                    .await?;

                let response = status_code_help(
                    response,
                    CheckUnauthorized::Check,
                    CheckNotFound::Check,
                    |_| String::from("Failed to create bundle upload."),
                    &self.api_host,
                    &self.org_url_slug,
                )?;

                self.deserialize_response::<message::CreateBundleUploadResponse>(response).await
            },
            log_progress_message: |time_elapsed, _| {
                format!("Reporting bundle upload initiation to Trunk services is taking longer than expected. It has taken {} seconds so far.", time_elapsed.as_secs())
            },
            report_slow_progress_message: |time_elapsed| {
                format!("Creating a Trunk upload intent is taking longer than {} seconds", time_elapsed.as_secs())
            },
            render_sender: self.render_sender.clone(),
        }
        .call_api()
        .await
    }

    pub async fn get_quarantining_config(
        &self,
        request: &message::GetQuarantineConfigRequest,
    ) -> anyhow::Result<message::GetQuarantineConfigResponse> {
        CallApi {
            action: || async {
                let response = self
                    .trunk_api_client
                    .post(format!("{}{}/metrics/getQuarantineConfig", self.api_host, self.version_path_prefix))
                    .json(&request)
                    .send()
                    .await?;

                let response = status_code_help(
                    response,
                    CheckUnauthorized::Check,
                    CheckNotFound::DoNotCheck,
                    |response| -> String {
                        if response.status() == StatusCode::NOT_FOUND {
                            String::from("Quarantining config not found.")
                        } else  {
                            String::from("Failed to get quarantine bulk test.")
                        }
                    },
                    &self.api_host,
                    &self.org_url_slug,
                )?;

                self.deserialize_response::<message::GetQuarantineConfigResponse>(response).await
            },
            log_progress_message: |time_elapsed, _| {
                format!("Getting quarantine configuration from Trunk services is taking longer than expected. It has taken {} seconds so far.", time_elapsed.as_secs())
            },
            report_slow_progress_message: |time_elapsed| {
                format!("Getting a Trunk quarantine configuration is taking longer than {} seconds", time_elapsed.as_secs())
            },
            render_sender: self.render_sender.clone(),
        }
        .call_api()
        .await
    }

    pub async fn list_quarantined_tests(
        &self,
        request: &message::ListQuarantinedTestsRequest,
    ) -> anyhow::Result<message::ListQuarantinedTestsResponse> {
        CallApi {
            action: || async {
                let response = self
                    .trunk_api_client
                    .post(format!("{}{}/flaky-tests/list-quarantined-tests", self.api_host, self.version_path_prefix))
                    .json(&request)
                    .send()
                    .await?;

                let response = status_code_help(
                    response,
                    CheckUnauthorized::Check,
                    CheckNotFound::DoNotCheck,
                    |_| String::from("Failed to list quarantined tests."),
                    &self.api_host,
                    &self.org_url_slug,
                )?;

                self.deserialize_response::<message::ListQuarantinedTestsResponse>(response).await
            },
            log_progress_message: |time_elapsed, _| {
                format!("Listing quarantined tests from Trunk services is taking longer than expected. It has taken {} seconds so far.", time_elapsed.as_secs())
            },
            report_slow_progress_message: |time_elapsed| {
                format!("Listing quarantined tests from Trunk services is taking longer than {} seconds", time_elapsed.as_secs())
            },
            render_sender: self.render_sender.clone(),
        }
        .call_api()
        .await
    }

    pub async fn put_bundle_to_s3<U: AsRef<str>, B: AsRef<Path>>(
        &self,
        url: U,
        bundle_path: B,
    ) -> anyhow::Result<Response> {
        CallApi {
            action: || async {
                let file = fs::File::open(bundle_path.as_ref()).await?;
                let file_size = file.metadata().await?.len();

                let response = self
                    .s3_client
                    .put(url.as_ref())
                    .header(header::CONTENT_LENGTH, file_size)
                    .body(file)
                    .send()
                    .await?;

                status_code_help(
                    response,
                    CheckUnauthorized::DoNotCheck,
                    CheckNotFound::DoNotCheck,
                    |_| String::from("Failed to upload bundle to S3."),
                    &self.api_host,
                    &self.org_url_slug
                )
            },
            log_progress_message: |time_elapsed, _| {
                format!("Uploading bundle to S3 is taking longer than expected. It has taken {} seconds so far.", time_elapsed.as_secs())
            },
            report_slow_progress_message: |time_elapsed| {
                format!("Uploading bundle to S3 is taking longer than {} seconds", time_elapsed.as_secs())
            },
            render_sender: self.render_sender.clone(),
        }
        .call_api()
        .await
    }

    pub async fn telemetry_upload_metrics(
        &self,
        request: &message::TelemetryUploadMetricsRequest,
    ) -> anyhow::Result<()> {
        CallApi {
            action: || async {
                if std::env::var("DISABLE_TELEMETRY").is_ok() {
                    return Ok(());
                }
                let response = self
                    .telemetry_client
                    .post(format!(
                        "{}{}/flakytests-cli/upload-metrics",
                        self.telemetry_host, self.version_path_prefix
                    ))
                    .body(prost::Message::encode_to_vec(&request.upload_metrics))
                    .send()
                    .await?;

                let error_message = "Failed to send telemetry metrics";
                if !response.status().is_client_error() {
                    response.error_for_status().map_err(|reqwest_error| {
                        tracing::warn!(hidden_in_console=true, "{} - {}", error_message, reqwest_error);
                        anyhow::Error::from(reqwest_error)
                    }).map(|_| ())
                } else {
                    match response.error_for_status() {
                        Ok(response) => {
                            tracing::debug!("{} - {}", error_message, response.status());
                            Err(anyhow::Error::msg(error_message))
                        },
                        Err(error) => {
                            tracing::debug!("{} - {}", error_message, error);
                            Err(anyhow::Error::from(error))
                        },
                    }
                }
            },
            log_progress_message: {
                |time_elapsed, _| {
                    format!("Reporting telemetry metrics to Trunk services is taking longer than expected. It has taken {} seconds so far.", time_elapsed.as_secs())
                }
            },
            report_slow_progress_message: {
                |time_elapsed| {
                    format!("Reporting telemetry metrics to Trunk services is taking longer than {} seconds", time_elapsed.as_secs())
                }
            },
            render_sender: self.render_sender.clone(),
        }
        .call_api()
        .await
    }

    async fn deserialize_response<MessageType: DeserializeOwned>(
        &self,
        response: Response,
    ) -> Result<MessageType, anyhow::Error> {
        let deserialized: reqwest::Result<MessageType> = response.json::<MessageType>().await;
        if deserialized.is_err() {
            tracing::warn!("Failed to get response body as json.");
        }
        deserialized.map_err(anyhow::Error::from)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CheckUnauthorized {
    Check,
    DoNotCheck,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CheckNotFound {
    Check,
    DoNotCheck,
}

pub(crate) const UNAUTHORIZED_CONTEXT: &str = concat!(
    "Your Trunk token may be incorrect - find it on the Trunk app ",
    "(Settings -> Manage Organization -> Organization API Token -> View).",
);

pub(crate) const NOT_FOUND_CONTEXT: &str = concat!(
    "Your Trunk organization URL slug may be incorrect - find it on the Trunk app ",
    "(Settings -> Manage Organization -> Organization Slug).",
);

pub(crate) fn status_code_help<T: FnMut(&Response) -> String>(
    response: Response,
    check_unauthorized: CheckUnauthorized,
    check_not_found: CheckNotFound,
    mut create_error_message: T,
    api_host: &str,
    org_url_slug: &str,
) -> anyhow::Result<Response> {
    let base_error_message = create_error_message(&response);

    if !response.status().is_client_error() {
        response.error_for_status().map_err(anyhow::Error::from)
    } else {
        let domain: Option<String> = url::Url::parse(api_host)
            .ok()
            .into_iter()
            .flat_map(|url| {
                url.domain()
                    .into_iter()
                    .map(String::from)
                    .collect::<Vec<String>>()
            })
            .next();
        let error_message = match (response.status(), check_unauthorized, check_not_found) {
            (StatusCode::UNAUTHORIZED, CheckUnauthorized::Check, _) => add_settings_url_to_context(
                UNAUTHORIZED_CONTEXT,
                domain,
                &String::from(org_url_slug),
            ),
            (StatusCode::NOT_FOUND, _, CheckNotFound::Check) => {
                add_settings_url_to_context(NOT_FOUND_CONTEXT, domain, &String::from(org_url_slug))
            }
            _ => base_error_message,
        };

        match response.error_for_status() {
            Ok(..) => Err(anyhow::Error::msg(error_message)),
            Err(error) => Err(anyhow::Error::from(error)),
        }
    }
}

fn add_settings_url_to_context(
    context: &str,
    domain: Option<String>,
    org_url_slug: &String,
) -> String {
    match domain {
        Some(present_domain) => {
            let settings_url = format!(
                "https://{}/{}/settings",
                present_domain.replace("api", "app"),
                org_url_slug
            );
            format!(
                "{}\nHint - Your settings page can be found at: {}",
                context, settings_url
            )
        }
        None => String::from(context),
    }
}

#[test]
fn adds_settings_if_domain_present() {
    let domain = url::Url::parse("https://api.fake-trunk.io/")
        .ok()
        .into_iter()
        .flat_map(|url| {
            url.domain()
                .into_iter()
                .map(String::from)
                .collect::<Vec<String>>()
        })
        .next();
    let final_context =
        add_settings_url_to_context("base_context", domain, &String::from("fake-org-slug"));
    assert_eq!(
        final_context,
        "base_context\nHint - Your settings page can be found at: https://app.fake-trunk.io/fake-org-slug/settings",
    )
}

#[test]
fn does_not_add_settings_if_domain_absent() {
    let final_context =
        add_settings_url_to_context("base_context", None, &String::from("fake-org-slug"));
    assert_eq!(final_context, "base_context",)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use axum::{http::StatusCode, response::Response};
    use context;
    use lazy_static::lazy_static;
    use test_utils::mock_server::MockServerBuilder;

    use super::ApiClient;
    use crate::message;

    #[tokio::test(start_paused = true)]
    async fn does_not_retry_on_ok_501() {
        let mut mock_server_builder = MockServerBuilder::new();

        lazy_static! {
            static ref CALL_COUNT: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        }

        let quarantining_config_handler = move || async {
            CALL_COUNT.fetch_add(1, Ordering::Relaxed);
            Response::builder()
                .status(StatusCode::NOT_IMPLEMENTED)
                .body(String::from(
                    r#"{ "status_code": 501, "error": "we broke" }"#,
                ))
                .unwrap()
        };

        mock_server_builder.set_get_quarantining_config_handler(quarantining_config_handler);

        let state = mock_server_builder.spawn_mock_server().await;

        let mut api_client =
            ApiClient::new(String::from("mock-token"), String::from("mock-org"), None).unwrap();
        api_client.api_host.clone_from(&state.host);

        assert!(api_client
            .get_quarantining_config(&message::GetQuarantineConfigRequest {
                repo: context::repo::RepoUrlParts {
                    host: String::from("host"),
                    owner: String::from("owner"),
                    name: String::from("name"),
                },
                org_url_slug: String::from("org_url_slug"),
                test_identifiers: vec![],
                remote_urls: vec![],
            })
            .await
            .unwrap_err()
            .to_string()
            .contains("501 Not Implemented"));
        assert_eq!(CALL_COUNT.load(Ordering::Relaxed), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn retries_on_ok_500() {
        let mut mock_server_builder = MockServerBuilder::new();

        lazy_static! {
            static ref CALL_COUNT: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        }

        let quarantining_config_handler = move || async {
            CALL_COUNT.fetch_add(1, Ordering::Relaxed);
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(String::from(
                    r#"{ "status_code": 500, "error": "we broke" }"#,
                ))
                .unwrap()
        };

        mock_server_builder.set_get_quarantining_config_handler(quarantining_config_handler);

        let state = mock_server_builder.spawn_mock_server().await;

        let mut api_client =
            ApiClient::new(String::from("mock-token"), String::from("mock-org"), None).unwrap();
        api_client.api_host.clone_from(&state.host);

        assert!(api_client
            .get_quarantining_config(&message::GetQuarantineConfigRequest {
                repo: context::repo::RepoUrlParts {
                    host: String::from("api_host"),
                    owner: String::from("owner"),
                    name: String::from("name"),
                },
                remote_urls: vec![],
                org_url_slug: String::from("org_url_slug"),
                test_identifiers: vec![],
            })
            .await
            .unwrap_err()
            .to_string()
            .contains("500 Internal Server Error"));
        assert_eq!(CALL_COUNT.load(Ordering::Relaxed), 6);
    }

    #[tokio::test(start_paused = true)]
    async fn get_quarantining_config_not_found() {
        let mut mock_server_builder = MockServerBuilder::new();

        async fn quarantining_config_not_found_handler() -> Response<String> {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(String::from(
                    r#"{ "status_code": 404, "error": "not found" }"#,
                ))
                .unwrap()
        }
        mock_server_builder
            .set_get_quarantining_config_handler(quarantining_config_not_found_handler);

        let state = mock_server_builder.spawn_mock_server().await;

        let mut api_client =
            ApiClient::new(String::from("mock-token"), String::from("mock-org"), None).unwrap();
        api_client.api_host.clone_from(&state.host);

        assert!(api_client
            .get_quarantining_config(&message::GetQuarantineConfigRequest {
                repo: context::repo::RepoUrlParts {
                    host: String::from("api_host"),
                    owner: String::from("owner"),
                    name: String::from("name"),
                },
                remote_urls: vec![],
                org_url_slug: String::from("org_url_slug"),
                test_identifiers: vec![],
            })
            .await
            .unwrap_err()
            .to_string()
            .contains("404 Not Found"));
    }
}
