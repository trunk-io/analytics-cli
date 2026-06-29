use std::sync::mpsc::Sender;
use std::sync::{Arc, LazyLock};
use std::{env, time::Duration};

use constants::{TRUNK_API_CLIENT_RETRY_COUNT_ENV, TRUNK_API_CLIENT_RETRY_DEADLINE_SECS_ENV};
use display::message::{DisplayMessage, ProgressMessage, send_message};
use http::StatusCode;
use tokio::time::{self, Instant};
use tokio_retry::{Action, strategy::ExponentialBackoff};

use crate::client::{NOT_FOUND_CONTEXT, UNAUTHORIZED_CONTEXT};

// Tokio-retry uses base ^ retry * factor formula.
// This will give us 2s, 4s, 8s, 16s, 32s
const RETRY_BASE_MS: u64 = 2;
const RETRY_FACTOR: u64 = 1000;
const RETRY_COUNT_DEFAULT: usize = 5;

const CHECK_PROGRESS_INTERVAL_SECS: u64 = 2;
const REPORT_SLOW_PROGRESS_TIMEOUT_SECS: u64 = enforce_increment_check_progress_interval_secs(10);

const fn enforce_increment_check_progress_interval_secs(
    report_slow_progress_timeout_secs: u64,
) -> u64 {
    if report_slow_progress_timeout_secs % CHECK_PROGRESS_INTERVAL_SECS == 0 {
        return report_slow_progress_timeout_secs;
    }
    // NOTE: This is a build time error due to `const fn`
    panic!(
        "`report_slow_progress_timeout_secs` must be an increment of `CHECK_PROGRESS_INTERVAL_SECS`"
    )
}

/// Exponential backoff schedule for retrying API calls, sized by
/// `TRUNK_API_CLIENT_RETRY_COUNT` (default `RETRY_COUNT_DEFAULT`). The env var is read once;
/// `.clone()` this to obtain a fresh, un-advanced iterator for each retry loop.
static DEFAULT_DELAY: LazyLock<std::iter::Take<ExponentialBackoff>> = LazyLock::new(|| {
    ExponentialBackoff::from_millis(RETRY_BASE_MS)
        .factor(RETRY_FACTOR)
        .take(
            env::var(TRUNK_API_CLIENT_RETRY_COUNT_ENV)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(RETRY_COUNT_DEFAULT),
        )
});

/// Optional overall wall-clock budget for the retry loop, read once from
/// `TRUNK_API_CLIENT_RETRY_DEADLINE_SECS`. `None` means no deadline (retry until the count
/// is exhausted).
static RETRY_DEADLINE: LazyLock<Option<Duration>> = LazyLock::new(|| {
    env::var(TRUNK_API_CLIENT_RETRY_DEADLINE_SECS_ENV)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
});

/// Run `action`, retrying on retryable errors with exponential backoff. Mirrors the previous
/// `tokio-retry` behavior (initial attempt + up to `DEFAULT_DELAY` retries) but adds an
/// optional overall deadline: once the elapsed time plus the next backoff would exceed
/// `deadline`, the loop gives up and returns the most recent error instead of sleeping again.
async fn retry_with_deadline<A>(
    action: &mut A,
    deadline: Option<Duration>,
) -> Result<A::Item, A::Error>
where
    A: Action,
    A::Error: AbortableRetry,
{
    let retry_start = Instant::now();
    let mut delays = DEFAULT_DELAY.clone();
    loop {
        match action.run().await {
            Ok(item) => return Ok(item),
            Err(error) => {
                if !error.should_retry() {
                    return Err(error);
                }
                let Some(delay) = delays.next() else {
                    return Err(error);
                };
                if let Some(deadline) = deadline {
                    let elapsed = retry_start.elapsed();
                    if elapsed.saturating_add(delay) >= deadline {
                        tracing::debug!(
                            "Retry deadline of {:?} reached after {:?}; giving up.",
                            deadline,
                            elapsed
                        );
                        return Err(error);
                    }
                }
                time::sleep(delay).await;
            }
        }
    }
}

pub trait AbortableRetry {
    fn should_retry(&self) -> bool;
}

impl AbortableRetry for anyhow::Error {
    fn should_retry(&self) -> bool {
        let self_text = format!("{self}");
        !self_text.contains(UNAUTHORIZED_CONTEXT)
            && !self_text.contains(NOT_FOUND_CONTEXT)
            && self.chain().fold(true, |acc: bool, cause| {
                let cause_should_retry =
                    if let Some(reqwest_error) = cause.downcast_ref::<reqwest::Error>() {
                        reqwest_error.should_retry()
                    } else {
                        true
                    };
                acc && cause_should_retry
            })
    }
}

impl AbortableRetry for reqwest::Error {
    fn should_retry(&self) -> bool {
        !(self.is_decode()
            || self.status().map_or(false, |status: StatusCode| {
                // List of codes for which we do not retry
                if let Some(url) = self.url() {
                    tracing::debug!("Received status code {:?} for {:?}", status, url.as_str());
                } else {
                    tracing::debug!("Received status code {:?}", status);
                }
                match status {
                    // 400
                    StatusCode::BAD_REQUEST => true,
                    StatusCode::UNAUTHORIZED => true,
                    StatusCode::PAYMENT_REQUIRED => true,
                    StatusCode::FORBIDDEN => true,
                    StatusCode::NOT_FOUND => true,
                    StatusCode::METHOD_NOT_ALLOWED => true,
                    StatusCode::NOT_ACCEPTABLE => true,
                    StatusCode::PROXY_AUTHENTICATION_REQUIRED => true,
                    StatusCode::GONE => true,
                    StatusCode::LENGTH_REQUIRED => true,
                    StatusCode::PRECONDITION_FAILED => true,
                    StatusCode::PAYLOAD_TOO_LARGE => true,
                    StatusCode::URI_TOO_LONG => true,
                    StatusCode::UNSUPPORTED_MEDIA_TYPE => true,
                    StatusCode::RANGE_NOT_SATISFIABLE => true,
                    StatusCode::EXPECTATION_FAILED => true,
                    StatusCode::IM_A_TEAPOT => true,
                    StatusCode::MISDIRECTED_REQUEST => true,
                    StatusCode::UNPROCESSABLE_ENTITY => true,
                    StatusCode::FAILED_DEPENDENCY => true,
                    StatusCode::UPGRADE_REQUIRED => true,
                    StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE => true,
                    StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS => true,

                    // 500
                    StatusCode::NOT_IMPLEMENTED => true,
                    StatusCode::HTTP_VERSION_NOT_SUPPORTED => true,
                    StatusCode::VARIANT_ALSO_NEGOTIATES => true,
                    StatusCode::INSUFFICIENT_STORAGE => true,
                    StatusCode::LOOP_DETECTED => true,
                    StatusCode::NOT_EXTENDED => true,
                    StatusCode::NETWORK_AUTHENTICATION_REQUIRED => true,

                    _ => false,
                }
            }))
    }
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
    pub render_sender: Option<Sender<DisplayMessage>>,
}

impl<A, L, R> CallApi<A, L, R>
where
    A: Action,
    L: (FnOnce(Duration, usize) -> String) + Copy + Send + 'static,
    R: (FnOnce(Duration) -> String) + Copy + Send + 'static,
    A::Error: AbortableRetry,
{
    pub async fn call_api(&mut self) -> Result<A::Item, A::Error> {
        let report_slow_progress_start = time::Instant::now();
        let report_slow_progress_message = self.report_slow_progress_message;
        let mut slow_progress_sender = self.render_sender.clone();
        let report_slow_progress_handle = tokio::spawn(async move {
            let duration = Duration::from_secs(REPORT_SLOW_PROGRESS_TIMEOUT_SECS);
            time::sleep(duration).await;
            let time_elapsed = Instant::now().duration_since(report_slow_progress_start);
            let message = report_slow_progress_message(time_elapsed);
            slow_progress_sender.iter_mut().for_each(|s| {
                send_message(
                    DisplayMessage::Progress(
                        Arc::new(ProgressMessage {
                            message: message.clone(),
                        }),
                        String::from("slow progress message"),
                    ),
                    s,
                );
            });
            tracing::debug!("{:?}", message);
        });

        let check_progress_start = time::Instant::now();
        let log_progress_message = self.log_progress_message;
        let mut check_progress_sender = self.render_sender.clone();
        let check_progress_handle = tokio::spawn(async move {
            let mut log_count = 0;
            let duration = Duration::from_secs(CHECK_PROGRESS_INTERVAL_SECS);
            let mut interval = time::interval_at(Instant::now() + duration, duration);

            loop {
                let instant = interval.tick().await;
                let time_elapsed = instant.duration_since(check_progress_start);
                let log_message = log_progress_message(time_elapsed, log_count);
                check_progress_sender.iter_mut().for_each(|s| {
                    send_message(
                        DisplayMessage::Progress(
                            Arc::new(ProgressMessage {
                                message: log_message.clone(),
                            }),
                            String::from("progress message"),
                        ),
                        s,
                    );
                });
                tracing::debug!("{}", log_message);
                log_count += 1;
            }
        });

        let result = retry_with_deadline(&mut self.action, *RETRY_DEADLINE).await;
        report_slow_progress_handle.abort();
        check_progress_handle.abort();

        result
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use lazy_static::lazy_static;
    use reqwest::Response;
    use serde::{Deserialize, Serialize};
    use tokio::time;

    use super::{
        CHECK_PROGRESS_INTERVAL_SECS, CallApi, REPORT_SLOW_PROGRESS_TIMEOUT_SECS,
        RETRY_COUNT_DEFAULT, retry_with_deadline,
    };
    use crate::client::{CheckNotFound, CheckUnauthorized, status_code_help};
    #[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
    struct EmptyResponse {}

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
                Result::<(), anyhow::Error>::Ok(())
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
            render_sender: None,
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
                Result::<(), anyhow::Error>::Ok(())
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
            render_sender: None,
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
                Result::<(), anyhow::Error>::Err(anyhow::Error::msg("Broken"))
            },
            log_progress_message: |_, _| String::new(),
            report_slow_progress_message: |_| String::new(),
            render_sender: None,
        }
        .call_api()
        .await;

        assert_eq!(retry_count.into_inner(), RETRY_COUNT_DEFAULT + 1);
    }

    #[tokio::test(start_paused = true)]
    async fn stops_retrying_once_deadline_would_be_exceeded() {
        let retry_count = AtomicUsize::new(0);

        // Backoffs are 2s, 4s, 8s, ... With a 5s deadline: attempt 1 (elapsed 0s, next
        // backoff 2s -> 2s < 5s, sleep), attempt 2 (elapsed 2s, next backoff 4s -> 6s >= 5s,
        // give up). So we expect exactly 2 attempts despite the default retry count of 5.
        let mut action = || async {
            retry_count.fetch_add(1, Ordering::Relaxed);
            Result::<(), anyhow::Error>::Err(anyhow::Error::msg("Broken"))
        };

        let result = retry_with_deadline(&mut action, Some(Duration::from_secs(5))).await;

        assert!(result.is_err());
        assert_eq!(retry_count.into_inner(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn retries_full_count_when_no_deadline_set() {
        let retry_count = AtomicUsize::new(0);

        let mut action = || async {
            retry_count.fetch_add(1, Ordering::Relaxed);
            Result::<(), anyhow::Error>::Err(anyhow::Error::msg("Broken"))
        };

        let result = retry_with_deadline(&mut action, None).await;

        assert!(result.is_err());
        assert_eq!(retry_count.into_inner(), RETRY_COUNT_DEFAULT + 1);
    }

    #[tokio::test(start_paused = true)]
    async fn does_not_retry_on_json_decode_error() {
        let retry_count = AtomicUsize::new(0);

        let _ = CallApi {
            action: || async {
                time::sleep(Duration::from_secs(CHECK_PROGRESS_INTERVAL_SECS - 1)).await;
                retry_count.fetch_add(1, Ordering::Relaxed);
                Response::from(http::Response::new("{'invalid': 'json'"))
                    .json::<EmptyResponse>()
                    .await
            },
            log_progress_message: |_, _| String::new(),
            report_slow_progress_message: |_| String::new(),
            render_sender: None,
        }
        .call_api()
        .await;

        assert_eq!(retry_count.into_inner(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn does_not_retry_on_404_from_response() {
        let retry_count = AtomicUsize::new(0);

        let _ = CallApi {
            action: || async {
                time::sleep(Duration::from_secs(CHECK_PROGRESS_INTERVAL_SECS - 1)).await;
                retry_count.fetch_add(1, Ordering::Relaxed);
                let http_response = http::Response::builder().status(404).body("body").unwrap();
                let response = Response::from(http_response);
                status_code_help(
                    response,
                    CheckUnauthorized::DoNotCheck,
                    CheckNotFound::Check,
                    |_e| String::from("Test message"),
                    &String::from("mock_host"),
                    &String::from("mock_url_slug"),
                )
            },
            log_progress_message: |_, _| String::new(),
            report_slow_progress_message: |_| String::new(),
            render_sender: None,
        }
        .call_api()
        .await;

        assert_eq!(retry_count.into_inner(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn does_not_retry_on_404_from_error() {
        let retry_count = AtomicUsize::new(0);

        let _ = CallApi {
            action: || async {
                time::sleep(Duration::from_secs(CHECK_PROGRESS_INTERVAL_SECS - 1)).await;
                retry_count.fetch_add(1, Ordering::Relaxed);
                let http_response = http::Response::builder().status(404).body("body").unwrap();
                Response::from(http_response).error_for_status()
            },
            log_progress_message: |_, _| String::new(),
            report_slow_progress_message: |_| String::new(),
            render_sender: None,
        }
        .call_api()
        .await;

        assert_eq!(retry_count.into_inner(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn retries_on_500_from_error() {
        let retry_count = AtomicUsize::new(0);

        let _ = CallApi {
            action: || async {
                time::sleep(Duration::from_secs(CHECK_PROGRESS_INTERVAL_SECS - 1)).await;
                retry_count.fetch_add(1, Ordering::Relaxed);
                let http_response = http::Response::builder().status(500).body("body").unwrap();
                Response::from(http_response).error_for_status()
            },
            log_progress_message: |_, _| String::new(),
            report_slow_progress_message: |_| String::new(),
            render_sender: None,
        }
        .call_api()
        .await;

        assert_eq!(retry_count.into_inner(), RETRY_COUNT_DEFAULT + 1);
    }
}
