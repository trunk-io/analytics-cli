use reqwest::StatusCode;

use super::{ApiClient, CheckNotFound, CheckUnauthorized, StatusCodeHelp, status_code_help};
use crate::call_api::CallApi;
use crate::message;

impl ApiClient {
    pub(super) async fn create_bundle_upload_v1(
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
    }

    pub(super) async fn get_quarantine_config_v1(
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
    }
}
