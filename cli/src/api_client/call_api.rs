use std::time::Duration;

use tokio::time::{self, Instant};
use tokio_retry::{strategy::ExponentialBackoff, Action, Retry};

// Tokio-retry uses base ^ retry * factor formula.
// This will give us 8ms, 64ms, 512ms, 4096ms, 32768ms
const RETRY_BASE_MS: u64 = 8;
const RETRY_FACTOR: u64 = 1;
const RETRY_COUNT: usize = 5;

const CHECK_PROGRESS_INTERVAL_SECS: u64 = 2;
const REPORT_SLOW_PROGRESS_TIMEOUT_SECS: u64 = enforce_increment_check_progress_interval_secs(10);

const fn enforce_increment_check_progress_interval_secs(
    report_slow_progress_timeout_secs: u64,
) -> u64 {
    if report_slow_progress_timeout_secs % CHECK_PROGRESS_INTERVAL_SECS == 0 {
        return report_slow_progress_timeout_secs;
    }
    // NOTE: This is a build time error due to `const fn`
    panic!("`report_slow_progress_timeout_secs` must be an increment of `CHECK_PROGRESS_INTERVAL_SECS`")
}

fn default_delay() -> std::iter::Take<ExponentialBackoff> {
    ExponentialBackoff::from_millis(RETRY_BASE_MS)
        .factor(RETRY_FACTOR)
        .take(RETRY_COUNT)
}

pub struct CallApi<A, L, R>
where
    A: Action,
    L: (FnOnce(Duration, usize) -> String) + Copy + Send + 'static,
    R: (FnOnce(Duration) -> String) + Copy + Send + 'static,
{
    pub action: A,
    pub log_progress_message: L,
    pub report_slow_progress_message: R,
}

impl<A, L, R> CallApi<A, L, R>
where
    A: Action,
    L: (FnOnce(Duration, usize) -> String) + Copy + Send + 'static,
    R: (FnOnce(Duration) -> String) + Copy + Send + 'static,
{
    pub async fn call_api(&mut self) -> Result<A::Item, A::Error> {
        let report_slow_progress_start = time::Instant::now();
        let report_slow_progress_message = self.report_slow_progress_message;
        let report_slow_progress_handle = tokio::spawn(async move {
            let duration = Duration::from_secs(REPORT_SLOW_PROGRESS_TIMEOUT_SECS);
            time::sleep(duration).await;
            let time_elapsed = Instant::now().duration_since(report_slow_progress_start);
            sentry::capture_message(
                report_slow_progress_message(time_elapsed).as_ref(),
                sentry::Level::Error,
            );
        });

        let check_progress_start = time::Instant::now();
        let log_progress_message = self.log_progress_message;
        let check_progress_handle = tokio::spawn(async move {
            let mut log_count = 0;
            let duration = Duration::from_secs(CHECK_PROGRESS_INTERVAL_SECS);
            let mut interval = time::interval_at(Instant::now() + duration, duration);

            loop {
                let instant = interval.tick().await;
                let time_elapsed = instant.duration_since(check_progress_start);
                let log_message = log_progress_message(time_elapsed, log_count);
                log::info!("{}", log_message);
                log_count += 1;
            }
        });

        let result = Retry::spawn(default_delay(), || (&mut self.action).run()).await;
        report_slow_progress_handle.abort();
        check_progress_handle.abort();

        result
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        time::Duration,
    };

    use lazy_static::lazy_static;
    use tokio::time;

    use super::{
        CallApi, CHECK_PROGRESS_INTERVAL_SECS, REPORT_SLOW_PROGRESS_TIMEOUT_SECS, RETRY_COUNT,
    };

    #[tokio::test(start_paused = true)]
    async fn logs_progress_and_reports_slow_progress() {
        lazy_static! {
            static ref LOG_PROGRESS_TIME_ELAPSED_AND_LOG_COUNT: Arc<Mutex<Vec<(Duration, usize)>>> =
                Arc::new(Mutex::new(vec![]));
            static ref REPORT_SLOW_PROGRESS_TIME_ELAPSED: Arc<Mutex<Vec<Duration>>> =
                Arc::new(Mutex::new(vec![]));
        }

        const DURATION: u64 = 20;

        CallApi {
            action: || async {
                time::sleep(Duration::from_secs(DURATION)).await;
                Result::<(), ()>::Ok(())
            },
            log_progress_message: |time_elapsed, log_count| {
                LOG_PROGRESS_TIME_ELAPSED_AND_LOG_COUNT
                    .lock()
                    .unwrap()
                    .push((time_elapsed, log_count));
                String::new()
            },
            report_slow_progress_message: |time_elapsed| {
                REPORT_SLOW_PROGRESS_TIME_ELAPSED
                    .lock()
                    .unwrap()
                    .push(time_elapsed);
                String::new()
            },
        }
        .call_api()
        .await
        .unwrap();

        assert_eq!(
            LOG_PROGRESS_TIME_ELAPSED_AND_LOG_COUNT
                .lock()
                .unwrap()
                .iter()
                .map(|(ts, count)| (ts.as_secs(), *count))
                .collect::<Vec<_>>(),
            (0..(DURATION / CHECK_PROGRESS_INTERVAL_SECS).saturating_sub(1))
                .map(|i| ((i + 1) * CHECK_PROGRESS_INTERVAL_SECS, i as usize))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            *REPORT_SLOW_PROGRESS_TIME_ELAPSED
                .lock()
                .unwrap()
                .iter()
                .map(|ts| ts.as_secs())
                .collect::<Vec<_>>(),
            vec![REPORT_SLOW_PROGRESS_TIMEOUT_SECS]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn does_not_log_after_action_completes() {
        lazy_static! {
            static ref LOG_PROGRESS_TIME_ELAPSED_AND_LOG_COUNT: Arc<Mutex<Vec<(Duration, usize)>>> =
                Arc::new(Mutex::new(vec![]));
            static ref REPORT_SLOW_PROGRESS_TIME_ELAPSED: Arc<Mutex<Vec<Duration>>> =
                Arc::new(Mutex::new(vec![]));
        }

        CallApi {
            action: || async {
                time::sleep(Duration::from_secs(CHECK_PROGRESS_INTERVAL_SECS - 1)).await;
                Result::<(), ()>::Ok(())
            },
            log_progress_message: |time_elapsed, log_count| {
                LOG_PROGRESS_TIME_ELAPSED_AND_LOG_COUNT
                    .lock()
                    .unwrap()
                    .push((time_elapsed, log_count));
                String::new()
            },
            report_slow_progress_message: |time_elapsed| {
                REPORT_SLOW_PROGRESS_TIME_ELAPSED
                    .lock()
                    .unwrap()
                    .push(time_elapsed);
                String::new()
            },
        }
        .call_api()
        .await
        .unwrap();

        assert_eq!(
            *LOG_PROGRESS_TIME_ELAPSED_AND_LOG_COUNT.lock().unwrap(),
            Vec::new()
        );
        assert_eq!(
            *REPORT_SLOW_PROGRESS_TIME_ELAPSED.lock().unwrap(),
            Vec::<Duration>::new()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn retries() {
        let retry_count = AtomicUsize::new(0);

        let _ = CallApi {
            action: || async {
                time::sleep(Duration::from_secs(CHECK_PROGRESS_INTERVAL_SECS - 1)).await;
                retry_count.fetch_add(1, Ordering::Relaxed);
                Result::<(), ()>::Err(())
            },
            log_progress_message: |_, _| String::new(),
            report_slow_progress_message: |_| String::new(),
        }
        .call_api()
        .await;

        assert_eq!(retry_count.into_inner(), RETRY_COUNT + 1);
    }
}
