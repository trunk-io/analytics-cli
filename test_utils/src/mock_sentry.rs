use std::sync::{Arc, Mutex};

use sentry::{protocol::Event, ClientInitGuard, Hub, Integration, Level};

struct MockSentryIntegration {
    events: Arc<Mutex<Vec<(Level, String)>>>,
    hub_current: Arc<Hub>,
}

impl Integration for MockSentryIntegration {
    fn process_event(
        &self,
        event: Event<'static>,
        _: &sentry::ClientOptions,
    ) -> Option<Event<'static>> {
        let same_thread = Arc::ptr_eq(&self.hub_current, &sentry::Hub::current());
        if let (true, Ok(mut events)) = (same_thread, self.events.try_lock()) {
            events.push((event.level, event.message.unwrap_or_default()));
            None
        } else {
            Some(event)
        }
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
        hub_current: Hub::current(),
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
