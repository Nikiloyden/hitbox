//! Simple in-memory test backend implementation using DashMap.

use async_trait::async_trait;
use dashmap::DashMap;
use hitbox_backend::format::{Format, JsonFormat};
use hitbox_backend::{
    Backend, BackendError, BackendResult, CacheBackend, CacheKeyFormat, Compressor, DeleteStatus,
    PassthroughCompressor,
};
use hitbox_core::{CacheKey, CacheValue, Raw};
use std::sync::Arc;

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

    /// Clear all entries from the backend.
    pub fn clear(&self) {
        self.store.clear();
    }

    /// Check if a key exists in the backend.
    pub fn has(&self, key: &CacheKey) -> bool {
        self.store.contains_key(key)
    }

    /// Get raw cache value with metadata (expire, stale) for inspection.
    pub fn get_raw(&self, key: &CacheKey) -> Option<CacheValue<Raw>> {
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
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        Ok(self.store.get(key).map(|v| v.clone()))
    }

    async fn write(&self, key: &CacheKey, value: CacheValue<Raw>) -> BackendResult<()> {
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

    fn name(&self) -> &str {
        "test"
    }
}

impl CacheBackend for TestBackend {}

/// Backend that always returns errors (for error testing).
#[derive(Clone, Default)]
pub struct ErrorBackend;

#[async_trait]
impl Backend for ErrorBackend {
    async fn read(&self, _key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        Err(BackendError::InternalError(Box::new(
            std::io::Error::other("simulated error"),
        )))
    }

    async fn write(&self, _key: &CacheKey, _value: CacheValue<Raw>) -> BackendResult<()> {
        Err(BackendError::InternalError(Box::new(
            std::io::Error::other("simulated error"),
        )))
    }

    async fn remove(&self, _key: &CacheKey) -> BackendResult<DeleteStatus> {
        Err(BackendError::InternalError(Box::new(
            std::io::Error::other("simulated error"),
        )))
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

    fn name(&self) -> &str {
        "error"
    }
}

impl CacheBackend for ErrorBackend {}
