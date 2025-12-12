use async_trait::async_trait;
use chrono::Utc;
use hitbox::{BackendLabel, CacheKey, CacheValue, Raw};
use hitbox_backend::Backend;
use hitbox_backend::format::{Format, JsonFormat};
use hitbox_backend::{
    BackendResult, CacheKeyFormat, Compressor, DeleteStatus, PassthroughCompressor,
};
use moka::{Expiry, future::Cache};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Expiration;

impl Expiry<CacheKey, CacheValue<Raw>> for Expiration {
    fn expire_after_create(
        &self,
        _key: &CacheKey,
        value: &CacheValue<Raw>,
        _created_at: Instant,
    ) -> Option<Duration> {
        Self::calculate_ttl(value)
    }

    fn expire_after_update(
        &self,
        _key: &CacheKey,
        value: &CacheValue<Raw>,
        _updated_at: Instant,
        _duration_until_expiry: Option<Duration>,
    ) -> Option<Duration> {
        // Use the NEW value's expiration time, not the old one.
        // The default implementation returns `duration_until_expiry` which
        // would preserve the OLD expiration time, causing premature expiration.
        Self::calculate_ttl(value)
    }
}

impl Expiration {
    /// Calculate TTL from CacheValue's expire timestamp.
    fn calculate_ttl(value: &CacheValue<Raw>) -> Option<Duration> {
        value.expire().map(|expiration| {
            let delta = expiration - Utc::now();
            // Use milliseconds for sub-second precision.
            // Handle negative delta (already expired) by returning zero duration.
            let millis = delta.num_milliseconds();
            if millis <= 0 {
                Duration::ZERO
            } else {
                Duration::from_millis(millis as u64)
            }
        })
    }
}

#[derive(Clone)]
pub struct MokaBackend<S = JsonFormat, C = PassthroughCompressor>
where
    S: Format,
    C: Compressor,
{
    pub cache: Cache<CacheKey, CacheValue<Raw>>,
    pub key_format: CacheKeyFormat,
    pub serializer: S,
    pub compressor: C,
    pub label: BackendLabel,
}

impl<S, C> std::fmt::Debug for MokaBackend<S, C>
where
    S: Format,
    C: Compressor,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MokaBackend")
            .field("label", &self.label)
            .field("cache", &self.cache)
            .field("key_format", &self.key_format)
            .field("serializer", &std::any::type_name::<S>())
            .field("compressor", &std::any::type_name::<C>())
            .finish()
    }
}

impl MokaBackend<JsonFormat, PassthroughCompressor> {
    pub fn builder(
        max_capacity: u64,
    ) -> crate::builder::MokaBackendBuilder<JsonFormat, PassthroughCompressor> {
        crate::builder::MokaBackendBuilder::new(max_capacity)
    }
}

#[async_trait]
impl<S, C> Backend for MokaBackend<S, C>
where
    S: Format + Send + Sync,
    C: Compressor + Send + Sync,
{
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        self.cache.get(key).await.map(Ok).transpose()
    }

    async fn write(&self, key: &CacheKey, value: CacheValue<Raw>) -> BackendResult<()> {
        self.cache.insert(key.clone(), value).await;
        Ok(())
    }

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        let value = self.cache.remove(key).await;
        match value {
            Some(_) => Ok(DeleteStatus::Deleted(1)),
            None => Ok(DeleteStatus::Missing),
        }
    }

    fn label(&self) -> BackendLabel {
        self.label.clone()
    }

    fn value_format(&self) -> &dyn Format {
        &self.serializer
    }

    fn key_format(&self) -> &CacheKeyFormat {
        &self.key_format
    }

    fn compressor(&self) -> &dyn Compressor {
        &self.compressor
    }
}

// Explicit CacheBackend implementation using default trait methods
impl<S, C> hitbox_backend::CacheBackend for MokaBackend<S, C>
where
    S: Format + Send + Sync,
    C: Compressor + Send + Sync,
{
}
