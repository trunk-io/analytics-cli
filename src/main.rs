use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use tokio_retry::strategy::ExponentialBackoff;
use tokio_retry::Retry;
use trunk_analytics_cli::bundler::BundlerUtil;
use trunk_analytics_cli::scanner::{BundleRepo, EnvScanner, FileSet, FileSetCounter};
use trunk_analytics_cli::types::BundleMeta;
use trunk_analytics_cli::utils::{from_non_empty_or_default, parse_custom_tags};

#[derive(Debug, Parser)]
#[command(
    version = std::env!("CARGO_PKG_VERSION"),
    name = "trunk-analytics-cli",
    about = "Trunk Analytics CLI"
)]
struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[clap(name = "upload")]
    Upload {
        #[arg(
            long,
            required = true,
            value_delimiter = ',',
            help = "Comma-separated list of glob paths to junit files."
        )]
        junit_paths: Vec<String>,
        #[arg(long, help = "Organization url slug.")]
        org_url_slug: String,
        #[arg(
            long,
            required = true,
            env = "TRUNK_API_TOKEN",
            help = "Organization token. Defaults to TRUNK_API_TOKEN env var."
        )]
        token: String,
        #[arg(long, help = "Path to repository root. Defaults to current directory.")]
        repo_root: Option<String>,
        #[arg(long, help = "Value to override URL of repository.")]
        repo_url: Option<String>,
        #[arg(long, help = "Value to override SHA of repository head.")]
        repo_head_sha: Option<String>,
        #[arg(long, help = "Value to override branch of repository head.")]
        repo_head_branch: Option<String>,
        #[arg(long, help = "Value to override commit epoch of repository head.")]
        repo_head_commit_epoch: Option<String>,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Comma separated list of custom tag=value pairs."
        )]
        tags: Vec<String>,
        #[arg(long, help = "Print files which will be uploaded to stdout.")]
        print_files: bool,
        #[arg(long, help = "Run metrics CLI without uploading to API.")]
        dry_run: bool,
    },
}

const DEFAULT_API_ADDRESS: &str = "https://api.trunk.io:5022";
// Tokio-retry uses base ^ retry * factor formula.
// This will give us 8ms, 64ms, 512ms, 4096ms, 32768ms
const RETRY_BASE_MS: u64 = 8;
const RETRY_FACTOR: u64 = 1;
const RETRY_COUNT: usize = 5;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    setup_logger()?;
    let cli = Cli::parse();
    if let Err(e) = run(cli).await {
        log::error!("Error: {:?}", e);
        std::process::exit(exitcode::SOFTWARE);
    }
    Ok(())
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let Commands::Upload {
        junit_paths,
        org_url_slug,
        token,
        repo_root,
        repo_url,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
        tags,
        print_files,
        dry_run,
    } = cli.command;

    log::info!(
        "Starting trunk-analytics-cli {} (git={}) rustc={}",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_RUSTC_SEMVER")
    );

    let api_address = from_non_empty_or_default(
        std::env::var("TRUNK_API_ADDRESS").ok(),
        DEFAULT_API_ADDRESS.to_string(),
        |s| s,
    );

    let tags = parse_custom_tags(&tags)?;

    let repo = BundleRepo::try_read_from_root(
        repo_root,
        repo_url,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
    )?;

    if junit_paths.len() == 0 {
        log::error!("No junit paths provided.");
        return Ok(());
    }
    let mut file_counter = FileSetCounter::default();
    let file_sets = junit_paths
        .iter()
        .map(|path| FileSet::scan_from_glob(&repo.repo_root, path.to_string(), &mut file_counter))
        .collect::<anyhow::Result<Vec<FileSet>>>()?;

    let envs = EnvScanner::scan_env();
    let meta = BundleMeta {
        org: org_url_slug.clone(),
        repo: repo.clone(),
        tags,
        file_sets,
        envs,
        upload_time_epoch: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    };
    log::info!("Total files pack and upload: {}", file_counter.get_count());
    if file_counter.get_count() == 0 {
        log::warn!(
            "No JUnit files found to pack and upload using globs: {:?}",
            junit_paths
        );
    }

    if print_files {
        println!("Files to upload:");
        for file_set in &meta.file_sets {
            println!(
                "  File set ({:?}): {}",
                file_set.file_set_type, file_set.glob
            );
            for file in &file_set.files {
                println!("    {}", file.original_path);
            }
        }
    }

    let bundle_temp_dir = tempfile::tempdir()?;
    let bundle_time_file = bundle_temp_dir.path().join("bundle.tar.zstd");
    let bundler = BundlerUtil::new(meta);
    bundler.make_tarball(&bundle_time_file)?;
    log::info!("Flushed temporary tarball to {:?}", bundle_time_file);

    let upload = Retry::spawn(default_delay(), || {
        trunk_analytics_cli::clients::get_bundle_upload_location(
            &api_address,
            &token,
            &org_url_slug,
            &repo.repo,
        )
    })
    .await?;

    if dry_run {
        log::info!("Dry run, skipping upload.");
        return Ok(());
    }

    Retry::spawn(default_delay(), || {
        trunk_analytics_cli::clients::put_bundle_to_s3(&upload.url, &bundle_time_file)
    })
    .await?;

    log::info!("Done");
    Ok(())
}

fn default_delay() -> std::iter::Take<ExponentialBackoff> {
    ExponentialBackoff::from_millis(RETRY_BASE_MS)
        .factor(RETRY_FACTOR)
        .take(RETRY_COUNT)
}

fn setup_logger() -> anyhow::Result<()> {
    let mut builder = env_logger::Builder::new();
    builder
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] - {}",
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .filter(None, log::LevelFilter::Info);
    if let Ok(log) = std::env::var("TRUNK_LOG") {
        builder.parse_filters(&log);
    }
    builder.init();
    Ok(())
}
