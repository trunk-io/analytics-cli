use api::client::get_api_host;
use schemars::JsonSchema;
use serde::Serialize;

use crate::error_report::{ErrorKind, ErrorReport, get_client_error_explanation, settings_url};

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ErrorSummary {
    pub kind: ErrorKind,
    /// What the CLI was doing when the error happened.
    pub message: Option<String>,
    /// The error, without its causes.
    pub error: String,
    /// The errors that led to it, outermost first.
    pub causes: Vec<String>,
    /// Trunk's explanation, when its API gave one.
    pub explanation: Option<String>,
    /// Where to check the org slug and token. Only set for `UNAUTHORIZED` errors.
    pub settings_url: Option<String>,
    /// A run that fails outright exits with this code, or 0 when it's null. Upload reports exit with their own `exitCode`.
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
