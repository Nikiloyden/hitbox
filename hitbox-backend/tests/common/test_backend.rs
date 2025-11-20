//! Simple in-memory test backend implementation using DashMap.

use async_trait::async_trait;
use dashmap::DashMap;
use hitbox_backend::serializer::{Format, JsonFormat};
use hitbox_backend::{
    Backend, BackendError, BackendResult, BackendValue, CacheBackend, CacheKeyFormat, Compressor,
    DeleteStatus, PassthroughCompressor,
};
use hitbox_core::{CacheKey, CacheValue, Raw};
use std::sync::Arc;
use std::time::Duration;

/// Simple in-memory backend for testing using DashMap.
///
/// This backend is thread-safe and can be cloned cheaply (Arc internally).
#[derive(Clone)]
pub struct TestBackend {
    store: Arc<DashMap<CacheKey, CacheValue<Raw>>>,
}

impl TestBackend {
    /// Create a new empty test backend.
    pub fn new() -> Self {
        Self {
            store: Arc::new(DashMap::new()),
        }
    }

    /// Get the number of entries in the backend.
    pub fn len(&self) -> usize {
        self.store.len()
    }

    /// Check if the backend is empty.
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    /// Clear all entries from the backend.
    pub fn clear(&self) {
        self.store.clear();
    }

    /// Check if a key exists in the backend.
    pub fn has(&self, key: &CacheKey) -> bool {
        self.store.contains_key(key)
    }

    /// Get a value directly (for test assertions).
    pub fn get_value(&self, key: &CacheKey) -> Option<CacheValue<Raw>> {
        self.store.get(key).map(|v| v.clone())
    }
}

impl Default for TestBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Backend for TestBackend {
    async fn read(&self, key: &CacheKey) -> BackendResult<BackendValue> {
        Ok(BackendValue::new(self.store.get(key).map(|v| v.clone())))
    }

    async fn write(
        &self,
        key: &CacheKey,
        value: CacheValue<Raw>,
        _ttl: Option<Duration>,
    ) -> BackendResult<()> {
        self.store.insert(key.clone(), value);
        Ok(())
    }

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        let existed = self.store.remove(key).is_some();
        Ok(if existed {
            DeleteStatus::Deleted(1)
        } else {
            DeleteStatus::Missing
        })
    }

    fn value_format(&self) -> &dyn Format {
        &JsonFormat
    }

    fn key_format(&self) -> &CacheKeyFormat {
        &CacheKeyFormat::Bitcode
    }

    fn compressor(&self) -> &dyn Compressor {
        &PassthroughCompressor
    }
}

impl CacheBackend for TestBackend {}

/// Backend that always returns errors (for error testing).
#[derive(Clone, Default)]
pub struct ErrorBackend;

#[async_trait]
impl Backend for ErrorBackend {
    async fn read(&self, _key: &CacheKey) -> BackendResult<BackendValue> {
        Err(BackendError::InternalError(Box::new(std::io::Error::other(
            "simulated error",
        ))))
    }

    async fn write(
        &self,
        _key: &CacheKey,
        _value: CacheValue<Raw>,
        _ttl: Option<Duration>,
    ) -> BackendResult<()> {
        Err(BackendError::InternalError(Box::new(std::io::Error::other(
            "simulated error",
        ))))
    }

    async fn remove(&self, _key: &CacheKey) -> BackendResult<DeleteStatus> {
        Err(BackendError::InternalError(Box::new(std::io::Error::other(
            "simulated error",
        ))))
    }

    fn value_format(&self) -> &dyn Format {
        &JsonFormat
    }

    fn key_format(&self) -> &CacheKeyFormat {
        &CacheKeyFormat::Bitcode
    }

    fn compressor(&self) -> &dyn Compressor {
        &PassthroughCompressor
    }
}

impl CacheBackend for ErrorBackend {}
