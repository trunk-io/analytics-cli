use std::format;
use std::path::PathBuf;

use anyhow::Context;

use crate::types::{
    BundleUploadLocation, CreateBundleUploadRequest, CreateRepoRequest,
    GetQuarantineBulkTestStatusRequest, QuarantineBulkTestStatus, Repo, Test,
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

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(anyhow::anyhow!(
            "Organization not found. Please double check the provided organization token and url slug: {}",
            org_slug
        )
        .context("Failed to validate trunk repo"));
    }

    Ok(())
}

pub async fn get_bundle_upload_location(
    origin: &str,
    api_token: &str,
    org_slug: &str,
    repo: &Repo,
) -> anyhow::Result<Option<BundleUploadLocation>> {
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

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(anyhow::anyhow!(
            "Organization not found. Please double check the provided organization token and url slug: {}",
            org_slug
        )
        .context("Failed to create bundle upload"));
    } else if resp.status() != reqwest::StatusCode::OK {
        log::warn!(
            "Failed to create bundle upload. {}: {}",
            resp.status(),
            status_code_help(resp.status())
        );
        return Ok(None);
    }

    resp.json::<Option<BundleUploadLocation>>()
        .await
        .context("Failed to get response body as json")
}

pub async fn get_quarantine_bulk_test_status(
    origin: &str,
    api_token: &str,
    org_slug: &str,
    repo: &Repo,
    test_identifiers: &[Test],
) -> anyhow::Result<QuarantineBulkTestStatus> {
    let client = reqwest::Client::new();
    let resp = match client
        .post(format!("{}/v1/metrics/getQuarantineBulkTestStatus", origin))
        .timeout(TRUNK_API_TIMEOUT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(TRUNK_API_TOKEN_HEADER, api_token)
        .json(&GetQuarantineBulkTestStatusRequest {
            org_url_slug: org_slug.to_owned(),
            repo: repo.clone(),
            test_identifiers: test_identifiers.to_vec(),
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

    resp.json::<QuarantineBulkTestStatus>()
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
