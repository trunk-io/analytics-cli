use api::client::get_api_host;
use serde::Serialize;

use crate::error_report::{ErrorKind, ErrorReport, get_client_error_explanation, settings_url};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorSummary {
    pub kind: ErrorKind,
    pub message: Option<String>,
    pub error: String,
    pub causes: Vec<String>,
    pub explanation: Option<String>,
    pub settings_url: Option<String>,
    pub exit_code: Option<i32>,
}

impl ErrorSummary {
    pub fn new(report: &ErrorReport) -> Self {
        let ErrorReport {
            error,
            context,
            kind,
        } = report;
        Self {
            kind: *kind,
            message: context.base_message.clone(),
            error: error.to_string(),
            causes: error.chain().skip(1).map(ToString::to_string).collect(),
            explanation: get_client_error_explanation(error),
            settings_url: (*kind == ErrorKind::Unauthorized)
                .then(|| settings_url(&get_api_host(), &context.org_url_slug)),
            exit_code: context.exit_code,
        }
    }
}
