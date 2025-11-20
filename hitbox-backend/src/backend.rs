use std::{future::Future, sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use hitbox_core::{CacheKey, CacheValue, CacheableResponse, Raw};
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    BackendContext, BackendError, CacheKeyFormat, Compressor, DeleteStatus, PassthroughCompressor,
    serializer::{Format, FormatExt, JsonFormat},
};

pub type BackendResult<T> = Result<T, BackendError>;

/// Value returned from `Backend::read()` with context.
///
/// The context provides hints for how higher-level operations should handle
/// this value (e.g., whether to write it back for cache refill).
pub struct BackendValue {
    /// The cached value (None if cache miss)
    pub value: Option<CacheValue<Raw>>,

    /// Context providing policy hints and optimization data
    pub context: Arc<dyn BackendContext>,
}

impl BackendValue {
    /// Create a backend value with no-context (using `()`)
    pub fn new(value: Option<CacheValue<Raw>>) -> Self {
        Self {
            value,
            context: Arc::new(()),
        }
    }

    /// Create a backend value with specific context
    pub fn with_context(value: Option<CacheValue<Raw>>, context: impl BackendContext + 'static) -> Self {
        Self {
            value,
            context: Arc::new(context),
        }
    }
}

#[async_trait]
pub trait Backend: Sync + Send {
    async fn read(&self, key: &CacheKey) -> BackendResult<BackendValue>;

    async fn write(
        &self,
        key: &CacheKey,
        value: CacheValue<Raw>,
        ttl: Option<Duration>,
    ) -> BackendResult<()>;

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus>;

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
    async fn read(&self, key: &CacheKey) -> BackendResult<BackendValue> {
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
    async fn read(&self, key: &CacheKey) -> BackendResult<BackendValue> {
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
    async fn read(&self, key: &CacheKey) -> BackendResult<BackendValue> {
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

// pub trait Backend: Send {
//     fn read(
//         &self,
//         key: &CacheKey,
//     ) -> impl Future<Output = BackendResult<Option<CacheValue<Raw>>>> + Send;
//
//     fn write(
//         &self,
//         key: &CacheKey,
//         value: CacheValue<Raw>,
//         ttl: Option<Duration>,
//     ) -> impl Future<Output = BackendResult<()>> + Send;
//
//     fn remove(&self, key: &CacheKey) -> impl Future<Output = BackendResult<DeleteStatus>> + Send;
// }

pub trait CacheBackend: Backend {
    fn get<T>(
        &self,
        key: &CacheKey,
    ) -> impl Future<Output = BackendResult<Option<CacheValue<T::Cached>>>> + Send
    where
        T: CacheableResponse,
        T::Cached: Serialize + DeserializeOwned + Send + Sync,
    {
        async move {
            let backend_value = self.read(key).await?;
            let policy = backend_value.context.policy();

            match backend_value.value {
                Some(value) => {
                    let (meta, value) = value.into_parts();
                    let format = self.value_format();
                    let decompressed = self.compressor().decompress(&value)?;
                    let decompressed_bytes = Bytes::from(decompressed);
                    let deserialized = format.deserialize(&decompressed_bytes)?;
                    let cached_value = CacheValue::new(deserialized, meta.expire, meta.stale);

                    // Honor write_after_read policy for refill operations
                    if policy.write_after_read {
                        // Write back using the context from the read operation
                        let _ = self.set::<T>(key, &cached_value, cached_value.ttl(), &*backend_value.context).await;
                    }

                    Ok(Some(cached_value))
                }
                None => Ok(None),
            }
        }
    }

    fn set<T>(
        &self,
        key: &CacheKey,
        value: &CacheValue<T::Cached>,
        ttl: Option<Duration>,
        context: &dyn BackendContext,
    ) -> impl Future<Output = BackendResult<()>> + Send
    where
        T: CacheableResponse,
        T::Cached: Serialize + Send + Sync,
    {
        async move {
            let format = self.value_format();
            let serialized_value = format.serialize(&value.data, context)?;
            let compressed_value = self.compressor().compress(&serialized_value)?;
            self.write(
                key,
                CacheValue::new(Bytes::from(compressed_value), value.expire, value.stale),
                ttl,
            )
            .await
        }
    }

    fn delete(&self, key: &CacheKey) -> impl Future<Output = BackendResult<DeleteStatus>> + Send {
        async move { self.remove(key).await }
    }
}

// Explicit CacheBackend implementations for trait objects
// These use the default implementations from the trait
impl CacheBackend for &dyn Backend {}

impl CacheBackend for Box<dyn Backend> {}

impl CacheBackend for Arc<dyn Backend + Send + 'static> {}
