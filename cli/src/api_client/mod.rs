use std::path::Path;

use anyhow::Context;
use api;
use call_api::CallApi;
use http::{header::HeaderMap, HeaderValue};
use reqwest::{header, Client, Response, StatusCode};
use tokio::fs;

use crate::constants::{DEFAULT_ORIGIN, TRUNK_PUBLIC_API_ADDRESS_ENV};

mod call_api;

pub struct ApiClient {
    host: String,
    s3_client: Client,
    trunk_client: Client,
}

impl ApiClient {
    const TRUNK_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
    const TRUNK_API_TOKEN_HEADER: &str = "x-api-token";

    pub fn new(api_token: String) -> anyhow::Result<Self> {
        let trimmed_token = api_token.trim();
        if trimmed_token.is_empty() {
            return Err(anyhow::anyhow!("Trunk API token is required."));
        }
        let api_token_header_value = HeaderValue::from_str(&api_token)
            .map_err(|_| anyhow::Error::msg("Trunk API token is not ASCII"))?;

        let host = std::env::var(TRUNK_PUBLIC_API_ADDRESS_ENV)
            .ok()
            .and_then(|s| if s.is_empty() { None } else { Some(s) })
            .unwrap_or_else(|| DEFAULT_ORIGIN.to_string());

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

        Ok(Self {
            host,
            s3_client,
            trunk_client,
        })
    }

    pub async fn create_trunk_repo(&self, request: &api::CreateRepoRequest) -> anyhow::Result<()> {
        CallApi {
            action: || async {
                let response = self
                    .trunk_client
                    .post(format!("{}/v1/repo/create", self.host))
                    .json(&request)
                    .send()
                    .await?;

                status_code_help(
                    &response,
                    CheckUnauthorized::Check,
                    CheckNotFound::DoNotCheck,
                    |_| format!("Failed to create repo."),
                )
            },
            log_progress_message: |time_elapsed, _| {
                format!("Communicating with Trunk services is taking longer than expected. It has taken {} seconds so far.", time_elapsed.as_secs())
            },
            report_slow_progress_message: |time_elapsed| {
                format!("Creating a Trunk repo is taking longer than {} seconds", time_elapsed.as_secs())
            },
        }
        .call_api()
        .await
    }

    pub async fn create_bundle_upload_intent(
        &self,
        request: &api::CreateBundleUploadRequest,
    ) -> anyhow::Result<api::CreateBundleUploadResponse> {
        CallApi {
            action: || async {
                let response = self
                    .trunk_client
                    .post(format!("{}/v1/metrics/createBundleUpload", self.host))
                    .json(&request)
                    .send()
                    .await?;

                status_code_help(
                    &response,
                    CheckUnauthorized::Check,
                    CheckNotFound::Check,
                    |_| String::from("Failed to create bundle upload."),
                )?;

                response
                    .json::<api::CreateBundleUploadResponse>()
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
        request: &api::GetQuarantineBulkTestStatusRequest,
    ) -> anyhow::Result<api::QuarantineConfig> {
        CallApi {
            action: || async {
                let response = self
                    .trunk_client
                    .post(format!("{}/v1/metrics/getQuarantineConfig", self.host))
                    .json(&request)
                    .send()
                    .await?;

                status_code_help(
                    &response,
                    CheckUnauthorized::Check,
                    CheckNotFound::Check,
                    |_| String::from("Failed to get quarantine bulk test."),
                )?;

                response
                    .json::<api::QuarantineConfig>()
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
    ) -> anyhow::Result<()> {
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
                    &response,
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

    pub async fn update_bundle_upload_status(
        &self,
        request: &api::UpdateBundleUploadRequest,
    ) -> anyhow::Result<()> {
        CallApi {
            action: || async {
                let response = self
                    .trunk_client
                    .patch(format!("{}/v1/metrics/updateBundleUpload", self.host))
                    .json(request)
                    .send()
                    .await?;

                status_code_help(
                    &response,
                    CheckUnauthorized::Check,
                    CheckNotFound::Check,
                    |_| {
                        format!(
                            "Failed to update bundle upload status to {:#?}",
                            request.upload_status
                        )
                    },
                )
            },
            log_progress_message: |time_elapsed, _| {
                format!("Communicating with Trunk services is taking longer than expected. It has taken {} seconds so far.", time_elapsed.as_secs())
            },
            report_slow_progress_message: |time_elapsed| {
                format!("Updating a bundle upload status is taking longer than {} seconds", time_elapsed.as_secs())
            },
        }
        .call_api()
        .await
    }
}

#[derive(Debug, Clone, Copy)]
enum CheckUnauthorized {
    Check,
    DoNotCheck,
}

#[derive(Debug, Clone, Copy)]
enum CheckNotFound {
    Check,
    DoNotCheck,
}

fn status_code_help<T: FnMut(&Response) -> String>(
    response: &Response,
    check_unauthorized: CheckUnauthorized,
    check_not_found: CheckNotFound,
    mut create_error_message: T,
) -> anyhow::Result<()> {
    if !response.status().is_client_error() {
        return Ok(());
    }

    let error_message = match (response.status(), check_unauthorized, check_not_found) {
        (StatusCode::UNAUTHORIZED, CheckUnauthorized::Check, _) => concat!(
            "Your Trunk token may be incorrect - find it on the Trunk app ",
            "(Settings -> Manage Organization -> Organization API Token -> View).",
        ),
        (StatusCode::NOT_FOUND, _, CheckNotFound::Check) => concat!(
            "Your Trunk organization URL slug may be incorrect - find it on the Trunk app ",
            "(Settings -> Manage Organization -> Organization Slug).",
        ),
        _ => &create_error_message(response),
    };

    let error_message_with_help =
        format!("{error_message}\n\nFor more help, contact us at https://slack.trunk.io/");

    Err(anyhow::Error::msg(error_message_with_help))
}
