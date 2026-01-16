use api::client::get_api_host;
use display::end_output::EndOutput;
use http::StatusCode;
use superconsole::{
    Line, Span,
    style::{Attribute, Stylize, style},
};

const HELP_TEXT: &str = "For more help, contact us at https://slack.trunk.io/";
const CONNECTION_REFUSED_CONTEXT: &str = concat!("Unable to connect to trunk's server",);

pub(crate) const UNAUTHORIZED_CONTEXT: &str =
    concat!("Unathorized access, your Trunk organization URL slug or token may be incorrect",);

const GIX_ERROR_CONTEXT: &str = "Unable to open git repository";

fn add_settings_url_to_context(domain: String, org_url_slug: &String) -> String {
    let settings_url = format!("{}/{}/settings", domain.replace("api", "app"), org_url_slug);
    format!(
        "Hint: You can find it under the settings page at {}",
        settings_url
    )
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
}

impl ErrorReport {
    pub fn new(error: anyhow::Error, org_url_slug: String, base_message: Option<String>) -> Self {
        Self {
            context: Context {
                base_message,
                org_url_slug,
                exit_code: ErrorReport::find_exit_code(&error),
            },
            error,
        }
    }

    fn find_exit_code(error: &anyhow::Error) -> Option<i32> {
        if is_connection_refused(error) {
            tracing::warn!(CONNECTION_REFUSED_CONTEXT);
            return None;
        }

        if is_unauthorized(error) {
            tracing::warn!(UNAUTHORIZED_CONTEXT);
            return Some(exitcode::SOFTWARE);
        }

        if is_gix_error(error) {
            tracing::warn!(GIX_ERROR_CONTEXT);
            return Some(exitcode::SOFTWARE);
        }

        if let Some(message) = get_interrupting_message(error) {
            tracing::warn!("{}", message);
            return Some(exitcode::SOFTWARE);
        }

        tracing::error!("{}", error);
        tracing::error!(hidden_in_console = true, "Caused by error: {:#?}", error);
        None
    }

    pub fn should_block_quarantining(&self) -> bool {
        is_unauthorized(&self.error)
            || is_gix_error(&self.error)
            || get_interrupting_message(&self.error).is_some()
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

fn get_interrupting_message(error: &anyhow::Error) -> Option<String> {
    println!("CHECKING {:?}, rc {:?}", error, error.root_cause());
    error
        .root_cause()
        .downcast_ref::<InterruptingError>()
        .map(|e| e.message.clone())
}

impl EndOutput for ErrorReport {
    fn output(&self) -> anyhow::Result<Vec<Line>> {
        let Context {
            base_message,
            org_url_slug,
            exit_code,
        } = &self.context;
        let mut lines = Vec::new();
        lines.push(Line::from_iter([Span::new_styled(
            String::from("Error Encountered").attribute(Attribute::Bold),
        )?]));
        if is_connection_refused(&self.error) {
            if let Some(base_message) = base_message {
                lines.push(Line::from_iter([Span::new_unstyled_lossy(base_message)]));
                lines.push(Line::default());
            }
            lines.push(Line::from_iter([Span::new_unstyled(
                CONNECTION_REFUSED_CONTEXT,
            )?]));
            return Ok(lines);
        }
        let api_host = get_api_host();
        if is_unauthorized(&self.error) {
            {
                lines.extend(vec![
                    Line::from_iter([Span::new_unstyled_lossy(
                        base_message.as_deref().unwrap_or_default(),
                    )]),
                    Line::default(),
                    Line::from_iter([Span::new_unstyled(UNAUTHORIZED_CONTEXT)?]),
                    Line::from_iter([Span::new_unstyled_lossy(add_settings_url_to_context(
                        api_host,
                        org_url_slug,
                    ))]),
                ]);
                if let Some(line) = lines.last_mut() {
                    line.pad_left(2);
                }
                return Ok(lines);
            }
        }
        if base_message.is_some() {
            lines.push(Line::from_iter([Span::new_unstyled_lossy(
                base_message.as_deref().unwrap_or("An error occurred"),
            )]));
            lines.push(Line::default());
        } else {
            lines.push(Line::from_iter([Span::new_unstyled_lossy(
                self.error.to_string(),
            )]));
        }
        lines.push(Line::from_iter([Span::new_unstyled(HELP_TEXT)?]));
        lines.push(Line::default());
        match exit_code {
            Some(exitcode::OK) => lines.push(Line::from_iter([
                Span::new_unstyled("No errors occurred, returning default exit code: ")?,
                Span::new_styled(style(exitcode::OK.to_string()).attribute(Attribute::Bold))?,
            ])),
            Some(exitcode::SOFTWARE) => {
                // SOFTWARE is used to indicate that the upload command failed due to user error
                lines.push(Line::from_iter([
                    Span::new_unstyled("Errors occurred during execution, returning exit code: ")?,
                    Span::new_styled(
                        style(exitcode::SOFTWARE.to_string()).attribute(Attribute::Bold),
                    )?,
                ]));
            }
            Some(other_code) => {
                // Should be an unused codepath, but we log it for completeness
                lines.push(Line::from_iter([
                    Span::new_unstyled("Errors occurred during execution, returning exit code: ")?,
                    Span::new_styled(style(other_code.to_string()).attribute(Attribute::Bold))?,
                ]));
            }
            None => {
                // If uploads fail because trunk is down, we fall back to whatever came out of quarantining
                // to minimize customer impact
                lines.push(Line::from_iter([Span::new_unstyled(
                    "Errors occurred during execution, using quarantining exit code",
                )?]));
            }
        }
        Ok(lines)
    }
}

#[test]
fn adds_settings_if_domain_present() {
    let host = "https://api.fake-trunk.io";
    let final_context = add_settings_url_to_context(host.into(), &String::from("fake-org-slug"));
    assert_eq!(
        final_context,
        "Hint: You can find it under the settings page at https://app.fake-trunk.io/fake-org-slug/settings"
    )
}
