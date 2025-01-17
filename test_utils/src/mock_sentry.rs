use sentry::{protocol::Event, ClientInitGuard, Integration, Level};
use std::sync::{Arc, Mutex};

struct MockSentryIntegration {
    events: Arc<Mutex<Vec<(Level, String)>>>,
}

impl Integration for MockSentryIntegration {
    fn process_event(
        &self,
        event: Event<'static>,
        _: &sentry::ClientOptions,
    ) -> Option<Event<'static>> {
        self.events
            .lock()
            .unwrap()
            .push((event.level, event.message.unwrap_or_default()));
        None
    }
}

pub fn mock_sentry() -> (Arc<Mutex<Vec<(Level, String)>>>, ClientInitGuard) {
    let events: Arc<Mutex<Vec<(Level, String)>>> = Default::default();

    let options = sentry::ClientOptions {
        environment: Some("development".into()),
        ..Default::default()
    }
    .add_integration(MockSentryIntegration {
        events: events.clone(),
    });

    let guard = sentry::init(("https://public@sentry.example.com/1", options));

    (events, guard)
}

#[cfg(test)]
mod tests {
    use super::mock_sentry;

    #[test]
    fn captures_events() {
        let (events, guard) = mock_sentry();
        const TEST_MESSAGE: &str = "test";
        sentry::capture_message(TEST_MESSAGE, sentry::Level::Error);
        guard.flush(None);
        assert_eq!(
            *events.lock().unwrap(),
            [(sentry::Level::Error, String::from(TEST_MESSAGE))]
        );
    }
}
