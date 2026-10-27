use std::collections::HashSet;
use std::fmt;
use std::path::Path;
use std::sync::mpsc::Sender;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use constants::{DEFAULT_ORIGIN, TRUNK_PUBLIC_API_ADDRESS_ENV};
use display::message::DisplayMessage;
use http::{HeaderValue, header::HeaderMap};
use reqwest::{Client, Response, StatusCode, header};
use serde::de::DeserializeOwned;
use tokio::fs;

use crate::call_api::CallApi;
use crate::message::{self, UploadApiVersion};
use crate::quarantine::{QuarantineLookup, QuarantinedTestSet, QuarantinedTests};
use crate::urls::TestCaseGuidScope;

#[derive(Debug)]
pub enum ApiErrorEndpoint {
    CreateBundleUpload,
    GetQuarantiningConfig,
    PutBundleToS3,
    TelemetryUploadMetrics,
}

impl ApiErrorEndpoint {
    pub fn unknown_error_reason(&self) -> String {
        match self {
            Self::CreateBundleUpload => "unknown_create_bundle_upload".into(),
            Self::GetQuarantiningConfig => "unknown_get_quarantining_config".into(),
            Self::PutBundleToS3 => "unknown_put_bundle_to_s3".into(),
            Self::TelemetryUploadMetrics => "unknown_telemetry_upload_metrics".into(),
        }
    }
}

impl fmt::Display for ApiErrorEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateBundleUpload => write!(f, "Error in create bundle upload endpoint"),
            Self::GetQuarantiningConfig => write!(f, "Error in get quarantine config endpoint"),
            Self::PutBundleToS3 => write!(f, "Error in put bundle to s3 endpoint"),
            Self::TelemetryUploadMetrics => write!(f, "Error in telemetry upload metrics endpoint"),
        }
    }
}

impl std::error::Error for ApiErrorEndpoint {}

pub struct ApiClient {
    pub api_host: String,
    pub telemetry_host: String,
    s3_client: Client,
    trunk_api_client: Client,
    /// Only a client holding an API token can call v2, which takes it as a bearer credential.
    trunk_api_v2_client: Option<Client>,
    telemetry_client: Client,
    version_path_prefix: String,
    org_url_slug: String,
    render_sender: Option<Sender<DisplayMessage>>,
    upload_api_version: UploadApiVersion,
}

pub fn get_api_host() -> String {
    std::env::var(TRUNK_PUBLIC_API_ADDRESS_ENV)
        .ok()
        .and_then(|s| if s.is_empty() { None } else { Some(s) })
        .unwrap_or_else(|| DEFAULT_ORIGIN.to_string())
}

impl ApiClient {
    const TRUNK_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
    // Two attempts at this, so a slow answer holds up a customer's CI run for at most 4s.
    const UPLOAD_API_VERSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
    const UPLOAD_API_VERSION_ATTEMPTS: usize = 2;
    const QUARANTINE_STATUS_MAX_IDENTIFIERS: usize = 2_000;
    const ALL_QUARANTINED_TESTS_PAGE_SIZE: usize = 25_000;
    // This should always be fast
    const TRUNK_TELEMETRY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);
    const TRUNK_API_TOKEN_HEADER: &'static str = "x-api-token";
    const TRUNK_PUBLIC_REPO_ID_HEADER: &'static str = "x-trunk-public-repo-id";
    const TRUNK_ALLOW_FORKED_PR_UPLOADS_HEADER: &'static str = "x-trunk-allow-forked-pr-uploads";

    pub fn new(
        api_token: Option<String>,
        public_repo_id: Option<String>,
        allow_forked_pr_uploads: bool,
        org_url_slug: impl AsRef<str>,
        render_sender: Option<Sender<DisplayMessage>>,
    ) -> anyhow::Result<ApiClient> {
        let org_url_slug = String::from(org_url_slug.as_ref());

        // Either a secret API token or a non-secret public repo id authenticates the
        // request. Fork PRs can't read repo secrets, so they rely on the public repo id.
        let api_token = api_token.filter(|token| !token.trim().is_empty());
        let public_repo_id = public_repo_id.filter(|id| !id.trim().is_empty());
        // The forked-PR lane presents no credential at all: the collection id already in the
        // request authorizes it, gated on that collection's opt-in server-side.
        if api_token.is_none() && public_repo_id.is_none() && !allow_forked_pr_uploads {
            return Err(anyhow::anyhow!(
                "Either a Trunk API token, a public repo id, or --allow-forked-pr-uploads is required."
            ));
        }

        let api_token_header_value = api_token
            .as_deref()
            .map(|token| {
                HeaderValue::from_str(token)
                    .map_err(|_| anyhow::Error::msg("Trunk API token is not ASCII"))
            })
            .transpose()?;
        let public_repo_id_header_value = public_repo_id
            .as_deref()
            .map(|id| {
                HeaderValue::from_str(id)
                    .map_err(|_| anyhow::Error::msg("Trunk public repo id is not ASCII"))
            })
            .transpose()?;

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
        if let Some(api_token_header_value) = api_token_header_value.clone() {
            trunk_api_client_default_headers
                .append(Self::TRUNK_API_TOKEN_HEADER, api_token_header_value);
        }
        if let Some(public_repo_id_header_value) = public_repo_id_header_value {
            trunk_api_client_default_headers.append(
                Self::TRUNK_PUBLIC_REPO_ID_HEADER,
                public_repo_id_header_value,
            );
        }
        if allow_forked_pr_uploads {
            trunk_api_client_default_headers.append(
                Self::TRUNK_ALLOW_FORKED_PR_UPLOADS_HEADER,
                HeaderValue::from_static("true"),
            );
        }

        let trunk_api_client = Client::builder()
            .timeout(Self::TRUNK_API_TIMEOUT)
            .default_headers(trunk_api_client_default_headers)
            .build()?;

        let trunk_api_v2_client = api_token
            .as_deref()
            .map(|token| {
                let mut headers = HeaderMap::new();
                headers.append(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                );
                headers.append(
                    header::AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {token}"))
                        .map_err(|_| anyhow::Error::msg("Trunk API token is not ASCII"))?,
                );
                Client::builder()
                    .timeout(Self::TRUNK_API_TIMEOUT)
                    .default_headers(headers)
                    .build()
                    .map_err(anyhow::Error::from)
            })
            .transpose()?;

        let mut telemetry_client_default_headers = HeaderMap::new();
        telemetry_client_default_headers.append(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-protobuf"),
        );

        if let Some(api_token_header_value) = api_token_header_value {
            telemetry_client_default_headers
                .append(Self::TRUNK_API_TOKEN_HEADER, api_token_header_value);
        }

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
            trunk_api_v2_client,
            telemetry_client,
            version_path_prefix,
            org_url_slug,
            render_sender,
            upload_api_version: UploadApiVersion::V1,
        })
    }

    pub fn upload_api_version(&self) -> UploadApiVersion {
        self.upload_api_version
    }

    /// Which API the run's quarantine lookup and upload use. Every later call follows it.
    pub fn set_upload_api_version(&mut self, upload_api_version: UploadApiVersion) {
        self.upload_api_version = upload_api_version;
    }

    /// Asks Trunk whether this run uses v2. Anything but an explicit `v2`, a failed call
    /// included, is v1, so this never fails a run. A timeout, a connection error or a 5xx is
    /// retried once; any other answer is final.
    pub async fn get_upload_api_version(
        &self,
        request: &message::GetUploadApiVersionRequest,
    ) -> UploadApiVersion {
        let upload_api_version = if self.trunk_api_v2_client.is_none()
            || request
                .test_collection_short_id
                .as_deref()
                .is_none_or(str::is_empty)
        {
            UploadApiVersion::V1
        } else {
            match self.request_upload_api_version(request).await {
                Ok(response) if response.api_version == "v2" => UploadApiVersion::V2,
                Ok(_) => UploadApiVersion::V1,
                Err(error) => {
                    tracing::debug!("Could not choose the Trunk API version, using v1: {error:?}");
                    UploadApiVersion::V1
                }
            }
        };
        tracing::info!("Using Trunk API {}", upload_api_version.as_str());
        upload_api_version
    }

    async fn request_upload_api_version(
        &self,
        request: &message::GetUploadApiVersionRequest,
    ) -> anyhow::Result<message::GetUploadApiVersionResponse> {
        let mut attempt = 1;
        loop {
            match self.request_upload_api_version_once(request).await {
                Err(error)
                    if attempt < Self::UPLOAD_API_VERSION_ATTEMPTS && is_retryable(&error) =>
                {
                    tracing::debug!("Retrying the Trunk API version check: {error:?}");
                    attempt += 1;
                }
                result => return result,
            }
        }
    }

    async fn request_upload_api_version_once(
        &self,
        request: &message::GetUploadApiVersionRequest,
    ) -> anyhow::Result<message::GetUploadApiVersionResponse> {
        Ok(self
            .trunk_api_client
            .post(format!(
                "{}{}/metrics/getUploadApiVersion",
                self.api_host, self.version_path_prefix
            ))
            .timeout(Self::UPLOAD_API_VERSION_TIMEOUT)
            .json(request)
            .send()
            .await?
            .error_for_status()?
            .json::<message::GetUploadApiVersionResponse>()
            .await?)
    }

    fn v2_client(&self) -> anyhow::Result<&Client> {
        self.trunk_api_v2_client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("The Trunk API v2 needs a Trunk API token."))
    }

    fn v2_test_collection_url(
        &self,
        test_collection_short_id: Option<&str>,
        route: &str,
    ) -> anyhow::Result<url::Url> {
        let short_id = test_collection_short_id
            .filter(|id| !id.is_empty())
            .ok_or_else(|| anyhow::anyhow!("The Trunk API v2 needs a test collection id."))?;
        let mut url = url::Url::parse(&self.api_host)?;
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Invalid Trunk API address: {}", self.api_host))?
            .pop_if_empty()
            .extend(["v2", "test-collections", short_id, route]);
        Ok(url)
    }

    pub async fn create_bundle_upload(
        &self,
        request: &message::CreateBundleUploadRequest,
    ) -> anyhow::Result<message::CreateBundleUploadResponse> {
        if self.upload_api_version == UploadApiVersion::V2 {
            return self
                .create_test_upload(request)
                .await
                .map_err(|e| e.context(ApiErrorEndpoint::CreateBundleUpload));
        }
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
                    StatusCodeHelp {
                        check_unauthorized: CheckUnauthorized::Check,
                        check_not_found: CheckNotFound::Check,
                        create_error_message: |_| String::from("Failed to create bundle upload."),
                        api_host: &self.api_host,
                        org_url_slug: &self.org_url_slug,
                    },
                )
                .await?;

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
        .map_err(|e| e.context(ApiErrorEndpoint::CreateBundleUpload))
    }

    /// Looks up the run's failing tests. A test runner checking a test at a time wants the whole
    /// set instead: `get_quarantined_test_set`.
    pub async fn get_quarantining_config(
        &self,
        request: &message::GetQuarantineConfigRequest,
    ) -> anyhow::Result<QuarantineLookup> {
        if self.upload_api_version == UploadApiVersion::V2 {
            return self
                .get_quarantine_status(request)
                .await
                .map_err(|e| e.context(ApiErrorEndpoint::GetQuarantiningConfig));
        }
        self.get_quarantine_config_v1(request)
            .await
            .map(QuarantineLookup::from)
    }

    /// Every quarantined test, for a test runner that checks each test as it runs. On v1 that is
    /// the lookup with no identifiers; on v2 it is the paged `all-quarantined-tests`.
    pub async fn get_quarantined_test_set(
        &self,
        request: &message::GetQuarantineConfigRequest,
    ) -> anyhow::Result<QuarantinedTestSet> {
        if self.upload_api_version == UploadApiVersion::V2 {
            return self
                .get_all_quarantined_tests(request)
                .await
                .map_err(|e| e.context(ApiErrorEndpoint::GetQuarantiningConfig));
        }
        let request = message::GetQuarantineConfigRequest {
            test_identifiers: Vec::new(),
            ..request.clone()
        };
        self.get_quarantine_config_v1(&request)
            .await
            .map(QuarantinedTestSet::from)
    }

    async fn get_quarantine_config_v1(
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
                    StatusCodeHelp {
                        check_unauthorized: CheckUnauthorized::Check,
                        check_not_found: CheckNotFound::DoNotCheck,
                        create_error_message: |response| -> String {
                            if response.status() == StatusCode::NOT_FOUND {
                                String::from("Quarantining config not found.")
                            } else  {
                                String::from("Failed to get quarantine bulk test.")
                            }
                        },
                        api_host: &self.api_host,
                        org_url_slug: &self.org_url_slug,
                    },
                )
                .await?;

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
        .map_err(|e| e.context(ApiErrorEndpoint::GetQuarantiningConfig))
    }

    async fn create_test_upload(
        &self,
        request: &message::CreateBundleUploadRequest,
    ) -> anyhow::Result<message::CreateBundleUploadResponse> {
        let client = self.v2_client()?;
        let url =
            self.v2_test_collection_url(request.test_collection_short_id.as_deref(), "uploads")?;
        let body = message::CreateTestUploadRequest::from(request);
        let upload = CallApi {
            action: || async {
                let response = client.post(url.clone()).json(&body).send().await?;
                let response = status_code_help(
                    response,
                    StatusCodeHelp {
                        check_unauthorized: CheckUnauthorized::Check,
                        check_not_found: CheckNotFound::DoNotCheck,
                        create_error_message: |response| {
                            if response.status() == StatusCode::NOT_FOUND {
                                String::from("Test collection not found.")
                            } else {
                                String::from("Failed to create bundle upload.")
                            }
                        },
                        api_host: &self.api_host,
                        org_url_slug: &self.org_url_slug,
                    },
                )
                .await?;
                self.deserialize_response::<message::CreatedTestUpload>(response).await
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
        .await?;

        let id_v2 = STANDARD.encode(serde_json::to_vec(&message::BundleUploadIdV2 {
            id: &upload.id,
            created_at: &upload.created_at,
            repo_id: &upload.repository_id,
        })?);
        Ok(message::CreateBundleUploadResponse {
            id: upload.id.clone(),
            id_v2,
            url: upload.upload_url,
            key: String::new(),
            test_collection_bundle_meta_id: Some(upload.id),
            test_collection_bundle_meta_created_at: Some(upload.created_at),
            repo_id: Some(upload.repository_id),
            test_collection_id: Some(upload.test_collection_uuid),
        })
    }

    async fn get_quarantine_status(
        &self,
        request: &message::GetQuarantineConfigRequest,
    ) -> anyhow::Result<QuarantineLookup> {
        let identifiers: Vec<message::QuarantineStatusIdentifier> = request
            .test_identifiers
            .iter()
            .map(message::QuarantineStatusIdentifier::from)
            .collect();
        let mut lookup = QuarantineLookup {
            quarantine_resolution_mode: message::QuarantineResolutionMode::TestCollection,
            ..QuarantineLookup::default()
        };
        for batch in identifiers.chunks(Self::QUARANTINE_STATUS_MAX_IDENTIFIERS) {
            let status = self.post_quarantine_status(request, batch).await?;
            lookup.quarantined_tests.extend(
                status
                    .tests
                    .into_iter()
                    .filter(|test| test.quarantined)
                    .filter_map(|test| test.client_test_id),
            );
            lookup.is_disabled = !status.quarantine_enabled;
            lookup.test_case_guid_scope = Some(TestCaseGuidScope {
                test_collection_id: status.test_collection_uuid,
                repo_id: status.repository_id,
            });
        }
        Ok(lookup)
    }

    async fn post_quarantine_status(
        &self,
        request: &message::GetQuarantineConfigRequest,
        identifiers: &[message::QuarantineStatusIdentifier],
    ) -> anyhow::Result<message::QuarantineStatus> {
        let client = self.v2_client()?;
        let url = self.v2_test_collection_url(
            request.test_collection_short_id.as_deref(),
            "quarantine-status",
        )?;
        let body = message::QuarantineStatusRequest {
            repo: request.repo.clone(),
            test_identifiers: identifiers.to_vec(),
        };
        CallApi {
            action: || async {
                let response = client.post(url.clone()).json(&body).send().await?;
                let response = status_code_help(
                    response,
                    StatusCodeHelp {
                        check_unauthorized: CheckUnauthorized::Check,
                        check_not_found: CheckNotFound::DoNotCheck,
                        create_error_message: |response| {
                            if response.status() == StatusCode::NOT_FOUND {
                                String::from("Test collection not found.")
                            } else {
                                String::from("Failed to get quarantine status.")
                            }
                        },
                        api_host: &self.api_host,
                        org_url_slug: &self.org_url_slug,
                    },
                )
                .await?;
                self.deserialize_response::<message::QuarantineStatus>(response).await
            },
            log_progress_message: |time_elapsed, _| {
                format!("Getting quarantine status from Trunk services is taking longer than expected. It has taken {} seconds so far.", time_elapsed.as_secs())
            },
            report_slow_progress_message: |time_elapsed| {
                format!("Getting a Trunk quarantine status is taking longer than {} seconds", time_elapsed.as_secs())
            },
            render_sender: self.render_sender.clone(),
        }
        .call_api()
        .await
    }

    async fn get_all_quarantined_tests(
        &self,
        request: &message::GetQuarantineConfigRequest,
    ) -> anyhow::Result<QuarantinedTestSet> {
        let client = self.v2_client()?;
        let base_url = self.v2_test_collection_url(
            request.test_collection_short_id.as_deref(),
            "all-quarantined-tests",
        )?;
        let repo = &request.repo;
        let has_repo = [&repo.host, &repo.owner, &repo.name]
            .iter()
            .all(|part| !part.is_empty());
        let mut guids = HashSet::new();
        let mut scope: Option<(TestCaseGuidScope, bool)> = None;
        let mut cursor: Option<String> = None;
        loop {
            let mut url = base_url.clone();
            {
                let mut query = url.query_pairs_mut();
                if has_repo {
                    query
                        .append_pair("repoHost", &repo.host)
                        .append_pair("repoOwner", &repo.owner)
                        .append_pair("repoName", &repo.name);
                }
                query.append_pair("limit", &Self::ALL_QUARANTINED_TESTS_PAGE_SIZE.to_string());
                if let Some(cursor) = cursor.as_deref() {
                    query.append_pair("cursor", cursor);
                }
            }
            let page = CallApi {
                action: || async {
                    let response = client.get(url.clone()).send().await?;
                    let response = status_code_help(
                        response,
                        StatusCodeHelp {
                            check_unauthorized: CheckUnauthorized::Check,
                            check_not_found: CheckNotFound::DoNotCheck,
                            create_error_message: |response| {
                                if response.status() == StatusCode::NOT_FOUND {
                                    String::from("Test collection not found.")
                                } else {
                                    String::from("Failed to get quarantined tests.")
                                }
                            },
                            api_host: &self.api_host,
                            org_url_slug: &self.org_url_slug,
                        },
                    )
                    .await?;
                    self.deserialize_response::<message::QuarantinedTestList>(response).await
                },
                log_progress_message: |time_elapsed, _| {
                    format!("Getting quarantined tests from Trunk services is taking longer than expected. It has taken {} seconds so far.", time_elapsed.as_secs())
                },
                report_slow_progress_message: |time_elapsed| {
                    format!("Getting Trunk quarantined tests is taking longer than {} seconds", time_elapsed.as_secs())
                },
                render_sender: self.render_sender.clone(),
            }
            .call_api()
            .await?;
            guids.extend(page.data.into_iter().map(|test| test.test_id));
            // Every page carries the same scope, so the first page's is the list's.
            let (scope, quarantine_enabled) = scope.get_or_insert((
                TestCaseGuidScope {
                    test_collection_id: page.test_collection_uuid,
                    repo_id: page.repository_id,
                },
                page.quarantine_enabled,
            ));
            match page.next_cursor {
                // A server that repeats a cursor would otherwise hold the CI step forever.
                Some(next) if cursor.as_deref() == Some(next.as_str()) => {
                    return Err(anyhow::anyhow!(
                        "The quarantined tests list repeated its cursor."
                    ));
                }
                Some(next) => cursor = Some(next),
                None => {
                    return Ok(QuarantinedTestSet {
                        is_disabled: !*quarantine_enabled,
                        quarantine_resolution_mode:
                            message::QuarantineResolutionMode::TestCollection,
                        tests: QuarantinedTests::ByTestCaseGuid {
                            scope: scope.clone(),
                            guids,
                        },
                    });
                }
            }
        }
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
                    StatusCodeHelp {
                        check_unauthorized: CheckUnauthorized::DoNotCheck,
                        check_not_found: CheckNotFound::DoNotCheck,
                        create_error_message: |_| String::from("Failed to upload bundle to S3."),
                        api_host: &self.api_host,
                        org_url_slug: &self.org_url_slug,
                    },
                )
                .await
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
        .map_err(|e| e.context(ApiErrorEndpoint::PutBundleToS3))
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
        .map_err(|e| e.context(ApiErrorEndpoint::TelemetryUploadMetrics))
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

/// The API's explanation of a client error, kept in the error chain so the error report can
/// print it beneath the endpoint context the callers add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientErrorExplanation {
    pub code: Option<String>,
    pub message: Option<String>,
}

impl fmt::Display for ClientErrorExplanation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.message, &self.code) {
            (Some(message), _) => f.write_str(message),
            (None, Some(code)) => write!(
                f,
                "The Trunk API rejected this request with error code {code}."
            ),
            (None, None) => Ok(()),
        }
    }
}

impl std::error::Error for ClientErrorExplanation {}

const MAX_CLIENT_ERROR_MESSAGE_LEN: usize = 2048;

fn is_retryable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<reqwest::Error>().is_some_and(|error| {
        error.is_timeout()
            || error.is_connect()
            || error
                .status()
                .is_some_and(|status| status.is_server_error())
    })
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn truncate_message(message: String) -> String {
    match message.char_indices().nth(MAX_CLIENT_ERROR_MESSAGE_LEN) {
        Some((end, _)) => format!("{}…", &message[..end]),
        None => message,
    }
}

async fn client_error_explanation(response: Response) -> Option<ClientErrorExplanation> {
    let body = response.text().await.ok()?;
    let message::ClientErrorBody { code, message } = serde_json::from_str(&body).ok()?;
    let explanation = ClientErrorExplanation {
        code: non_empty(code),
        message: non_empty(message).map(truncate_message),
    };
    (explanation.code.is_some() || explanation.message.is_some()).then_some(explanation)
}

pub(crate) struct StatusCodeHelp<'a, T: FnMut(&Response) -> String> {
    pub check_unauthorized: CheckUnauthorized,
    pub check_not_found: CheckNotFound,
    pub create_error_message: T,
    pub api_host: &'a str,
    pub org_url_slug: &'a str,
}

pub(crate) async fn status_code_help<T: FnMut(&Response) -> String>(
    response: Response,
    StatusCodeHelp {
        check_unauthorized,
        check_not_found,
        mut create_error_message,
        api_host,
        org_url_slug,
    }: StatusCodeHelp<'_, T>,
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

        match response.error_for_status_ref().err() {
            None => Err(anyhow::Error::msg(error_message)),
            Some(error) => {
                let error = anyhow::Error::from(error);
                Err(match client_error_explanation(response).await {
                    Some(explanation) => error.context(explanation),
                    None => error,
                })
            }
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
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{
        http::{HeaderMap, StatusCode},
        response::Response,
    };
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

        let mut api_client = ApiClient::new(
            Some(String::from("mock-token")),
            None,
            false,
            "mock-org",
            None,
        )
        .unwrap();
        api_client.api_host.clone_from(&state.host);

        assert!(
            format!(
                "{:?}",
                api_client
                    .get_quarantining_config(&message::GetQuarantineConfigRequest {
                        repo: context::repo::RepoUrlParts {
                            host: String::from("host"),
                            owner: String::from("owner"),
                            name: String::from("name"),
                        },
                        org_url_slug: String::from("org_url_slug"),
                        test_identifiers: vec![],
                        remote_urls: vec![],
                        test_collection_short_id: None,
                    })
                    .await
                    .unwrap_err()
            )
            .contains("501 Not Implemented")
        );
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

        let mut api_client = ApiClient::new(
            Some(String::from("mock-token")),
            None,
            false,
            "mock-org",
            None,
        )
        .unwrap();
        api_client.api_host.clone_from(&state.host);

        assert!(
            format!(
                "{:?}",
                api_client
                    .get_quarantining_config(&message::GetQuarantineConfigRequest {
                        repo: context::repo::RepoUrlParts {
                            host: String::from("api_host"),
                            owner: String::from("owner"),
                            name: String::from("name"),
                        },
                        remote_urls: vec![],
                        org_url_slug: String::from("org_url_slug"),
                        test_identifiers: vec![],
                        test_collection_short_id: None,
                    })
                    .await
                    .unwrap_err()
            )
            .contains("500 Internal Server Error")
        );
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

        let mut api_client = ApiClient::new(
            Some(String::from("mock-token")),
            None,
            false,
            "mock-org",
            None,
        )
        .unwrap();
        api_client.api_host.clone_from(&state.host);

        assert!(
            format!(
                "{:?}",
                api_client
                    .get_quarantining_config(&message::GetQuarantineConfigRequest {
                        repo: context::repo::RepoUrlParts {
                            host: String::from("api_host"),
                            owner: String::from("owner"),
                            name: String::from("name"),
                        },
                        remote_urls: vec![],
                        org_url_slug: String::from("org_url_slug"),
                        test_identifiers: vec![],
                        test_collection_short_id: None,
                    })
                    .await
                    .unwrap_err()
            )
            .contains("404 Not Found")
        );
    }

    #[test]
    fn requires_a_token_or_public_repo_id() {
        // Neither credential is an error.
        let err = ApiClient::new(None, None, false, "mock-org", None)
            .err()
            .expect("expected an error when no credentials are provided");
        assert!(err.to_string().contains(
            "Either a Trunk API token, a public repo id, or --allow-forked-pr-uploads is required"
        ));

        // Blank values are treated as absent.
        assert!(
            ApiClient::new(
                Some(String::from("   ")),
                Some(String::from("   ")),
                false,
                "mock-org",
                None
            )
            .is_err()
        );

        // A token alone is sufficient - no public repo id required.
        assert!(ApiClient::new(Some(String::from("token")), None, false, "mock-org", None).is_ok());

        // A public repo id alone is sufficient - no token required.
        assert!(
            ApiClient::new(
                None,
                Some(String::from("public-repo-id")),
                false,
                "mock-org",
                None
            )
            .is_ok()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn sends_public_repo_id_header_in_place_of_token() {
        let mut mock_server_builder = MockServerBuilder::new();

        lazy_static! {
            static ref CAPTURED_HEADERS: Arc<Mutex<HeaderMap>> =
                Arc::new(Mutex::new(HeaderMap::new()));
        }

        let quarantining_config_handler = move |headers: HeaderMap| async move {
            *CAPTURED_HEADERS.lock().unwrap() = headers;
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(String::from(
                    r#"{ "status_code": 404, "error": "not found" }"#,
                ))
                .unwrap()
        };
        mock_server_builder.set_get_quarantining_config_handler(quarantining_config_handler);

        let state = mock_server_builder.spawn_mock_server().await;

        let mut api_client = ApiClient::new(
            None,
            Some(String::from("public-repo-id-123")),
            false,
            "mock-org",
            None,
        )
        .unwrap();
        api_client.api_host.clone_from(&state.host);

        // The call itself errors (404) - we only care about the headers it sent.
        let _ = api_client
            .get_quarantining_config(&message::GetQuarantineConfigRequest {
                repo: context::repo::RepoUrlParts {
                    host: String::from("host"),
                    owner: String::from("owner"),
                    name: String::from("name"),
                },
                org_url_slug: String::from("org_url_slug"),
                test_identifiers: vec![],
                remote_urls: vec![],
                test_collection_short_id: None,
            })
            .await;

        let headers = CAPTURED_HEADERS.lock().unwrap();
        assert_eq!(
            headers
                .get(super::ApiClient::TRUNK_PUBLIC_REPO_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("public-repo-id-123")
        );
        assert!(
            headers
                .get(super::ApiClient::TRUNK_API_TOKEN_HEADER)
                .is_none(),
            "token header should be absent when only a public repo id is provided"
        );
    }

    #[tokio::test]
    async fn sends_allow_forked_pr_uploads_header_with_no_credential() {
        let mut mock_server_builder = MockServerBuilder::new();

        lazy_static! {
            static ref CAPTURED_HEADERS: Arc<Mutex<HeaderMap>> =
                Arc::new(Mutex::new(HeaderMap::new()));
        }

        let quarantining_config_handler = move |headers: HeaderMap| async move {
            *CAPTURED_HEADERS.lock().unwrap() = headers;
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(String::from(
                    r#"{ "status_code": 404, "error": "not found" }"#,
                ))
                .unwrap()
        };
        mock_server_builder.set_get_quarantining_config_handler(quarantining_config_handler);

        let state = mock_server_builder.spawn_mock_server().await;

        let mut api_client = ApiClient::new(None, None, true, "mock-org", None).unwrap();
        api_client.api_host.clone_from(&state.host);

        let _ = api_client
            .get_quarantining_config(&message::GetQuarantineConfigRequest {
                repo: context::repo::RepoUrlParts {
                    host: String::from("host"),
                    owner: String::from("owner"),
                    name: String::from("name"),
                },
                org_url_slug: String::from("org_url_slug"),
                test_identifiers: vec![],
                remote_urls: vec![],
                test_collection_short_id: Some(String::from("abc12345")),
            })
            .await;

        let headers = CAPTURED_HEADERS.lock().unwrap();
        assert_eq!(
            headers
                .get(super::ApiClient::TRUNK_ALLOW_FORKED_PR_UPLOADS_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );
        // The lane presents no credential at all — the collection id in the body is what the
        // server authorizes, against that collection's own opt-in.
        assert!(
            headers
                .get(super::ApiClient::TRUNK_API_TOKEN_HEADER)
                .is_none()
                && headers
                    .get(super::ApiClient::TRUNK_PUBLIC_REPO_ID_HEADER)
                    .is_none(),
            "no credential header should be sent on the forked-PR lane"
        );
    }

    #[test]
    fn allow_forked_pr_uploads_satisfies_the_credential_requirement() {
        assert!(ApiClient::new(None, None, true, "mock-org", None).is_ok());
    }
}

#[cfg(test)]
mod v2_tests {
    use std::{
        collections::HashSet,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use axum::{
        Json, Router,
        extract::{Query, State},
        http::{HeaderMap, StatusCode, Uri},
        response::IntoResponse,
        routing::{get, post},
    };
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use bundle::Test;
    use serde_json::{Value, json};
    use tokio::net::TcpListener;

    use super::ApiClient;
    use crate::message::{self, UploadApiCaller, UploadApiVersion};
    use crate::quarantine::{QuarantineLookup, QuarantinedTestSet, QuarantinedTests};
    use crate::urls::TestCaseGuidScope;

    const SHORT_ID: &str = "aB3xY9kQ";
    const COLLECTION_UUID: &str = "204b6c93-1c51-4e0f-a13c-e07b9bc60688";
    const REPO_ID: &str = "7a1f0e3d-2b4c-4d5e-8f90-123456789abc";

    #[derive(Default)]
    struct Seen {
        bodies: Vec<Value>,
        uris: Vec<String>,
        authorization: Vec<Option<String>>,
    }

    type Shared = Arc<Mutex<Seen>>;

    fn record(seen: &Shared, headers: &HeaderMap, uri: &Uri, body: Option<Value>) {
        let mut seen = seen.lock().unwrap();
        seen.uris.push(uri.to_string());
        seen.authorization.push(
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .map(String::from),
        );
        if let Some(body) = body {
            seen.bodies.push(body);
        }
    }

    async fn serve(router: Router<Shared>) -> (String, Shared) {
        let listener = TcpListener::bind("localhost:0").await.unwrap();
        let host = format!("http://localhost:{}", listener.local_addr().unwrap().port());
        let seen = Shared::default();
        let app = router.with_state(seen.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (host, seen)
    }

    fn client(host: &str, token: Option<&str>) -> ApiClient {
        let mut client =
            ApiClient::new(token.map(String::from), None, false, "acme", None).unwrap();
        client.api_host = String::from(host);
        client
    }

    fn repo() -> context::repo::RepoUrlParts {
        context::repo::RepoUrlParts {
            host: String::from("github.com"),
            owner: String::from("acme"),
            name: String::from("web"),
        }
    }

    fn version_request(short_id: Option<&str>) -> message::GetUploadApiVersionRequest {
        message::GetUploadApiVersionRequest {
            cli_version: String::from("0.17.0"),
            org_slug: String::from("acme"),
            test_collection_short_id: short_id.map(String::from),
            repo: Some(repo()),
            ci_provider: Some(String::from("GitHubActions")),
            is_fork_pr: false,
            caller: UploadApiCaller::TestRunnerPlugin,
        }
    }

    fn quarantine_request(test_identifiers: Vec<Test>) -> message::GetQuarantineConfigRequest {
        message::GetQuarantineConfigRequest {
            repo: repo(),
            remote_urls: vec![String::from("https://github.com/acme/web.git")],
            org_url_slug: String::from("acme"),
            test_identifiers,
            test_collection_short_id: Some(String::from(SHORT_ID)),
        }
    }

    fn failure(index: usize) -> Test {
        Test {
            id: format!("00000000-0000-4000-8000-{index:012}"),
            name: format!("test {index}"),
            parent_name: String::from("Suite"),
            class_name: Some(String::new()),
            file: None,
            timestamp_millis: None,
            is_quarantined: false,
            failure_message: None,
            variant: None,
        }
    }

    async fn decide(answer: (StatusCode, &'static str)) -> (UploadApiVersion, Shared) {
        let router = Router::new().route(
            "/v1/metrics/getUploadApiVersion",
            post(
                move |State(seen): State<Shared>,
                      headers: HeaderMap,
                      uri: Uri,
                      Json(body): Json<Value>| async move {
                    record(&seen, &headers, &uri, Some(body));
                    answer
                },
            ),
        );
        let (host, seen) = serve(router).await;
        let version = client(&host, Some("org-key"))
            .get_upload_api_version(&version_request(Some(SHORT_ID)))
            .await;
        (version, seen)
    }

    #[tokio::test]
    async fn sends_the_run_and_reads_an_explicit_v2() {
        let (version, seen) = decide((StatusCode::OK, r#"{"apiVersion":"v2"}"#)).await;

        assert_eq!(version, UploadApiVersion::V2);
        assert_eq!(
            seen.lock().unwrap().bodies,
            vec![json!({
                "cliVersion": "0.17.0",
                "orgSlug": "acme",
                "testCollectionShortId": SHORT_ID,
                "repo": {"host": "github.com", "owner": "acme", "name": "web"},
                "ciProvider": "GitHubActions",
                "isForkPr": false,
                "caller": "test-runner-plugin",
            })]
        );
    }

    #[tokio::test]
    async fn reads_any_other_answer_as_v1() {
        for (answer, attempts) in [
            ((StatusCode::OK, r#"{"apiVersion":"v1"}"#), 1),
            ((StatusCode::OK, r#"{"apiVersion":"v3"}"#), 1),
            ((StatusCode::OK, "not json"), 1),
            ((StatusCode::UNAUTHORIZED, ""), 1),
            // Only a 5xx, a timeout or a failed connection is worth a second attempt.
            (
                (StatusCode::INTERNAL_SERVER_ERROR, r#"{"apiVersion":"v2"}"#),
                2,
            ),
        ] {
            let (version, seen) = decide(answer).await;
            assert_eq!(version, UploadApiVersion::V1, "{answer:?}");
            assert_eq!(seen.lock().unwrap().bodies.len(), attempts, "{answer:?}");
        }
    }

    #[tokio::test]
    async fn retries_a_failed_check_once() {
        let router = Router::new().route(
            "/v1/metrics/getUploadApiVersion",
            post(
                |State(seen): State<Shared>, headers: HeaderMap, uri: Uri| async move {
                    record(&seen, &headers, &uri, None);
                    if seen.lock().unwrap().uris.len() == 1 {
                        StatusCode::SERVICE_UNAVAILABLE.into_response()
                    } else {
                        Json(json!({"apiVersion": "v2"})).into_response()
                    }
                },
            ),
        );
        let (host, seen) = serve(router).await;

        let version = client(&host, Some("org-key"))
            .get_upload_api_version(&version_request(Some(SHORT_ID)))
            .await;

        assert_eq!(
            (version, seen.lock().unwrap().uris.len()),
            (UploadApiVersion::V2, 2)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn reads_a_slow_answer_as_v1() {
        let router = Router::new().route(
            "/v1/metrics/getUploadApiVersion",
            post(|| async {
                tokio::time::sleep(Duration::from_secs(3)).await;
                Json(json!({"apiVersion": "v2"}))
            }),
        );
        let (host, _) = serve(router).await;

        let version = client(&host, Some("org-key"))
            .get_upload_api_version(&version_request(Some(SHORT_ID)))
            .await;

        assert_eq!(version, UploadApiVersion::V1);
    }

    #[tokio::test]
    async fn does_not_ask_without_a_token_or_a_collection() {
        let router = Router::new().route(
            "/v1/metrics/getUploadApiVersion",
            post(
                |State(seen): State<Shared>, headers: HeaderMap, uri: Uri| async move {
                    record(&seen, &headers, &uri, None);
                    Json(json!({"apiVersion": "v2"}))
                },
            ),
        );
        let (host, seen) = serve(router).await;

        let mut public_repo_client =
            ApiClient::new(None, Some(String::from("public-id")), false, "acme", None).unwrap();
        public_repo_client.api_host.clone_from(&host);
        assert_eq!(
            public_repo_client
                .get_upload_api_version(&version_request(Some(SHORT_ID)))
                .await,
            UploadApiVersion::V1
        );
        for short_id in [None, Some("")] {
            assert_eq!(
                client(&host, Some("org-key"))
                    .get_upload_api_version(&version_request(short_id))
                    .await,
                UploadApiVersion::V1
            );
        }
        assert!(seen.lock().unwrap().uris.is_empty());
    }

    #[tokio::test]
    async fn creates_a_v2_upload() {
        let router = Router::new().route(
            "/v2/test-collections/:id/uploads",
            post(
                |State(seen): State<Shared>,
                 headers: HeaderMap,
                 uri: Uri,
                 Json(body): Json<Value>| async move {
                    record(&seen, &headers, &uri, Some(body));
                    (
                        StatusCode::CREATED,
                        Json(json!({
                            "id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
                            "status": "awaitingData",
                            "uploadUrl": "https://bundles.example/key?sig=1",
                            "expiresAt": "2026-10-06T17:00:00.000Z",
                            "testCollectionUuid": COLLECTION_UUID,
                            "repositoryId": REPO_ID,
                            "createdAt": "2026-10-06T16:00:00.000Z",
                        })),
                    )
                },
            ),
        );
        let (host, seen) = serve(router).await;
        let mut client = client(&host, Some("org-key"));
        client.set_upload_api_version(UploadApiVersion::V2);

        let upload = client
            .create_bundle_upload(&message::CreateBundleUploadRequest {
                repo: repo(),
                org_url_slug: String::from("acme"),
                client_version: String::from("trunk-analytics-cli 0.17.0"),
                remote_urls: vec![String::from("https://github.com/acme/web.git")],
                external_id: None,
                test_collection_short_id: Some(String::from(SHORT_ID)),
            })
            .await
            .unwrap();

        let id_v2: Value =
            serde_json::from_slice(&STANDARD.decode(&upload.id_v2).unwrap()).unwrap();
        assert_eq!(
            (
                upload.id.as_str(),
                id_v2,
                upload.url.as_str(),
                upload.test_collection_bundle_meta_id.as_deref(),
                upload.test_collection_bundle_meta_created_at.as_deref(),
                upload.repo_id.as_deref(),
                upload.test_collection_id.as_deref(),
            ),
            (
                "f47ac10b-58cc-4372-a567-0e02b2c3d479",
                json!({
                    "id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
                    "createdAt": "2026-10-06T16:00:00.000Z",
                    "repoId": REPO_ID,
                }),
                "https://bundles.example/key?sig=1",
                Some("f47ac10b-58cc-4372-a567-0e02b2c3d479"),
                Some("2026-10-06T16:00:00.000Z"),
                Some(REPO_ID),
                // From the upload itself, so a run that makes no lookup still gets GUID links.
                Some(COLLECTION_UUID),
            )
        );
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen.uris,
            vec![format!("/v2/test-collections/{SHORT_ID}/uploads")]
        );
        assert_eq!(
            seen.authorization,
            vec![Some(String::from("Bearer org-key"))]
        );
        assert_eq!(
            seen.bodies,
            vec![json!({
                "repo": {"host": "github.com", "owner": "acme", "name": "web"},
                "remoteUrls": ["https://github.com/acme/web.git"],
                "clientVersion": "trunk-analytics-cli 0.17.0",
            })]
        );
    }

    fn all_quarantined_tests_router(quarantine_enabled: bool) -> Router<Shared> {
        Router::new().route(
            "/v2/test-collections/:id/all-quarantined-tests",
            get(
                move |State(seen): State<Shared>,
                      headers: HeaderMap,
                      uri: Uri,
                      Query(query): Query<std::collections::HashMap<String, String>>| async move {
                    record(&seen, &headers, &uri, None);
                    let (data, next_cursor) = match query.get("cursor").map(String::as_str) {
                        _ if !quarantine_enabled => (json!([]), json!(null)),
                        None => (
                            json!([{"testId": "bfeebcf4-72d1-887d-8bcd-788d0dec7f97"}]),
                            json!("page-2"),
                        ),
                        Some("page-2") => (
                            json!([{"testId": "0f7f61f2-0000-8000-8000-000000000000"}]),
                            json!(null),
                        ),
                        Some(_) => return StatusCode::BAD_REQUEST.into_response(),
                    };
                    Json(json!({
                        "data": data,
                        "nextCursor": next_cursor,
                        "hasMore": !next_cursor.is_null(),
                        "testCollectionUuid": COLLECTION_UUID,
                        "repositoryId": REPO_ID,
                        "quarantineEnabled": quarantine_enabled,
                    }))
                    .into_response()
                },
            ),
        )
    }

    fn quarantine_status_router(quarantine_enabled: bool) -> Router<Shared> {
        Router::new().route(
            "/v2/test-collections/:id/quarantine-status",
            post(
                move |State(seen): State<Shared>,
                      headers: HeaderMap,
                      uri: Uri,
                      Json(body): Json<Value>| async move {
                    let tests: Vec<Value> = body["testIdentifiers"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|identifier| {
                            json!({
                                "clientTestId": identifier["id"],
                                "testId": null,
                                "quarantined": identifier["id"] != "00000000-0000-4000-8000-000000000000",
                            })
                        })
                        .collect();
                    record(&seen, &headers, &uri, Some(body));
                    Json(json!({
                        "testCollectionUuid": COLLECTION_UUID,
                        "repositoryId": REPO_ID,
                        "quarantineEnabled": quarantine_enabled,
                        "tests": tests,
                    }))
                },
            ),
        )
    }

    fn scope() -> Option<TestCaseGuidScope> {
        Some(TestCaseGuidScope {
            test_collection_id: String::from(COLLECTION_UUID),
            repo_id: String::from(REPO_ID),
        })
    }

    #[tokio::test]
    async fn looks_up_failures_in_batches_of_2000() {
        let (host, seen) = serve(quarantine_status_router(true)).await;
        let mut client = client(&host, Some("org-key"));
        client.set_upload_api_version(UploadApiVersion::V2);
        let failures: Vec<Test> = (0..2001).map(failure).collect();

        let config = client
            .get_quarantining_config(&quarantine_request(failures.clone()))
            .await
            .unwrap();

        assert_eq!(
            config,
            QuarantineLookup {
                is_disabled: false,
                quarantined_tests: failures[1..].iter().map(|test| test.id.clone()).collect(),
                quarantine_resolution_mode: message::QuarantineResolutionMode::TestCollection,
                test_case_guid_scope: scope(),
            }
        );
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen.bodies
                .iter()
                .map(|body| body["testIdentifiers"].as_array().unwrap().len())
                .collect::<Vec<_>>(),
            vec![2000, 1]
        );
        assert_eq!(
            seen.bodies[1],
            json!({
                "repo": {"host": "github.com", "owner": "acme", "name": "web"},
                "testIdentifiers": [{"id": failures[2000].id}],
            })
        );
        assert_eq!(
            seen.authorization,
            vec![Some(String::from("Bearer org-key")); 2]
        );
    }

    #[tokio::test]
    async fn makes_no_request_to_look_up_no_tests() {
        let (host, seen) = serve(quarantine_status_router(true)).await;
        let mut client = client(&host, Some("org-key"));
        client.set_upload_api_version(UploadApiVersion::V2);

        let config = client
            .get_quarantining_config(&quarantine_request(vec![]))
            .await
            .unwrap();

        assert_eq!(
            (config.quarantined_tests, config.test_case_guid_scope),
            (Vec::<String>::new(), None)
        );
        assert!(seen.lock().unwrap().uris.is_empty());
    }

    #[tokio::test]
    async fn pages_the_whole_quarantined_set_with_no_status_call() {
        let (host, seen) =
            serve(all_quarantined_tests_router(true).merge(quarantine_status_router(true))).await;
        let mut client = client(&host, Some("org-key"));
        client.set_upload_api_version(UploadApiVersion::V2);

        let set = client
            .get_quarantined_test_set(&quarantine_request(vec![]))
            .await
            .unwrap();

        assert_eq!(
            set,
            QuarantinedTestSet {
                is_disabled: false,
                quarantine_resolution_mode: message::QuarantineResolutionMode::TestCollection,
                tests: QuarantinedTests::ByTestCaseGuid {
                    scope: scope().unwrap(),
                    guids: HashSet::from([
                        String::from("bfeebcf4-72d1-887d-8bcd-788d0dec7f97"),
                        String::from("0f7f61f2-0000-8000-8000-000000000000"),
                    ]),
                },
            }
        );
        assert_eq!(
            seen.lock().unwrap().uris,
            vec![
                format!(
                    "/v2/test-collections/{SHORT_ID}/all-quarantined-tests?repoHost=github.com&repoOwner=acme&repoName=web&limit=25000"
                ),
                format!(
                    "/v2/test-collections/{SHORT_ID}/all-quarantined-tests?repoHost=github.com&repoOwner=acme&repoName=web&limit=25000&cursor=page-2"
                ),
            ]
        );
    }

    #[tokio::test]
    async fn names_no_repository_for_an_upload_without_one() {
        let (host, seen) = serve(all_quarantined_tests_router(false)).await;
        let mut client = client(&host, Some("org-key"));
        client.set_upload_api_version(UploadApiVersion::V2);
        let request = message::GetQuarantineConfigRequest {
            repo: context::repo::RepoUrlParts::default(),
            ..quarantine_request(vec![])
        };

        client.get_quarantined_test_set(&request).await.unwrap();

        assert_eq!(
            seen.lock().unwrap().uris,
            vec![format!(
                "/v2/test-collections/{SHORT_ID}/all-quarantined-tests?limit=25000"
            )]
        );
    }

    #[tokio::test]
    async fn reads_quarantining_off_from_the_list() {
        let (host, seen) = serve(all_quarantined_tests_router(false)).await;
        let mut client = client(&host, Some("org-key"));
        client.set_upload_api_version(UploadApiVersion::V2);

        let set = client
            .get_quarantined_test_set(&quarantine_request(vec![]))
            .await
            .unwrap();

        assert_eq!(
            (set.is_disabled, set.tests),
            (
                true,
                QuarantinedTests::ByTestCaseGuid {
                    scope: scope().unwrap(),
                    guids: HashSet::new(),
                },
            )
        );
        assert_eq!(seen.lock().unwrap().uris.len(), 1);
    }

    #[tokio::test]
    async fn stops_paging_on_a_repeated_cursor() {
        let router = Router::new().route(
            "/v2/test-collections/:id/all-quarantined-tests",
            get(|| async {
                Json(json!({
                    "data": [],
                    "nextCursor": "same",
                    "hasMore": true,
                    "testCollectionUuid": COLLECTION_UUID,
                    "repositoryId": REPO_ID,
                    "quarantineEnabled": true,
                }))
            }),
        );
        let (host, _) = serve(router).await;
        let mut client = client(&host, Some("org-key"));
        client.set_upload_api_version(UploadApiVersion::V2);

        let error = client
            .get_quarantined_test_set(&quarantine_request(vec![]))
            .await
            .unwrap_err();

        assert!(
            format!("{error:?}").contains("repeated its cursor"),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn reports_a_failed_v2_lookup_without_falling_back_to_v1() {
        let router = Router::new()
            .route(
                "/v2/test-collections/:id/quarantine-status",
                post(|State(seen): State<Shared>, headers: HeaderMap, uri: Uri| async move {
                    record(&seen, &headers, &uri, None);
                    (
                        StatusCode::NOT_FOUND,
                        r#"{"error":{"code":"NOT_FOUND","message":"Test collection not found."}}"#,
                    )
                }),
            )
            .route(
                "/v1/metrics/getQuarantineConfig",
                post(|State(seen): State<Shared>, headers: HeaderMap, uri: Uri| async move {
                    record(&seen, &headers, &uri, None);
                    Json(json!({"isDisabled": false, "testIds": []}))
                }),
            );
        let (host, seen) = serve(router).await;
        let mut client = client(&host, Some("org-key"));
        client.set_upload_api_version(UploadApiVersion::V2);

        let error = client
            .get_quarantining_config(&quarantine_request(vec![failure(0)]))
            .await
            .unwrap_err();

        assert!(format!("{error:?}").contains("404 Not Found"), "{error:?}");
        assert_eq!(
            seen.lock().unwrap().uris,
            vec![format!("/v2/test-collections/{SHORT_ID}/quarantine-status")]
        );
    }
}
