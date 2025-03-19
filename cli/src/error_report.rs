use api::client::get_api_host;
use http::StatusCode;

pub(crate) const UNAUTHORIZED_CONTEXT: &str = concat!(
    "Your Trunk organization URL slug or token may be incorrect - find it in the Trunk app",
);

fn add_settings_url_to_context(
    base_message: String,
    domain: Option<String>,
    org_url_slug: &String,
) -> String {
    match domain {
        Some(present_domain) => {
            let settings_url = format!(
                "{}/{}/settings",
                present_domain.replace("api", "app"),
                org_url_slug
            );
            format!(
                "{}\n  Hint: Your settings page can be found at: {}",
                base_message, settings_url
            )
        }
        None => base_message,
    }
}

const HELP_TEXT: &str = "\n\nFor more help, contact us at https://slack.trunk.io/";

pub struct Context {
    pub base_message: Option<String>,
    pub org_url_slug: String,
}

pub fn log_error(error: &anyhow::Error, context: Context) -> i32 {
    let root_cause = error.root_cause();
    let Context {
        base_message,
        org_url_slug,
    } = context;
    if let Some(io_error) = root_cause.downcast_ref::<std::io::Error>() {
        if io_error.kind() == std::io::ErrorKind::ConnectionRefused {
            tracing::warn!(
                "{}",
                message(base_message, "Unable to connect to trunk's server")
            );
            return exitcode::OK;
        }
    }

    let api_host = get_api_host();
    if let Some(reqwest_error) = root_cause.downcast_ref::<reqwest::Error>() {
        if let Some(status) = reqwest_error.status() {
            if status == StatusCode::UNAUTHORIZED
                || status == StatusCode::FORBIDDEN
                || status == StatusCode::NOT_FOUND
            {
                tracing::warn!(
                    "{}",
                    add_settings_url_to_context(
                        message(base_message, UNAUTHORIZED_CONTEXT),
                        Some(api_host),
                        &org_url_slug
                    )
                );
                return exitcode::SOFTWARE;
            }
        }
    }

    if let Some(base_message) = base_message {
        tracing::error!("{}", message(Some(base_message), HELP_TEXT));
    } else {
        tracing::error!("{}", error);
    }
    tracing::error!(hidden_in_console = true, "Caused by error: {:#?}", error);
    exitcode::SOFTWARE
}

pub fn error_reason(error: &anyhow::Error) -> String {
    let root_cause = error.root_cause();
    if let Some(io_error) = root_cause.downcast_ref::<std::io::Error>() {
        if io_error.kind() == std::io::ErrorKind::ConnectionRefused {
            return "connection".to_string();
        }
    }

    if let Some(reqwest_error) = root_cause.downcast_ref::<reqwest::Error>() {
        if let Some(status) = reqwest_error.status() {
            return status.to_string().replace(' ', "_").to_lowercase();
        }
    }
    "unknown".into()
}

fn message(base_message: Option<String>, hint: &str) -> String {
    match base_message {
        Some(base_message) => format!("{}\n\t{}", base_message, hint),
        None => String::from(hint),
    }
}

#[test]
fn adds_settings_if_domain_present() {
    let host = "https://api.fake-trunk.io";
    let final_context = add_settings_url_to_context(
        "base_context".into(),
        Some(host.into()),
        &String::from("fake-org-slug"),
    );
    assert_eq!(
        final_context,
        "base_context\n  Hint: Your settings page can be found at: https://app.fake-trunk.io/fake-org-slug/settings",
    )
}

#[test]
fn does_not_add_settings_if_domain_absent() {
    let final_context =
        add_settings_url_to_context("base_context".into(), None, &String::from("fake-org-slug"));
    assert_eq!(final_context, "base_context",)
}
