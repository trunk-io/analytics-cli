//! Graceful shutdown on termination signals.
//!
//! GitHub Actions (and most CI systems) cancel a job by signaling the process tree. We race
//! these signals against the actual work in `main` so a cancellation aborts in-flight
//! operations (e.g. the API retry loop) immediately instead of blocking until they finish.

// Conventional process exit codes for termination by signal: `128 + signal number`. This
// POSIX/shell convention isn't part of sysexits.h, so the `exitcode` crate used elsewhere
// doesn't define it. SIGINT is 2 and SIGTERM is 15 on the Unix platforms we target.
const EXIT_SIGNAL_BASE: i32 = 128;
const EXIT_CODE_SIGINT: i32 = EXIT_SIGNAL_BASE + 2;
const EXIT_CODE_SIGTERM: i32 = EXIT_SIGNAL_BASE + 15;

/// Resolves once the process receives a termination signal — SIGINT/SIGTERM on Unix, Ctrl-C
/// on Windows. Returns the conventional `128 + signal` exit code.
pub async fn shutdown_signal() -> i32 {
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
            _ = sigterm.recv() => EXIT_CODE_SIGTERM,
            _ = sigint.recv() => EXIT_CODE_SIGINT,
        }
    }
    #[cfg(not(unix))]
    {
        // Ctrl-C on Windows is the SIGINT-equivalent interruption.
        let _ = tokio::signal::ctrl_c().await;
        EXIT_CODE_SIGINT
    }
}
