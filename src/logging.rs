use std::{collections::BTreeMap, time::Duration};
#[derive(Default)]
pub struct LogThrottle {
    last: BTreeMap<&'static str, Duration>,
}
impl LogThrottle {
    pub fn ready(&mut self, key: &'static str, now: Duration, interval: Duration) -> bool {
        if self
            .last
            .get(key)
            .is_some_and(|last| now.saturating_sub(*last) < interval)
        {
            return false;
        }
        self.last.insert(key, now);
        true
    }
    pub fn recover(&mut self, key: &'static str) -> bool {
        self.last.remove(key).is_some()
    }
}
pub fn log(level: &str, message: impl std::fmt::Display) {
    eprintln!("{level}: {message}");
}
