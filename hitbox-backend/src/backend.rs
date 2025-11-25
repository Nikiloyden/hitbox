use std::{future::Future, sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use hitbox_core::{CacheKey, CacheValue, Cacheable, CacheableResponse, Raw};

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
        T::Cached: Cacheable,
    {
        async move {
            let value_opt = self.read(key).await?;

            match value_opt {
                Some(value) => {
                    let (meta, value) = value.into_parts();
                    let format = self.value_format();
                    let decompressed = self.compressor().decompress(&value)?;
                    let decompressed_bytes = Bytes::from(decompressed);

                    // Deserialize using with_deserializer to extract context
                    let mut deserialized_opt: Option<T::Cached> = None;
                    let (_, context) =
                        format.with_deserializer(&decompressed_bytes, &mut |deserializer| {
                            let value: T::Cached = deserializer.deserialize()?;
                            deserialized_opt = Some(value);
                            Ok(())
                        })?;

                    let deserialized = deserialized_opt.ok_or_else(|| {
                        BackendError::InternalError(Box::new(std::io::Error::other(
                            "deserialization produced no result",
                        )))
                    })?;

                    let cached_value = CacheValue::new(deserialized, meta.expire, meta.stale);

                    // Check if we should write back after read (refill from L2 to L1 in composition)
                    // CacheBackend is high-level code that only uses the trait interface
                    if context.policy().write_after_read {
                        // Write back using the context from deserialization
                        // Serialize with context, then call Backend::write directly
                        let serialized = format.serialize(&cached_value.data, &*context)?;
                        let compressed = self.compressor().compress(&serialized)?;
                        let raw_value = CacheValue::new(
                            Bytes::from(compressed),
                            cached_value.expire,
                            cached_value.stale,
                        );
                        let _ = self.write(key, raw_value, cached_value.ttl()).await;
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
    ) -> impl Future<Output = BackendResult<()>> + Send
    where
        T: CacheableResponse,
        T::Cached: Cacheable,
    {
        async move {
            let format = self.value_format();
            // Use unit context for normal writes (no refill context)
            let serialized_value = format.serialize(&value.data, &())?;
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
