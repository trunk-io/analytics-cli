use lazy_static::lazy_static;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
struct MockLogger {
    pub logs: Arc<Mutex<Vec<(log::Level, String)>>>,
}

impl log::Log for MockLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn flush(&self) {}
    fn log(&self, record: &log::Record) {
        self.logs
            .lock()
            .unwrap()
            .push((record.level(), record.args().to_string()));
    }
}

pub fn mock_logger(max_level: Option<log::LevelFilter>) -> Arc<Mutex<Vec<(log::Level, String)>>> {
    lazy_static! {
        static ref MOCK_LOGGER: MockLogger = MockLogger::default();
    }

    log::set_logger(&MOCK_LOGGER as &'static MockLogger).unwrap();
    log::set_max_level(max_level.unwrap_or(log::LevelFilter::Debug));

    MOCK_LOGGER.logs.clone()
}

#[cfg(test)]
mod tests {
    use super::mock_logger;

    #[test]
    fn captures_logs() {
        let logs = mock_logger(None);
        const TEST_MESSAGE: &str = "test";
        log::error!("{}", TEST_MESSAGE);
        assert_eq!(
            *logs.lock().unwrap(),
            [(log::Level::Error, String::from(TEST_MESSAGE))]
        );
    }
}
