use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use dashmap::DashMap;
use hitbox_backend::{Backend, BackendResult, DeleteStatus};
use hitbox_core::{BackendLabel, CacheKey, CacheValue, Raw};

#[derive(Debug, Default)]
pub struct BackendCounters {
    pub read_count: AtomicUsize,
    pub read_hit_count: AtomicUsize,
    pub read_miss_count: AtomicUsize,
    pub write_count: AtomicUsize,
    pub remove_count: AtomicUsize,
}

impl BackendCounters {
    pub fn read_count(&self) -> usize {
        self.read_count.load(Ordering::SeqCst)
    }

    pub fn read_hit_count(&self) -> usize {
        self.read_hit_count.load(Ordering::SeqCst)
    }

    pub fn read_miss_count(&self) -> usize {
        self.read_miss_count.load(Ordering::SeqCst)
    }

    pub fn write_count(&self) -> usize {
        self.write_count.load(Ordering::SeqCst)
    }

    pub fn remove_count(&self) -> usize {
        self.remove_count.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.read_count.store(0, Ordering::SeqCst);
        self.read_hit_count.store(0, Ordering::SeqCst);
        self.read_miss_count.store(0, Ordering::SeqCst);
        self.write_count.store(0, Ordering::SeqCst);
        self.remove_count.store(0, Ordering::SeqCst);
    }
}

#[derive(Clone, Debug)]
pub struct MockBackend {
    pub cache: Arc<DashMap<CacheKey, CacheValue<Raw>>>,
    pub counters: Arc<BackendCounters>,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBackend {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            counters: Arc::new(BackendCounters::default()),
        }
    }

    pub fn read_count(&self) -> usize {
        self.counters.read_count()
    }

    pub fn read_hit_count(&self) -> usize {
        self.counters.read_hit_count()
    }

    pub fn read_miss_count(&self) -> usize {
        self.counters.read_miss_count()
    }

    pub fn write_count(&self) -> usize {
        self.counters.write_count()
    }

    pub fn remove_count(&self) -> usize {
        self.counters.remove_count()
    }

    pub fn reset_counters(&self) {
        self.counters.reset();
    }

    pub fn cache_entry_count(&self) -> usize {
        self.cache.len()
    }
}

#[async_trait]
impl Backend for MockBackend {
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        self.counters.read_count.fetch_add(1, Ordering::SeqCst);
        let result = self.cache.get(key).map(|v| v.value().clone());
        if result.is_some() {
            self.counters.read_hit_count.fetch_add(1, Ordering::SeqCst);
        } else {
            self.counters.read_miss_count.fetch_add(1, Ordering::SeqCst);
        }
        Ok(result)
    }

    async fn write(&self, key: &CacheKey, value: CacheValue<Raw>) -> BackendResult<()> {
        self.counters.write_count.fetch_add(1, Ordering::SeqCst);
        self.cache.insert(key.clone(), value);
        Ok(())
    }

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        self.counters.remove_count.fetch_add(1, Ordering::SeqCst);
        match self.cache.remove(key) {
            Some(_) => Ok(DeleteStatus::Deleted(1)),
            None => Ok(DeleteStatus::Missing),
        }
    }

    fn name(&self) -> BackendLabel {
        BackendLabel::new_static("mock")
    }
}

impl hitbox_backend::CacheBackend for MockBackend {}
