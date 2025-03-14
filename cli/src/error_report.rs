use http::StatusCode;

pub fn log_error(error: &anyhow::Error, base_message: Option<&str>) -> i32 {
    let root_cause = error.root_cause();
    if let Some(io_error) = root_cause.downcast_ref::<std::io::Error>() {
        if io_error.kind() == std::io::ErrorKind::ConnectionRefused {
            tracing::warn!(
                "{}",
                message(base_message, "could not connect to trunk's server")
            );
            return exitcode::OK;
        }
    }

    if let Some(reqwest_error) = root_cause.downcast_ref::<reqwest::Error>() {
        if let Some(status) = reqwest_error.status() {
            if status == StatusCode::UNAUTHORIZED
                || status == StatusCode::FORBIDDEN
                || status == StatusCode::NOT_FOUND
            {
                tracing::warn!("{}", message(base_message, "unauthorized to access trunk"),);
                return exitcode::SOFTWARE;
            }
        }
    }

    tracing::error!("{}", message(base_message, "error"));
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

fn message(base_message: Option<&str>, hint: &str) -> String {
    match base_message {
        Some(base_message) => format!("{} because {}", base_message, hint),
        None => String::from(hint),
    }
}
