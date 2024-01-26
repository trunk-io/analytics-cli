use std::path::PathBuf;

use anyhow::Context;

use crate::types::{BundleUploadLocation, Repo};

pub const TRUNK_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
pub const TRUNK_API_TOKEN_HEADER: &str = "x-api-token";

fn handle_upload_bundle_error_logs(status: Option<reqwest::StatusCode>) {
    log::error!("Failed to create bundle upload. Status: {:?}", status);
    if status == Some(reqwest::StatusCode::UNAUTHORIZED) {
        log::error!("Your Trunk token may be incorrect - find it on the Trunk app (Settings -> Manage Organization -> Organization API Token -> View).");
    } else if status == Some(reqwest::StatusCode::NOT_FOUND) {
        log::error!("Your Trunk organization URL slug may be incorrect - find it on the Trunk app (Settings -> Manage Organization -> Organization Slug).");
    }
}

pub async fn get_bundle_upload_location(
    api_address: &str,
    api_token: &str,
    org_slug: &str,
    repo: &Repo,
) -> anyhow::Result<BundleUploadLocation> {
    let req_body = serde_json::json!({
        "repo": {
            "host": repo.host.clone(),
            "owner": repo.owner.clone(),
            "name": repo.name.clone(),
        },
        "orgUrlSlug": org_slug.to_string(),
    });

    let client = reqwest::Client::new();
    let resp = match client
        .post(api_address)
        .timeout(TRUNK_API_TIMEOUT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(TRUNK_API_TOKEN_HEADER, api_token)
        .json(&req_body)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            handle_upload_bundle_error_logs(e.status());
            return Err(anyhow::anyhow!(e).context("Failed to create bundle upload"));
        }
    };

    if resp.status() != reqwest::StatusCode::OK {
        handle_upload_bundle_error_logs(Some(resp.status()));
        return Err(anyhow::anyhow!(resp.status()).context("Failed to create bundle upload"));
    }

    resp.json::<BundleUploadLocation>()
        .await
        .context("Failed to get repsonse body as json")
}

/// Puts file to S3 using pre-signed link.
///
pub async fn put_bundle_to_s3(url: &str, bundle_path: &PathBuf) -> anyhow::Result<()> {
    let file_size = bundle_path.metadata()?.len();
    let file = tokio::fs::File::open(bundle_path).await?;
    let client = reqwest::Client::new();
    let resp = match client
        .put(url)
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .header(reqwest::header::CONTENT_LENGTH, file_size)
        .body(reqwest::Body::from(file))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            log::error!("Failed to upload bundle to S3. Status: {:?}", e.status());
            return Err(anyhow::anyhow!(
                "Failed to upload bundle to S3. Error: {}",
                e
            ));
        }
    };

    if !resp.status().is_success() {
        log::error!("Failed to upload bundle to S3. Code: {:?}", resp.status());
        return Err(anyhow::anyhow!(
            "Failed to upload bundle to S3. Code={}: {}",
            resp.status(),
            resp.text().await?
        ));
    }

    Ok(())
}
