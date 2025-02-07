pub const SENTRY_DNS: &str =
    "https://4814eaf1df0e8a1e3303bb7e2f89095a@o681886.ingest.us.sentry.io/4507772986982400";

pub fn init(options: Option<sentry::ClientOptions>) -> sentry::ClientInitGuard {
    let mut opts;
    if options.is_none() {
        opts = sentry::ClientOptions::default();
        opts.release = sentry::release_name!();
        #[cfg(feature = "force-sentry-env-dev")]
        {
            opts.environment = Some("development".into());
        }
    } else {
        opts = options.unwrap_or_default();
    }

    sentry::init((SENTRY_DNS, opts))
}

pub fn logger(mut builder: env_logger::Builder, log_level: log::LevelFilter) -> anyhow::Result<()> {
    let logger = sentry_log::SentryLogger::with_dest(builder.build());
    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(log_level);
    Ok(())
}
