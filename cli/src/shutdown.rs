//! Graceful shutdown on termination signals.
//!
//! GitHub Actions (and most CI systems) cancel a job by signaling the process tree. We race
//! these signals against the actual work in `main` so a cancellation aborts in-flight
//! operations (e.g. the API retry loop) immediately instead of blocking until they finish.

use trunk_analytics_cli::exit_code::ExitCode;

/// Resolves once the process receives a termination signal — SIGINT/SIGTERM on Unix, Ctrl-C
/// on Windows. Returns the conventional `128 + signal` exit code.
pub async fn shutdown_signal() -> ExitCode {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        // If we can't install the handlers, fall back to default disposition by never
        // resolving here, letting the OS terminate the process as it normally would.
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(stream) => stream,
            Err(error) => {
                tracing::warn!("Failed to install SIGTERM handler: {error}");
                std::future::pending::<()>().await;
                unreachable!()
            }
        };
        let mut sigint = match signal(SignalKind::interrupt()) {
            Ok(stream) => stream,
            Err(error) => {
                tracing::warn!("Failed to install SIGINT handler: {error}");
                std::future::pending::<()>().await;
                unreachable!()
            }
        };
        tokio::select! {
            _ = sigterm.recv() => ExitCode::Terminated,
            _ = sigint.recv() => ExitCode::Interrupted,
        }
    }
    #[cfg(not(unix))]
    {
        // Ctrl-C on Windows is the SIGINT-equivalent interruption.
        let _ = tokio::signal::ctrl_c().await;
        ExitCode::Interrupted
    }
}
