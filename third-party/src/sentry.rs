use std::borrow::Cow;

use sentry::IntoDsn;

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
    if std::env::var("DISABLE_SENTRY").is_ok() || std::env::var("DISABLE_TELEMETRY").is_ok() {
        opts.sample_rate = 0.0;
    }

    opts.dsn = match SENTRY_DSN.into_dsn() {
        Ok(dsn) => dsn,
        Err(_) => {
            // Logging is not set up at this point, but this is a pure code error
            // that can only be caused by bad formatting of the dsn const.
            None
        }
    };

    sentry::init(opts)
}
