use std::format;
use std::path::PathBuf;

use anyhow::Context;

use crate::types::{
    BundleUploadStatus, CreateBundleUploadRequest, CreateBundleUploadResponse, CreateRepoRequest,
    GetQuarantineBulkTestStatusRequest, QuarantineConfig, Repo, UpdateBundleUploadRequest
};
use crate::utils::status_code_help;

pub const TRUNK_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
pub const TRUNK_API_TOKEN_HEADER: &str = "x-api-token";

pub async fn create_trunk_repo(
    origin: &str,
    api_token: &str,
    org_slug: &str,
    repo: &Repo,
    remote_urls: &[String],
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let resp = match client
        .post(format!("{}/v1/repo/create", origin))
        .timeout(TRUNK_API_TIMEOUT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(TRUNK_API_TOKEN_HEADER, api_token)
        .json(&CreateRepoRequest {
            org_url_slug: org_slug.to_owned(),
            repo: repo.clone(),
            remote_urls: remote_urls.to_vec(),
        })
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => return Err(anyhow::anyhow!(e).context("Failed to validate trunk repo")),
    };

    if resp.status().is_client_error() {
        return Err(anyhow::anyhow!(
            "Organization not found. Please double check the provided organization token and url slug: {}",
            org_slug
        )
        .context("Failed to validate trunk repo"));
    }

    Ok(())
}
pub async fn update_bundle_upload_status(
    origin: &str,
    api_token: &str,
    id: &str,
    upload_status: &BundleUploadStatus,
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let resp = client
        .patch(format!("{}/v1/metrics/updateBundleUpload", origin))
        .timeout(TRUNK_API_TIMEOUT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(TRUNK_API_TOKEN_HEADER, api_token)
        .json(&UpdateBundleUploadRequest {
            id: id.to_owned(),
            upload_status: upload_status.to_owned(),
        })
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e).context("Failed to update bundle upload status"))?;

    if resp.status().is_client_error() {
        return Err(anyhow::anyhow!(
            "Failed to update bundle upload status. Client error: {}",
            resp.status()
        ));
    }

    Ok(())
}

pub async fn create_bundle_upload_intent(
    origin: &str,
    api_token: &str,
    org_slug: &str,
    repo: &Repo,
) -> anyhow::Result<CreateBundleUploadResponse> {
    let client = reqwest::Client::new();
    let resp = match client
        .post(format!("{}/v1/metrics/createBundleUpload", origin))
        .timeout(TRUNK_API_TIMEOUT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(TRUNK_API_TOKEN_HEADER, api_token)
        .json(&CreateBundleUploadRequest {
            org_url_slug: org_slug.to_owned(),
            repo: repo.clone(),
        })
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => return Err(anyhow::anyhow!(e).context("Failed to create bundle upload")),
    };

    if resp.status() != reqwest::StatusCode::OK {
        return Err(
            anyhow::anyhow!("{}: {}", resp.status(), status_code_help(resp.status()))
                .context("Failed to create bundle upload"),
        );
    }

    resp.json::<CreateBundleUploadResponse>()
        .await
        .context("Failed to get response body as json")
}

pub async fn get_quarantining_config(
    origin: &str,
    api_token: &str,
    org_slug: &str,
    repo: &Repo,
) -> anyhow::Result<QuarantineConfig> {
    let client = reqwest::Client::new();
    let resp = match client
        .post(format!("{}/v1/metrics/getQuarantineConfig", origin))
        .timeout(TRUNK_API_TIMEOUT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(TRUNK_API_TOKEN_HEADER, api_token)
        .json(&GetQuarantineBulkTestStatusRequest {
            org_url_slug: org_slug.to_owned(),
            repo: repo.clone(),
        })
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => return Err(anyhow::anyhow!(e).context("Failed to get quarantine bulk test")),
    };

    if resp.status() != reqwest::StatusCode::OK {
        return Err(
            anyhow::anyhow!("{}: {}", resp.status(), status_code_help(resp.status()))
                .context("Failed to get quarantine bulk test"),
        );
    }

    resp.json::<QuarantineConfig>()
        .await
        .context("Failed to get response body as json")
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
