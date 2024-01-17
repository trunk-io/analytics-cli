use std::path::PathBuf;

use crate::types::{BundleUploadLocation, Repo};

pub const TRUNK_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
pub const TRUNK_API_TOKEN_HEADER: &str = "x-api-token";

pub async fn get_bundle_upload_location(
    _api_address: &str,
    _api_token: &str,
    _org_slug: &str,
    _repo: &Repo,
) -> anyhow::Result<BundleUploadLocation> {
    todo!("TRUNK-10274")
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
