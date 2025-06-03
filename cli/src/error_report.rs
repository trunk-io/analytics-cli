use api::client::get_api_host;
use http::StatusCode;
use superconsole::{
    style::{style, Attribute, Stylize},
    Component, Dimensions, DrawMode, Line, Lines, Span,
};

const HELP_TEXT: &str = "For more help, contact us at https://slack.trunk.io/";
const CONNECTION_REFUSED_CONTEXT: &str = concat!("Unable to connect to trunk's server",);

pub(crate) const UNAUTHORIZED_CONTEXT: &str =
    concat!("Unathorized access, your Trunk organization URL slug or token may be incorrect",);

fn add_settings_url_to_context(domain: String, org_url_slug: &String) -> String {
    let settings_url = format!("{}/{}/settings", domain.replace("api", "app"), org_url_slug);
    format!(
        "Hint: You can find it under the settings page at {}",
        settings_url
    )
}

pub struct Context {
    pub base_message: Option<String>,
    pub org_url_slug: String,
    pub exit_code: i32,
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

    fn find_exit_code(error: &anyhow::Error) -> i32 {
        if is_connection_refused(error) {
            tracing::warn!(CONNECTION_REFUSED_CONTEXT);
            return exitcode::OK;
        }

        if is_unauthorized(error) {
            tracing::warn!(UNAUTHORIZED_CONTEXT);
            return exitcode::SOFTWARE;
        }

        tracing::error!("{}", error);
        tracing::error!(hidden_in_console = true, "Caused by error: {:#?}", error);
        exitcode::SOFTWARE
    }
}

fn is_connection_refused(error: &anyhow::Error) -> bool {
    if let Some(io_error) = error.root_cause().downcast_ref::<std::io::Error>() {
        io_error.kind() == std::io::ErrorKind::ConnectionRefused
    } else {
        false
    }
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

impl Component for ErrorReport {
    fn draw_unchecked(&self, _dimensions: Dimensions, _mode: DrawMode) -> anyhow::Result<Lines> {
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
                lines.push(Line::from_iter([Span::new_unstyled(base_message)?]));
                lines.push(Line::default());
            }
            lines.push(Line::from_iter([Span::new_unstyled(
                CONNECTION_REFUSED_CONTEXT,
            )?]));
            return Ok(Lines(lines));
        }
        let api_host = get_api_host();
        if is_unauthorized(&self.error) {
            {
                lines.extend(vec![
                    Line::from_iter([Span::new_unstyled(
                        base_message.as_deref().unwrap_or_default(),
                    )?]),
                    Line::default(),
                    Line::from_iter([Span::new_unstyled(UNAUTHORIZED_CONTEXT)?]),
                    Line::from_iter([Span::new_unstyled(add_settings_url_to_context(
                        api_host,
                        org_url_slug,
                    ))?]),
                ]);
                if let Some(line) = lines.last_mut() {
                    line.pad_left(2);
                }
                return Ok(Lines(lines));
            }
        }
        if base_message.is_some() {
            lines.push(Line::from_iter([Span::new_unstyled(
                base_message.as_deref().unwrap_or("An error occurred"),
            )?]));
            lines.push(Line::default());
        } else {
            lines.push(Line::from_iter([Span::new_unstyled(
                self.error.to_string(),
            )?]));
        }
        lines.push(Line::from_iter([Span::new_unstyled(HELP_TEXT)?]));
        lines.push(Line::default());
        if exit_code == &exitcode::OK {
            lines.push(Line::from_iter([
                Span::new_unstyled("No errors occurred, returning default exit code: ")?,
                Span::new_styled(style(exit_code.to_string()).attribute(Attribute::Bold))?,
            ]));
        } else if exit_code == &exitcode::SOFTWARE {
            // SOFTWARE is used to indicate that the upload command failed
            lines.push(Line::from_iter([
                Span::new_unstyled("Errors occurred during execution, returning exit code: ")?,
                Span::new_styled(style(exit_code.to_string()).attribute(Attribute::Bold))?,
            ]));
        } else {
            // Should be an unused codepath, but we log it for completeness
            lines.push(Line::from_iter([
                Span::new_unstyled("Errors occurred during execution, returning exit code: ")?,
                Span::new_styled(style(exit_code.to_string()).attribute(Attribute::Bold))?,
            ]));
        }
        Ok(Lines(lines))
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
