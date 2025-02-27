use std::path::Path;

use anyhow::Context;
use constants::{DEFAULT_ORIGIN, TRUNK_PUBLIC_API_ADDRESS_ENV};
use http::{header::HeaderMap, HeaderValue};
use reqwest::{header, Client, Response, StatusCode};
use tokio::fs;

use crate::call_api::CallApi;
use crate::message;

pub struct ApiClient {
    pub host: String,
    s3_client: Client,
    trunk_client: Client,
    version_path_prefix: String,
}

impl ApiClient {
    const TRUNK_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
    const TRUNK_API_TOKEN_HEADER: &'static str = "x-api-token";

    pub fn new<T: AsRef<str>>(api_token: T) -> anyhow::Result<Self> {
        let api_token = api_token.as_ref();
        if api_token.trim().is_empty() {
            return Err(anyhow::anyhow!("Trunk API token is required."));
        }
        let api_token_header_value = HeaderValue::from_str(api_token)
            .map_err(|_| anyhow::Error::msg("Trunk API token is not ASCII"))?;

        let host = std::env::var(TRUNK_PUBLIC_API_ADDRESS_ENV)
            .ok()
            .and_then(|s| if s.is_empty() { None } else { Some(s) })
            .unwrap_or_else(|| DEFAULT_ORIGIN.to_string());
        tracing::info!("Using public api address {}", host);

        let mut trunk_client_default_headers = HeaderMap::new();
        trunk_client_default_headers.append(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        trunk_client_default_headers.append(Self::TRUNK_API_TOKEN_HEADER, api_token_header_value);

        let trunk_client = Client::builder()
            .timeout(Self::TRUNK_API_TIMEOUT)
            .default_headers(trunk_client_default_headers)
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

        Ok(Self {
            host,
            s3_client,
            trunk_client,
            version_path_prefix,
        })
    }

    pub async fn create_bundle_upload(
        &self,
        request: &message::CreateBundleUploadRequest,
    ) -> anyhow::Result<message::CreateBundleUploadResponse> {
        CallApi {
            action: || async {
                let response = self
                    .trunk_client
                    .post(format!("{}{}/metrics/createBundleUpload", self.host, self.version_path_prefix))
                    .json(&request)
                    .send()
                    .await?;

                let response = status_code_help(
                    response,
                    CheckUnauthorized::Check,
                    CheckNotFound::Check,
                    |_| String::from("Failed to create bundle upload."),
                )?;

                response
                    .json::<message::CreateBundleUploadResponse>()
                    .await
                    .context("Failed to get response body as json.")
            },
            log_progress_message: |time_elapsed, _| {
                format!("Reporting bundle upload initiation to Trunk services is taking longer than expected. It has taken {} seconds so far.", time_elapsed.as_secs())
            },
            report_slow_progress_message: |time_elapsed| {
                format!("Creating a Trunk upload intent is taking longer than {} seconds", time_elapsed.as_secs())
            },
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
                    .trunk_client
                    .post(format!("{}{}/metrics/getQuarantineConfig", self.host, self.version_path_prefix))
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
                )?;

                response
                    .json::<message::GetQuarantineConfigResponse>()
                    .await
                    .context("Failed to get response body as json.")
            },
            log_progress_message: |time_elapsed, _| {
                format!("Getting quarantine configuration from Trunk services is taking longer than expected. It has taken {} seconds so far.", time_elapsed.as_secs())
            },
            report_slow_progress_message: |time_elapsed| {
                format!("Getting a Trunk quarantine configuration is taking longer than {} seconds", time_elapsed.as_secs())
            },
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
                )
            },
            log_progress_message: |time_elapsed, _| {
                format!("Uploading bundle to S3 is taking longer than expected. It has taken {} seconds so far.", time_elapsed.as_secs())
            },
            report_slow_progress_message: |time_elapsed| {
                format!("Uploading bundle to S3 is taking longer than {} seconds", time_elapsed.as_secs())
            },
        }
        .call_api()
        .await
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

const HELP_TEXT: &str = "\n\nFor more help, contact us at https://slack.trunk.io/";

pub(crate) fn status_code_help<T: FnMut(&Response) -> String>(
    response: Response,
    check_unauthorized: CheckUnauthorized,
    check_not_found: CheckNotFound,
    mut create_error_message: T,
) -> anyhow::Result<Response> {
    let base_error_message = &create_error_message(&response);

    if !response.status().is_client_error() {
        response.error_for_status().map_err(|reqwest_error| {
            let error_message = format!("{base_error_message}{HELP_TEXT}");
            anyhow::Error::from(reqwest_error).context(error_message)
        })
    } else {
        let error_message = match (response.status(), check_unauthorized, check_not_found) {
            (StatusCode::UNAUTHORIZED, CheckUnauthorized::Check, _) => UNAUTHORIZED_CONTEXT,
            (StatusCode::NOT_FOUND, _, CheckNotFound::Check) => NOT_FOUND_CONTEXT,
            _ => base_error_message,
        };

        let error_message_with_help = format!("{error_message}{HELP_TEXT}");

        match response.error_for_status() {
            Ok(..) => Err(anyhow::Error::msg(error_message_with_help)),
            Err(error) => Err(anyhow::Error::from(error).context(error_message_with_help)),
        }
    }
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

        let mut api_client = ApiClient::new(String::from("mock-token")).unwrap();
        api_client.host.clone_from(&state.host);

        assert!(api_client
            .get_quarantining_config(&message::GetQuarantineConfigRequest {
                repo: context::repo::RepoUrlParts {
                    host: String::from("host"),
                    owner: String::from("owner"),
                    name: String::from("name"),
                },
                org_url_slug: String::from("org_url_slug"),
                test_identifiers: vec![],
            })
            .await
            .unwrap_err()
            .to_string()
            .contains("Failed to get quarantine bulk test."));
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

        let mut api_client = ApiClient::new(String::from("mock-token")).unwrap();
        api_client.host.clone_from(&state.host);

        assert!(api_client
            .get_quarantining_config(&message::GetQuarantineConfigRequest {
                repo: context::repo::RepoUrlParts {
                    host: String::from("host"),
                    owner: String::from("owner"),
                    name: String::from("name"),
                },
                org_url_slug: String::from("org_url_slug"),
                test_identifiers: vec![],
            })
            .await
            .unwrap_err()
            .to_string()
            .contains("Failed to get quarantine bulk test."));
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

        let mut api_client = ApiClient::new(String::from("mock-token")).unwrap();
        api_client.host.clone_from(&state.host);

        assert!(api_client
            .get_quarantining_config(&message::GetQuarantineConfigRequest {
                repo: context::repo::RepoUrlParts {
                    host: String::from("host"),
                    owner: String::from("owner"),
                    name: String::from("name"),
                },
                org_url_slug: String::from("org_url_slug"),
                test_identifiers: vec![],
            })
            .await
            .unwrap_err()
            .to_string()
            .contains("Quarantining config not found"));
    }
}
