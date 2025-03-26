use clap::Args;

use crate::{
    error_report::{log_error, Context},
    upload_command::{run_upload, UploadArgs, UploadRunResult},
};

#[derive(Args, Clone, Debug)]
pub struct QuarantineArgs {
    #[command(flatten)]
    upload_args: UploadArgs,
}

impl QuarantineArgs {
    pub fn token(&self) -> String {
        self.upload_args.token.clone()
    }

    pub fn org_url_slug(&self) -> String {
        self.upload_args.org_url_slug.clone()
    }

    pub fn repo_root(&self) -> Option<String> {
        self.upload_args.repo_root.clone()
    }

    pub fn hide_banner(&self) -> bool {
        self.upload_args.hide_banner
    }
}

// This is an alias to `run_upload`, but does not exit on upload failure
pub async fn run_quarantine(QuarantineArgs { upload_args }: QuarantineArgs) -> anyhow::Result<i32> {
    let org_url_slug = upload_args.org_url_slug.clone();
    let upload_run_result = run_upload(upload_args, None, None).await;
    upload_run_result.map(
        |UploadRunResult {
             exit_code,
             upload_bundle_error,
         }| {
            if let Some(e) = upload_bundle_error {
                log_error(
                    &e,
                    Context {
                        base_message: Some("Error uploading test results".into()),
                        org_url_slug,
                    },
                );
            }
            exit_code
        },
    )
}
