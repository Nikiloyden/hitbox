use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use dashmap::DashMap;
use strum::EnumString;

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, EnumString)]
pub enum HandlerName {
    GetBook,
    GetBooks,
    PostBook,
    GetBookCover,
    EchoBody,
}

#[derive(Debug, Default)]
pub struct HandlerConfig {
    pub call_count: AtomicUsize,
    pub delay_ms: AtomicUsize,
}

impl HandlerConfig {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct HandlerState {
    configs: Arc<DashMap<HandlerName, HandlerConfig>>,
}

impl HandlerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn increment_call_count(&self, handler: HandlerName) {
        self.configs
            .entry(handler)
            .or_default()
            .call_count
            .fetch_add(1, Ordering::SeqCst);
    }

    pub fn get_call_count(&self, handler: HandlerName) -> usize {
        self.configs
            .get(&handler)
            .map(|c| c.call_count.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    pub fn set_delay(&self, handler: HandlerName, delay_ms: u64) {
        self.configs
            .entry(handler)
            .or_default()
            .delay_ms
            .store(delay_ms as usize, Ordering::SeqCst);
    }

    pub fn get_delay(&self, handler: HandlerName) -> Duration {
        let ms = self
            .configs
            .get(&handler)
            .map(|c| c.delay_ms.load(Ordering::SeqCst))
            .unwrap_or(0);
        Duration::from_millis(ms as u64)
    }

    pub async fn apply_delay(&self, handler: HandlerName) {
        tokio::time::sleep(self.get_delay(handler)).await;
    }

    pub fn reset(&self) {
        self.configs.clear();
    }
}
