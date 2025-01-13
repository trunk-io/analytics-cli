use clap::Args;

use crate::upload_command::{run_upload, UploadArgs, UploadRunResult};

#[derive(Args, Clone, Debug)]
pub struct QuarantineArgs {
    #[command(flatten)]
    upload_args: UploadArgs,
}

pub async fn run_quarantine(QuarantineArgs { upload_args }: QuarantineArgs) -> anyhow::Result<i32> {
    let upload_run_result = run_upload(upload_args, None, None).await;
    upload_run_result.map(
        |UploadRunResult {
             exit_code,
             upload_bundle_error,
         }| {
            if let Some(e) = upload_bundle_error {
                log::error!("Error uploading test results: {:?}", e);
            }
            exit_code
        },
    )
}
