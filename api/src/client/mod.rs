use std::fmt;
use std::path::Path;
use std::sync::mpsc::Sender;

use constants::{DEFAULT_ORIGIN, TRUNK_PUBLIC_API_ADDRESS_ENV};
use display::message::DisplayMessage;
use http::{HeaderName, HeaderValue, header::HeaderMap};
use reqwest::{Client, Response, StatusCode, header};
use serde::de::DeserializeOwned;
use tokio::fs;

use crate::call_api::CallApi;
use crate::credential::{self, LegacyTokenRefused, TokenKind};
use crate::message;
use crate::quarantine::{QuarantineLookup, QuarantinedTestSet};

mod v2;

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
    s3_client: Client,
    /// Holds the run's credential: an API token from the start, or a fork pull request's Trunk
    /// login once it has one.
    trunk_api_v2_client: Option<Client>,
    /// What `trunk_api_v2_client` presents, for the message when Trunk refuses it.
    credential: Option<RunCredential>,
    /// `--allow-forked-pr-uploads` with no token.
    credential_free: bool,
    /// A credential-free GitHub Actions fork PR run, which swaps its `GITHUB_TOKEN` for a login.
    github_fork_pull_request: Option<GithubForkPullRequest>,
    /// Why a fork pull request's login failed, for the error every later call returns.
    github_fork_login_failure: Option<String>,
    /// The run's v2 API key or Trunk JWT, for the CLI telemetry route; `None` without one (a fork
    /// pull request has neither, and the gateway refuses its GitHub Actions login there).
    telemetry_client: Option<Client>,
    org_url_slug: String,
    render_sender: Option<Sender<DisplayMessage>>,
}

/// A GitHub Actions run of a pull request from a fork, as its event payload names it.
pub struct GithubForkPullRequest {
    pub number: u64,
    /// `pull_request.head.sha`, not `GITHUB_SHA`, which is the merge commit on a PR run.
    pub head_sha: String,
    /// The job's `GITHUB_TOKEN`, when the step was given it.
    pub github_token: Option<String>,
}

/// What a v2 call presents. The gateway takes only an API key on `Authorization`, so a Trunk JWT
/// goes on `x-trunk-token` and the two are never sent together.
pub(crate) enum V2Credential {
    ApiKey(String),
    Jwt(String),
}

/// Where the run's credential came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunCredential {
    ApiKey,
    /// A Trunk JWT passed as the token, with its `exp` when it has one.
    Jwt {
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    },
    /// A fork pull request's GitHub Actions login.
    GithubActionsLogin,
}

impl V2Credential {
    const TRUNK_TOKEN_HEADER: HeaderName = HeaderName::from_static("x-trunk-token");

    fn client(&self) -> anyhow::Result<Client> {
        self.client_with_timeout(ApiClient::TRUNK_API_TIMEOUT)
    }

    fn client_with_timeout(&self, timeout: std::time::Duration) -> anyhow::Result<Client> {
        let (name, value, error) = match self {
            Self::ApiKey(token) => (
                header::AUTHORIZATION,
                format!("Bearer {token}"),
                "Trunk API token is not ASCII",
            ),
            Self::Jwt(jwt) => (
                Self::TRUNK_TOKEN_HEADER,
                jwt.clone(),
                "Trunk JWT is not ASCII",
            ),
        };
        let mut value = HeaderValue::from_str(&value).map_err(|_| anyhow::Error::msg(error))?;
        value.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.append(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.append(name, value);
        Ok(Client::builder()
            .timeout(timeout)
            .default_headers(headers)
            .build()?)
    }
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
    const TRUNK_ALLOW_FORKED_PR_UPLOADS_HEADER: &'static str = "x-trunk-allow-forked-pr-uploads";

    /// Refuses a token that is neither a v2 API key nor a Trunk JWT with `LegacyTokenRefused`,
    /// before any call.
    pub fn new(
        api_token: Option<String>,
        allow_forked_pr_uploads: bool,
        org_url_slug: impl AsRef<str>,
        render_sender: Option<Sender<DisplayMessage>>,
    ) -> anyhow::Result<ApiClient> {
        let org_url_slug = String::from(org_url_slug.as_ref());

        let api_token = api_token
            .map(|token| token.trim().to_owned())
            .filter(|token| !token.is_empty());
        if api_token.is_none() && !allow_forked_pr_uploads {
            return Err(anyhow::anyhow!(
                "Either a Trunk API token or --allow-forked-pr-uploads is required."
            ));
        }

        let api_host = get_api_host();
        tracing::debug!("Using public api address {}", api_host);

        let (trunk_api_v2_client, telemetry_client, credential) = match api_token.clone() {
            None => (None, None, None),
            Some(token) => match TokenKind::of(&token) {
                TokenKind::ApiKey => {
                    let v2 = V2Credential::ApiKey(token);
                    (
                        Some(v2.client()?),
                        Some(v2.client_with_timeout(Self::TRUNK_TELEMETRY_TIMEOUT)?),
                        Some(RunCredential::ApiKey),
                    )
                }
                TokenKind::Jwt => {
                    let expires_at = credential::jwt_expiry(&token);
                    let v2 = V2Credential::Jwt(token);
                    (
                        Some(v2.client()?),
                        Some(v2.client_with_timeout(Self::TRUNK_TELEMETRY_TIMEOUT)?),
                        Some(RunCredential::Jwt { expires_at }),
                    )
                }
                TokenKind::Legacy => return Err(anyhow::Error::new(LegacyTokenRefused)),
            },
        };
        let credential_free = allow_forked_pr_uploads && api_token.is_none();

        let mut s3_client_default_headers = HeaderMap::new();
        s3_client_default_headers.append(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        let s3_client = Client::builder()
            .default_headers(s3_client_default_headers)
            .build()?;

        Ok(ApiClient {
            api_host,
            s3_client,
            trunk_api_v2_client,
            credential,
            credential_free,
            github_fork_pull_request: None,
            github_fork_login_failure: None,
            telemetry_client,
            org_url_slug,
            render_sender,
        })
    }

    /// Lets a credential-free run of a GitHub Actions fork PR log in. Ignored for any other run.
    pub fn set_github_fork_pull_request(&mut self, pull_request: GithubForkPullRequest) {
        if self.credential_free {
            self.github_fork_pull_request = Some(pull_request);
        }
    }

    pub async fn create_bundle_upload(
        &self,
        request: &message::CreateBundleUploadRequest,
    ) -> anyhow::Result<message::CreateBundleUploadResponse> {
        self.create_test_upload(request)
            .await
            .map_err(|e| e.context(ApiErrorEndpoint::CreateBundleUpload))
    }

    /// Looks up the run's failing tests. A test runner checking a test at a time wants the whole
    /// set instead: `get_quarantined_test_set`.
    pub async fn get_quarantining_config(
        &self,
        request: &message::GetQuarantineConfigRequest,
    ) -> anyhow::Result<QuarantineLookup> {
        self.get_quarantine_status(request)
            .await
            .map_err(|e| e.context(ApiErrorEndpoint::GetQuarantiningConfig))
    }

    /// Every quarantined test, for a test runner that checks each test as it runs: the paged
    /// `all-quarantined-tests`.
    pub async fn get_quarantined_test_set(
        &self,
        request: &message::GetQuarantineConfigRequest,
    ) -> anyhow::Result<QuarantinedTestSet> {
        self.get_all_quarantined_tests(request)
            .await
            .map_err(|e| e.context(ApiErrorEndpoint::GetQuarantiningConfig))
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
        let Some(telemetry_client) = &self.telemetry_client else {
            return Ok(());
        };
        let environment = if self.api_host == DEFAULT_ORIGIN {
            "production"
        } else {
            "staging"
        };
        CallApi {
            action: || async {
                if std::env::var("DISABLE_TELEMETRY").is_ok() {
                    return Ok(());
                }
                let response = telemetry_client
                    .post(format!("{}{}", self.api_host, crate::otlp::CLI_TELEMETRY_PATH))
                    .json(&crate::otlp::upload_metrics_logs(
                        &request.upload_metrics,
                        environment,
                    ))
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

pub(crate) const UNAUTHORIZED_CONTEXT: &str = concat!(
    "Your Trunk API key may be incorrect - find it in the Trunk app ",
    "(Settings -> Developer -> API Keys).",
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
    let message::ClientErrorEnvelope {
        error: message::ClientErrorBody { code, message },
    } = serde_json::from_str(&body).ok()?;
    let explanation = ClientErrorExplanation {
        code: non_empty(code),
        message: non_empty(message).map(truncate_message),
    };
    (explanation.code.is_some() || explanation.message.is_some()).then_some(explanation)
}

pub(crate) struct StatusCodeHelp<'a, T: FnMut(&Response) -> String> {
    pub check_unauthorized: CheckUnauthorized,
    pub create_error_message: T,
    pub api_host: &'a str,
    pub org_url_slug: &'a str,
}

pub(crate) async fn status_code_help<T: FnMut(&Response) -> String>(
    response: Response,
    StatusCodeHelp {
        check_unauthorized,
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
        let error_message = match (response.status(), check_unauthorized) {
            (StatusCode::UNAUTHORIZED, CheckUnauthorized::Check) => add_settings_url_to_context(
                UNAUTHORIZED_CONTEXT,
                domain,
                &String::from(org_url_slug),
            ),
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
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{http::StatusCode, response::Response};
    use lazy_static::lazy_static;
    use test_utils::mock_server::MockServerBuilder;

    use super::ApiClient;
    use crate::message;

    fn lookup_one_failure() -> message::GetQuarantineConfigRequest {
        message::GetQuarantineConfigRequest {
            repo: context::repo::RepoUrlParts {
                host: String::from("host"),
                owner: String::from("owner"),
                name: String::from("name"),
            },
            org_url_slug: String::from("org_url_slug"),
            test_identifiers: vec![bundle::Test {
                id: String::from("00000000-0000-4000-8000-000000000000"),
                name: String::from("test"),
                parent_name: String::from("Suite"),
                class_name: None,
                file: None,
                timestamp_millis: None,
                is_quarantined: false,
                failure_message: None,
                failure_message_attribute: None,
                failure_text: None,
                variant: None,
            }],
            remote_urls: vec![],
            test_collection_short_id: Some(String::from("abc12345")),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn does_not_retry_on_ok_501() {
        let mut mock_server_builder = MockServerBuilder::new();

        lazy_static! {
            static ref CALL_COUNT: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        }

        mock_server_builder.set_quarantine_status_handler(move || async {
            CALL_COUNT.fetch_add(1, Ordering::Relaxed);
            Response::builder()
                .status(StatusCode::NOT_IMPLEMENTED)
                .body(String::from(
                    r#"{"error":{"code":"INTERNAL_ERROR","message":"we broke"}}"#,
                ))
                .unwrap()
        });

        let state = mock_server_builder.spawn_mock_server().await;

        let mut api_client = ApiClient::new(
            Some(String::from("trunk_v2_mock-token")),
            false,
            "mock-org",
            None,
        )
        .unwrap();
        api_client.api_host.clone_from(&state.host);

        let error = api_client
            .get_quarantining_config(&lookup_one_failure())
            .await
            .unwrap_err();
        assert!(format!("{error:?}").contains("501 Not Implemented"));
        assert_eq!(CALL_COUNT.load(Ordering::Relaxed), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn retries_on_ok_500() {
        let mut mock_server_builder = MockServerBuilder::new();

        lazy_static! {
            static ref CALL_COUNT: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        }

        mock_server_builder.set_quarantine_status_handler(move || async {
            CALL_COUNT.fetch_add(1, Ordering::Relaxed);
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(String::from(
                    r#"{"error":{"code":"INTERNAL_ERROR","message":"we broke"}}"#,
                ))
                .unwrap()
        });

        let state = mock_server_builder.spawn_mock_server().await;

        let mut api_client = ApiClient::new(
            Some(String::from("trunk_v2_mock-token")),
            false,
            "mock-org",
            None,
        )
        .unwrap();
        api_client.api_host.clone_from(&state.host);

        let error = api_client
            .get_quarantining_config(&lookup_one_failure())
            .await
            .unwrap_err();
        assert!(format!("{error:?}").contains("500 Internal Server Error"));
        assert_eq!(CALL_COUNT.load(Ordering::Relaxed), 6);
    }

    #[tokio::test(start_paused = true)]
    async fn get_quarantining_config_not_found() {
        let mut mock_server_builder = MockServerBuilder::new();

        mock_server_builder.set_quarantine_status_handler(|| async {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(String::from(
                    r#"{"error":{"code":"TEST_COLLECTION_NOT_FOUND","message":"Test collection not found"}}"#,
                ))
                .unwrap()
        });

        let state = mock_server_builder.spawn_mock_server().await;

        let mut api_client = ApiClient::new(
            Some(String::from("trunk_v2_mock-token")),
            false,
            "mock-org",
            None,
        )
        .unwrap();
        api_client.api_host.clone_from(&state.host);

        let error = api_client
            .get_quarantining_config(&lookup_one_failure())
            .await
            .unwrap_err();
        assert!(format!("{error:?}").contains("404 Not Found"));
    }

    #[test]
    fn requires_a_token_or_allow_forked_pr_uploads() {
        let err = ApiClient::new(None, false, "mock-org", None)
            .err()
            .expect("expected an error when no credentials are provided");
        assert!(
            err.to_string()
                .contains("Either a Trunk API token or --allow-forked-pr-uploads is required")
        );

        // A blank token is treated as absent.
        assert!(ApiClient::new(Some(String::from("   ")), false, "mock-org", None).is_err());

        assert!(
            ApiClient::new(
                Some(String::from("trunk_v2_token")),
                false,
                "mock-org",
                None
            )
            .is_ok()
        );
    }

    #[test]
    fn allow_forked_pr_uploads_satisfies_the_credential_requirement() {
        assert!(ApiClient::new(None, true, "mock-org", None).is_ok());
    }

    #[tokio::test]
    async fn a_credential_free_run_that_is_not_a_fork_pr_makes_no_request() {
        let state = MockServerBuilder::new().spawn_mock_server().await;
        let mut api_client = ApiClient::new(None, true, "mock-org", None).unwrap();
        api_client.api_host.clone_from(&state.host);

        api_client.log_in_github_fork_pull_request().await;
        let error = api_client
            .get_quarantining_config(&lookup_one_failure())
            .await
            .unwrap_err();

        assert!(
            format!("{error:?}")
                .contains("from a GitHub Actions run of a pull request from a fork"),
            "{error:?}"
        );
        assert!(state.requests.lock().unwrap().is_empty());
    }
}
