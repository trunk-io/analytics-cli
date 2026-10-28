use std::collections::HashSet;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::{Client, StatusCode};

use super::{ApiClient, CheckNotFound, CheckUnauthorized, StatusCodeHelp, status_code_help};
use crate::call_api::CallApi;
use crate::message;
use crate::quarantine::{QuarantineLookup, QuarantinedTestSet, QuarantinedTests};
use crate::urls::TestCaseGuidScope;

impl ApiClient {
    const QUARANTINE_STATUS_MAX_IDENTIFIERS: usize = 2_000;
    const ALL_QUARANTINED_TESTS_PAGE_SIZE: usize = 25_000;

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
}
