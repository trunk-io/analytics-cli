use std::path::PathBuf;

use crate::types::{BundleUploadLocation, Repo};

pub const TRUNK_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
pub const TRUNK_API_TOKEN_HEADER: &str = "x-api-token";

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
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("x-api-token", api_token)
        .json(&req_body)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            log::error!("Failed to upload bundle to S3. Status: {:?}", e.status());
            if e.status() == Some(reqwest::StatusCode::UNAUTHORIZED) {
                log::info!("Your Trunk token may be incorrect - find it on the Trunk app (Settings -> Manage Organization -> Organization API Token -> View).");
            }
            return Err(anyhow::anyhow!(
                "Failed to upload bundle to S3. Error: {}",
                e
            ));
        }
    };

    resp.json::<BundleUploadLocation>()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get repsonse body as json. Error: {}", e))
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
