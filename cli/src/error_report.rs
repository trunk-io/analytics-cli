use api::client::ClientErrorExplanation;
use http::StatusCode;
use schemars::JsonSchema;
use serde::Serialize;

pub(crate) const HELP_TEXT: &str = "For more help, contact us at https://slack.trunk.io/";
pub(crate) const CONNECTION_REFUSED_CONTEXT: &str = concat!("Unable to connect to trunk's server",);

pub(crate) const UNAUTHORIZED_CONTEXT: &str =
    concat!("Unathorized access, your Trunk organization URL slug or token may be incorrect",);

const GIX_ERROR_CONTEXT: &str = "Unable to open git repository";

pub(crate) fn settings_url(domain: &str, org_url_slug: &str) -> String {
    format!("{}/{}/settings", domain.replace("api", "app"), org_url_slug)
}

pub struct InterruptingError {
    message: String,
}
impl core::fmt::Display for InterruptingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl core::fmt::Debug for InterruptingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.message)
    }
}
impl std::error::Error for InterruptingError {}
impl InterruptingError {
    pub fn new<T: AsRef<str>>(message: T) -> Self {
        Self {
            message: message.as_ref().into(),
        }
    }
}

pub struct Context {
    pub base_message: Option<String>,
    pub org_url_slug: String,
    pub exit_code: Option<i32>,
}

pub struct ErrorReport {
    pub error: anyhow::Error,
    pub context: Context,
    pub kind: ErrorKind,
}

impl ErrorReport {
    pub fn new(error: anyhow::Error, org_url_slug: String, base_message: Option<String>) -> Self {
        let kind = ErrorKind::of(&error);
        match kind {
            ErrorKind::ConnectionRefused => tracing::warn!(CONNECTION_REFUSED_CONTEXT),
            ErrorKind::Unauthorized => tracing::warn!(UNAUTHORIZED_CONTEXT),
            ErrorKind::GitRepository => tracing::warn!(GIX_ERROR_CONTEXT),
            ErrorKind::Interrupted => {
                if let Some(message) = get_interrupting_message(&error) {
                    tracing::warn!("{}", message);
                }
            }
            ErrorKind::Other => {
                tracing::error!("{}", error);
                tracing::error!(hidden_in_console = true, "Caused by error: {:#?}", error);
            }
        }
        Self {
            context: Context {
                base_message,
                org_url_slug,
                exit_code: kind.exit_code(),
            },
            error,
            kind,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    /// Couldn't connect to Trunk.
    ConnectionRefused,
    /// Trunk rejected the org slug or token.
    Unauthorized,
    /// The repo root couldn't be opened as a git repository.
    GitRepository,
    /// The run stopped early, such as when it found no test reports and empty results aren't allowed.
    Interrupted,
    Other,
}

impl ErrorKind {
    pub fn of(error: &anyhow::Error) -> Self {
        if is_connection_refused(error) {
            Self::ConnectionRefused
        } else if is_unauthorized(error) {
            Self::Unauthorized
        } else if is_gix_error(error) {
            Self::GitRepository
        } else if get_interrupting_message(error).is_some() {
            Self::Interrupted
        } else {
            Self::Other
        }
    }

    pub fn exit_code(self) -> Option<i32> {
        match self {
            Self::ConnectionRefused | Self::Other => None,
            Self::Unauthorized | Self::GitRepository | Self::Interrupted => {
                Some(exitcode::SOFTWARE)
            }
        }
    }

    pub fn blocks_quarantining(self) -> bool {
        matches!(
            self,
            Self::Unauthorized | Self::GitRepository | Self::Interrupted
        )
    }
}

fn is_connection_refused(error: &anyhow::Error) -> bool {
    error
        .root_cause()
        .downcast_ref::<std::io::Error>()
        .is_some()
}

fn is_unauthorized(error: &anyhow::Error) -> bool {
    if let Some(reqwest_error) = error.root_cause().downcast_ref::<reqwest::Error>() {
        if let Some(status) = reqwest_error.status() {
            status == StatusCode::UNAUTHORIZED
                || status == StatusCode::FORBIDDEN
                || status == StatusCode::NOT_FOUND
        } else {
            false
        }
    } else {
        false
    }
}

fn is_gix_error(error: &anyhow::Error) -> bool {
    #[cfg(feature = "git-access")]
    {
        for cause in error.chain() {
            if cause.downcast_ref::<gix::open::Error>().is_some() {
                return true;
            }
        }
        false
    }
    #[cfg(not(feature = "git-access"))]
    {
        false
    }
}

pub(crate) fn get_client_error_explanation(error: &anyhow::Error) -> Option<String> {
    error
        .downcast_ref::<ClientErrorExplanation>()
        .map(ToString::to_string)
}

fn get_interrupting_message(error: &anyhow::Error) -> Option<String> {
    error
        .root_cause()
        .downcast_ref::<InterruptingError>()
        .map(|e| e.message.clone())
}

#[test]
fn builds_the_settings_url_from_the_api_host() {
    assert_eq!(
        settings_url("https://api.fake-trunk.io", "fake-org-slug"),
        "https://app.fake-trunk.io/fake-org-slug/settings"
    )
}
