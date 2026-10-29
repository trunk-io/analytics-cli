mod error_report;
mod upload;
mod validate;

fn unauthorized_error() -> anyhow::Error {
    let response = http::Response::builder().status(401).body("").unwrap();
    anyhow::Error::from(
        reqwest::Response::from(response)
            .error_for_status()
            .unwrap_err(),
    )
}
