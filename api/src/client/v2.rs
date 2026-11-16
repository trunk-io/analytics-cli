use std::collections::HashSet;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::{Client, StatusCode};

use super::{
    ApiClient, CheckUnauthorized, GithubForkPullRequest, StatusCodeHelp, V2Credential,
    status_code_help,
};
use crate::call_api::CallApi;
use crate::message;
use crate::quarantine::{QuarantineLookup, QuarantinedTestSet};
use crate::urls::TestCaseGuidScope;

impl ApiClient {
    const QUARANTINE_STATUS_MAX_IDENTIFIERS: usize = 2_000;
    const CREDENTIAL_FREE_NOT_A_FORK_PR: &'static str = "--allow-forked-pr-uploads without a Trunk API token uploads only from a GitHub Actions run of a pull request from a fork; test results will not be uploaded.";
    const ALL_QUARANTINED_TESTS_PAGE_SIZE: usize = 25_000;

    /// Swaps a credential-free GitHub Actions fork PR run's `GITHUB_TOKEN` for a Trunk login. Any
    /// other run already holds its credential, or has none to swap. A failed swap is kept, and
    /// every later call fails with its reason.
    pub async fn log_in_github_fork_pull_request(&mut self) {
        if self.trunk_api_v2_client.is_some() {
            return;
        }
        let Some(pull_request) = &self.github_fork_pull_request else {
            tracing::warn!("{}", Self::CREDENTIAL_FREE_NOT_A_FORK_PR);
            return;
        };
        match self.github_actions_login(pull_request).await {
            Ok(client) => self.trunk_api_v2_client = Some(client),
            Err(reason) => {
                tracing::warn!(
                    "Could not log in to Trunk for this fork pull request ({reason}); test results will not be uploaded"
                );
                self.github_fork_login_failure = Some(reason);
            }
        }
    }

    /// `POST /v2/auth/github-actions/login` with the job's `GITHUB_TOKEN`. The error is a reason
    /// fit to log: it never holds the token, the login, or a parse error that could quote them.
    async fn github_actions_login(
        &self,
        pull_request: &GithubForkPullRequest,
    ) -> Result<Client, String> {
        let github_token = pull_request
            .github_token
            .as_deref()
            .filter(|token| !token.trim().is_empty())
            .ok_or("GITHUB_TOKEN is not set")?;
        let mut url = url::Url::parse(&self.api_host).map_err(|error| error.to_string())?;
        url.path_segments_mut()
            .map_err(|_| format!("invalid Trunk API address {}", self.api_host))?
            .pop_if_empty()
            .extend(["v2", "auth", "github-actions", "login"]);
        let response = Client::builder()
            .timeout(Self::TRUNK_API_TIMEOUT)
            .build()
            .map_err(|error| error.to_string())?
            .post(url)
            .header(Self::TRUNK_ALLOW_FORKED_PR_UPLOADS_HEADER, "true")
            .json(&message::GithubActionsForkLoginRequest {
                github_token,
                pull_request_number: pull_request.number,
                sha: &pull_request.head_sha,
            })
            .send()
            .await
            .map_err(|error| format!("the request failed: {}", error.without_url()))?;
        let status = response.status();
        if status != StatusCode::OK {
            let code = response
                .json::<message::ClientErrorEnvelope>()
                .await
                .ok()
                .and_then(|refusal| refusal.error.code);
            return Err(match code {
                Some(code) => format!("Trunk answered {status}, {code}"),
                None => format!("Trunk answered {status}"),
            });
        }
        let login = response
            .json::<message::GithubActionsLogin>()
            .await
            .ok()
            .filter(|login| !login.jwt.is_empty())
            .ok_or("Trunk's answer could not be read")?;
        tracing::debug!("Logged in to the Trunk API v2 until {}", login.expires_at);
        V2Credential::TrunkLogin(login.jwt)
            .client()
            .map_err(|error| error.to_string())
    }

    fn v2_client(&self) -> anyhow::Result<&Client> {
        if let Some(client) = &self.trunk_api_v2_client {
            return Ok(client);
        }
        Err(match &self.github_fork_login_failure {
            Some(reason) => {
                anyhow::anyhow!("Could not log in to Trunk for this fork pull request ({reason}).")
            }
            None if self.credential_free => anyhow::anyhow!(Self::CREDENTIAL_FREE_NOT_A_FORK_PR),
            None => anyhow::anyhow!("The Trunk API needs a Trunk API token."),
        })
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

    pub(super) async fn create_test_upload(
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
            test_collection_bundle_meta_id: Some(upload.id),
            test_collection_bundle_meta_created_at: Some(upload.created_at),
            repo_id: Some(upload.repository_id),
            test_collection_id: Some(upload.test_collection_uuid),
        })
    }

    pub(super) async fn get_quarantine_status(
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
            repo: message::named_repo(&request.repo),
            test_identifiers: identifiers,
        };
        CallApi {
            action: || async {
                let response = client.post(url.clone()).json(&body).send().await?;
                let response = status_code_help(
                    response,
                    StatusCodeHelp {
                        check_unauthorized: CheckUnauthorized::Check,
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

    pub(super) async fn get_all_quarantined_tests(
        &self,
        request: &message::GetQuarantineConfigRequest,
    ) -> anyhow::Result<QuarantinedTestSet> {
        let client = self.v2_client()?;
        let base_url = self.v2_test_collection_url(
            request.test_collection_short_id.as_deref(),
            "all-quarantined-tests",
        )?;
        let body = message::AllQuarantinedTestsRequest {
            repo: message::named_repo(&request.repo),
        };
        let mut guids = HashSet::new();
        let mut scope: Option<(TestCaseGuidScope, bool)> = None;
        let mut cursor: Option<String> = None;
        loop {
            let mut url = base_url.clone();
            {
                let mut query = url.query_pairs_mut();
                query.append_pair("limit", &Self::ALL_QUARANTINED_TESTS_PAGE_SIZE.to_string());
                if let Some(cursor) = cursor.as_deref() {
                    query.append_pair("cursor", cursor);
                }
            }
            let page = CallApi {
                action: || async {
                    let response = client.post(url.clone()).json(&body).send().await?;
                    let response = status_code_help(
                        response,
                        StatusCodeHelp {
                            check_unauthorized: CheckUnauthorized::Check,
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
                        scope: scope.clone(),
                        guids,
                    });
                }
            }
        }
    }
}
