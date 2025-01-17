use std::{
    cmp, env,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::anyhow;
use api::{client::ApiClient, message::BundleUploadStatus};
use bundle::{
    BundleMeta, BundleMetaBaseProps, BundleMetaDebugProps, BundleMetaJunitProps, BundledFile,
    BundlerUtil, FileSet, META_VERSION,
};
use clap::Parser;
use constants::ENVS_TO_GET;
use context::repo::{BundleRepo, RepoUrlParts};
use trunk_analytics_cli::context::gather_upload_id_context;

#[derive(Debug, Parser)]
pub struct Cli {
    /// API token
    #[arg(long)]
    pub token: String,
    /// Organization URL slug
    #[arg(long)]
    pub org_url_slug: String,
    /// Repository host, e.g. `github.com`
    #[arg(long)]
    pub repo_host: String,
    /// Repository owner, e.g. `trunk-io`
    #[arg(long)]
    pub repo_owner: String,
    /// Repository name, e.g. `analytics-cli`
    #[arg(long)]
    pub repo_name: String,
    /// Software version
    #[arg(long)]
    pub repo_head_sha: String,
    /// Repository branch, e.g. `main`
    #[arg(long)]
    pub repo_head_branch: String,
    /// Repository commit epoch, e.g. `1737099516`
    #[arg(long)]
    pub repo_head_commit_epoch: String,
    /// Number of tests in JUnit file to upload
    #[arg(long)]
    pub junit_number_of_tests: Option<usize>,
    /// File path to JUnit file to upload, must end in `.xml` or `.junit`
    pub junit_file_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Cli {
        token,
        org_url_slug,
        repo_host,
        repo_owner,
        repo_name,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
        junit_file_path,
        junit_number_of_tests,
    } = Cli::parse();

    let api_client = ApiClient::new(&token)?;

    let repo_url_parts = RepoUrlParts {
        host: repo_host,
        owner: repo_owner,
        name: repo_name,
    };

    let repo_url = format!("https://{}", repo_url_parts.repo_full_name());

    let repo_head_sha_short = Some(String::from(
        &repo_head_sha[..cmp::min(7, repo_head_sha.len())],
    ));

    let bundled_file = if let Some(bundled_file) =
        BundledFile::from_path(&junit_file_path, 0, "", (), None, &None, None)?
    {
        bundled_file
    } else {
        return Err(anyhow!("File not allowed"));
    };

    let mut meta = BundleMeta {
        junit_props: BundleMetaJunitProps {
            num_files: 1,
            num_tests: junit_number_of_tests.unwrap_or(1),
        },
        debug_props: BundleMetaDebugProps {
            command_line: String::with_capacity(0),
        },
        bundle_upload_id_v2: String::with_capacity(0),
        base_props: BundleMetaBaseProps {
            version: META_VERSION.to_string(),
            org: org_url_slug,
            repo: BundleRepo {
                repo: repo_url_parts,
                repo_root: String::with_capacity(0),
                repo_url,
                repo_head_sha,
                repo_head_sha_short,
                repo_head_branch,
                repo_head_commit_epoch: repo_head_commit_epoch.parse()?,
                repo_head_commit_message: String::with_capacity(0),
                repo_head_author_name: String::with_capacity(0),
                repo_head_author_email: String::with_capacity(0),
            },
            cli_version: String::from(
                "cargo=0.6.999 git=3b2506691f89afab3acddd7b62afac8b601daf23 rustc=1.80.0-nightly",
            ),
            bundle_upload_id: String::with_capacity(0),
            tags: Vec::with_capacity(0),
            file_sets: vec![FileSet::new(
                vec![bundled_file],
                String::with_capacity(0),
                None,
            )],
            envs: ENVS_TO_GET
                .iter()
                .filter_map(|&env_var| {
                    env::var(env_var)
                        .map(|env_var_value| (env_var.to_string(), env_var_value))
                        .ok()
                })
                .collect(),
            upload_time_epoch: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            test_command: None,
            quarantined_tests: Vec::with_capacity(0),
            os_info: Some(env::consts::OS.to_string()),
            codeowners: None,
        },
    };

    let upload = gather_upload_id_context(&mut meta, &api_client).await?;

    let (
        bundle_temp_file,
        // directory is removed on drop
        _bundle_temp_dir,
    ) = BundlerUtil::new(meta, None).make_tarball_in_temp_dir()?;

    api_client
        .put_bundle_to_s3(&upload.url, &bundle_temp_file)
        .await?;

    api_client
        .update_bundle_upload(&api::message::UpdateBundleUploadRequest {
            id: upload.id.clone(),
            upload_status: BundleUploadStatus::UploadComplete,
        })
        .await?;

    Ok(())
}
