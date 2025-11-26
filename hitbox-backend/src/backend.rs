use std::{future::Future, sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use hitbox_core::{
    BoxContext, CacheKey, CacheStatus, CacheValue, Cacheable, CacheableResponse, Raw, ReadMode,
    ResponseSource,
};

use crate::{
    BackendError, CacheKeyFormat, Compressor, DeleteStatus, PassthroughCompressor,
    format::{Format, FormatExt, JsonFormat},
};

pub type BackendResult<T> = Result<T, BackendError>;

#[async_trait]
pub trait Backend: Sync + Send {
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>>;

    async fn write(
        &self,
        key: &CacheKey,
        value: CacheValue<Raw>,
        ttl: Option<Duration>,
    ) -> BackendResult<()>;

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus>;

    /// Returns the name of this backend for source path composition.
    ///
    /// This is used to build hierarchical source paths like "composition.l1.moka"
    /// when backends are nested within CompositionBackend.
    fn name(&self) -> &str {
        "backend"
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

#[async_trait]
impl Backend for &dyn Backend {
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        (*self).read(key).await
    }

    async fn write(
        &self,
        key: &CacheKey,
        value: CacheValue<Raw>,
        ttl: Option<Duration>,
    ) -> BackendResult<()> {
        (*self).write(key, value, ttl).await
    }

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        (*self).remove(key).await
    }

    fn name(&self) -> &str {
        (*self).name()
    }

    fn value_format(&self) -> &dyn Format {
        (*self).value_format()
    }

    fn key_format(&self) -> &CacheKeyFormat {
        (*self).key_format()
    }

    fn compressor(&self) -> &dyn Compressor {
        (*self).compressor()
    }
}

#[async_trait]
impl Backend for Box<dyn Backend> {
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        (**self).read(key).await
    }

    async fn write(
        &self,
        key: &CacheKey,
        value: CacheValue<Raw>,
        ttl: Option<Duration>,
    ) -> BackendResult<()> {
        (**self).write(key, value, ttl).await
    }

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        (**self).remove(key).await
    }

    fn name(&self) -> &str {
        (**self).name()
    }

    fn value_format(&self) -> &dyn Format {
        (**self).value_format()
    }

    fn key_format(&self) -> &CacheKeyFormat {
        (**self).key_format()
    }

    fn compressor(&self) -> &dyn Compressor {
        (**self).compressor()
    }
}

#[async_trait]
impl Backend for Arc<dyn Backend + Send + 'static> {
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        (**self).read(key).await
    }

    async fn write(
        &self,
        key: &CacheKey,
        value: CacheValue<Raw>,
        ttl: Option<Duration>,
    ) -> BackendResult<()> {
        (**self).write(key, value, ttl).await
    }

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        (**self).remove(key).await
    }

    fn name(&self) -> &str {
        (**self).name()
    }

    fn value_format(&self) -> &dyn Format {
        (**self).value_format()
    }

    fn key_format(&self) -> &CacheKeyFormat {
        (**self).key_format()
    }

    fn compressor(&self) -> &dyn Compressor {
        (**self).compressor()
    }
}

/// High-level cache backend trait with typed operations.
///
/// This trait provides typed `get`, `set`, and `delete` operations that handle
/// serialization/deserialization and context tracking. The context is passed
/// as a mutable reference and updated in-place during operations.
pub trait CacheBackend: Backend {
    fn get<T>(
        &self,
        key: &CacheKey,
        ctx: &mut BoxContext,
    ) -> impl Future<Output = BackendResult<Option<CacheValue<T::Cached>>>> + Send
    where
        T: CacheableResponse,
        T::Cached: Cacheable,
    {
        async move {
            let backend_name = self.name().to_owned();
            let read_result = self.read(key).await;

            match read_result {
                Ok(Some(value)) => {
                    let bytes_read = value.data.len() as u64;
                    let (meta, raw_data) = value.into_parts();
                    let format = self.value_format();
                    let decompressed = self.compressor().decompress(&raw_data)?;
                    let decompressed_bytes = Bytes::from(decompressed);

                    // Deserialize using with_deserializer - context may be upgraded
                    let mut deserialized_opt: Option<T::Cached> = None;
                    format.with_deserializer(&decompressed_bytes, &mut |deserializer| {
                        let value: T::Cached = deserializer.deserialize()?;
                        deserialized_opt = Some(value);
                        Ok(())
                    }, ctx)?;

                    let deserialized = deserialized_opt.ok_or_else(|| {
                        BackendError::InternalError(Box::new(std::io::Error::other(
                            "deserialization produced no result",
                        )))
                    })?;

                    let cached_value = CacheValue::new(deserialized, meta.expire, meta.stale);

                    // Refill L1 if read mode is Refill (data came from L2).
                    // CompositionFormat will create L1-only envelope, so only L1 gets populated.
                    if ctx.read_mode() == ReadMode::Refill {
                        let _ = self.set::<T>(key, &cached_value, cached_value.ttl(), ctx).await;
                    }

                    // Record read metrics
                    ctx.metrics_mut().record_read(&backend_name, bytes_read, true);
                    ctx.set_status(CacheStatus::Hit);
                    ctx.set_source(ResponseSource::Backend(backend_name));
                    Ok(Some(cached_value))
                }
                Ok(None) => {
                    // Record read miss (0 bytes)
                    ctx.metrics_mut().record_read(&backend_name, 0, true);
                    Ok(None)
                }
                Err(e) => {
                    // Record read error
                    ctx.metrics_mut().record_read(&backend_name, 0, false);
                    Err(e)
                }
            }
        }
    }

    fn set<T>(
        &self,
        key: &CacheKey,
        value: &CacheValue<T::Cached>,
        ttl: Option<Duration>,
        ctx: &mut BoxContext,
    ) -> impl Future<Output = BackendResult<()>> + Send
    where
        T: CacheableResponse,
        T::Cached: Cacheable,
    {
        async move {
            let backend_name = self.name().to_owned();
            let format = self.value_format();
            // Use the context for serialization (allows CompositionFormat to check policy)
            let serialized_value = format.serialize(&value.data, &**ctx)?;
            let compressed_value = self.compressor().compress(&serialized_value)?;
            let bytes_written = compressed_value.len() as u64;
            let result = self
                .write(
                    key,
                    CacheValue::new(Bytes::from(compressed_value), value.expire, value.stale),
                    ttl,
                )
                .await;
            // Record write metrics
            ctx.metrics_mut()
                .record_write(&backend_name, bytes_written, result.is_ok());
            result
        }
    }

    fn delete(
        &self,
        key: &CacheKey,
        ctx: &mut BoxContext,
    ) -> impl Future<Output = BackendResult<DeleteStatus>> + Send {
        async move {
            let backend_name = self.name().to_owned();
            let result = self.remove(key).await;
            // Record delete metrics
            ctx.metrics_mut().record_delete(&backend_name, result.is_ok());
            result
        }
    }
}

// Explicit CacheBackend implementations for trait objects
// These use the default implementations from the trait
impl CacheBackend for &dyn Backend {}

impl CacheBackend for Box<dyn Backend> {}

impl CacheBackend for Arc<dyn Backend + Send + 'static> {}
