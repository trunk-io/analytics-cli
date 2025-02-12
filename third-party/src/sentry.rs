use std::borrow::Cow;

pub const SENTRY_DSN: &str =
    "https://4814eaf1df0e8a1e3303bb7e2f89095a@o681886.ingest.us.sentry.io/4507772986982400";

pub fn init(
    release_name: Cow<'static, str>,
    options: Option<sentry::ClientOptions>,
) -> sentry::ClientInitGuard {
    let mut opts;
    if options.is_none() {
        opts = sentry::ClientOptions::default();
        #[cfg(feature = "force-sentry-env-dev")]
        {
            opts.environment = Some("development".into());
        }
    } else {
        opts = options.unwrap_or_default();
    }
    opts.release = Some(release_name);
    if std::env::var("DISABLE_SENTRY").is_ok() {
        opts.sample_rate = 0.0;
    }

    sentry::init((SENTRY_DSN, opts))
}

pub fn logger(mut builder: env_logger::Builder, log_level: log::LevelFilter) -> anyhow::Result<()> {
    let logger = sentry_log::SentryLogger::with_dest(builder.build());
    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(log_level);
    Ok(())
}
