use std::{future::Future, sync::Arc};

use async_trait::async_trait;
use bytes::Bytes;
use hitbox_core::{
    BackendLabel, BoxContext, CacheKey, CacheStatus, CacheValue, Cacheable, CacheableResponse, Raw,
    ReadMode, ResponseSource,
};

use crate::{
    BackendError, CacheKeyFormat, Compressor, DeleteStatus, PassthroughCompressor,
    format::{Format, FormatExt, JsonFormat},
    metrics::Timer,
};

pub type BackendResult<T> = Result<T, BackendError>;

/// Type alias for a dynamically dispatched Backend that is Send but not Sync.
pub type UnsyncBackend = dyn Backend + Send;

/// Type alias for a dynamically dispatched Backend that is Send + Sync.
pub type SyncBackend = dyn Backend + Send + Sync;

#[async_trait]
pub trait Backend: Sync + Send {
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>>;

    async fn write(&self, key: &CacheKey, value: CacheValue<Raw>) -> BackendResult<()>;

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus>;

    /// Returns the label of this backend for source path composition.
    ///
    /// This is used to build hierarchical source paths like "composition.l1.moka"
    /// when backends are nested within CompositionBackend.
    fn label(&self) -> BackendLabel {
        BackendLabel::new_static("backend")
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

    async fn write(&self, key: &CacheKey, value: CacheValue<Raw>) -> BackendResult<()> {
        (*self).write(key, value).await
    }

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        (*self).remove(key).await
    }

    fn label(&self) -> BackendLabel {
        (*self).label()
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

    async fn write(&self, key: &CacheKey, value: CacheValue<Raw>) -> BackendResult<()> {
        (**self).write(key, value).await
    }

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        (**self).remove(key).await
    }

    fn label(&self) -> BackendLabel {
        (**self).label()
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
impl Backend for Arc<UnsyncBackend> {
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        (**self).read(key).await
    }

    async fn write(&self, key: &CacheKey, value: CacheValue<Raw>) -> BackendResult<()> {
        (**self).write(key, value).await
    }

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        (**self).remove(key).await
    }

    fn label(&self) -> BackendLabel {
        (**self).label()
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
impl Backend for Arc<SyncBackend> {
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        (**self).read(key).await
    }

    async fn write(&self, key: &CacheKey, value: CacheValue<Raw>) -> BackendResult<()> {
        (**self).write(key, value).await
    }

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        (**self).remove(key).await
    }

    fn label(&self) -> BackendLabel {
        (**self).label()
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
            let backend_label = self.label();

            let read_timer = Timer::new();
            let read_result = self.read(key).await;
            crate::metrics::record_read(backend_label.as_str(), read_timer.elapsed());

            match read_result {
                Ok(Some(value)) => {
                    let (meta, raw_data) = value.into_parts();
                    let raw_len = raw_data.len();
                    crate::metrics::record_read_bytes(backend_label.as_str(), raw_len);

                    let format = self.value_format();

                    let decompress_timer = Timer::new();
                    let decompressed = self.compressor().decompress(&raw_data)?;
                    crate::metrics::record_decompress(
                        backend_label.as_str(),
                        decompress_timer.elapsed(),
                    );

                    let decompressed_bytes = Bytes::from(decompressed);

                    // Deserialize using with_deserializer - context may be upgraded
                    let deserialize_timer = Timer::new();
                    let mut deserialized_opt: Option<T::Cached> = None;
                    format.with_deserializer(
                        &decompressed_bytes,
                        &mut |deserializer| {
                            let value: T::Cached = deserializer.deserialize()?;
                            deserialized_opt = Some(value);
                            Ok(())
                        },
                        ctx,
                    )?;
                    crate::metrics::record_deserialize(
                        backend_label.as_str(),
                        deserialize_timer.elapsed(),
                    );

                    let deserialized = deserialized_opt.ok_or_else(|| {
                        BackendError::InternalError(Box::new(std::io::Error::other(
                            "deserialization produced no result",
                        )))
                    })?;

                    let cached_value = CacheValue::new(deserialized, meta.expire, meta.stale);

                    // Refill L1 if read mode is Refill (data came from L2).
                    // CompositionFormat will create L1-only envelope, so only L1 gets populated.
                    if ctx.read_mode() == ReadMode::Refill {
                        let _ = self.set::<T>(key, &cached_value, ctx).await;
                    }

                    ctx.set_status(CacheStatus::Hit);
                    ctx.set_source(ResponseSource::Backend(backend_label));
                    Ok(Some(cached_value))
                }
                Ok(None) => Ok(None),
                Err(e) => {
                    crate::metrics::record_read_error(backend_label.as_str());
                    Err(e)
                }
            }
        }
    }

    fn set<T>(
        &self,
        key: &CacheKey,
        value: &CacheValue<T::Cached>,
        ctx: &mut BoxContext,
    ) -> impl Future<Output = BackendResult<()>> + Send
    where
        T: CacheableResponse,
        T::Cached: Cacheable,
    {
        async move {
            // Skip write if this is a refill operation reaching the source backend.
            // The source backend already has this data - it provided it during get().
            // CompositionBackend handles L1 refill via its own set() implementation.
            if ctx.read_mode() == ReadMode::Refill {
                return Ok(());
            }

            let backend_label = self.label();
            let format = self.value_format();

            let serialize_timer = Timer::new();
            let serialized_value = format.serialize(&value.data, &**ctx)?;
            crate::metrics::record_serialize(backend_label.as_str(), serialize_timer.elapsed());

            let compress_timer = Timer::new();
            let compressed_value = self.compressor().compress(&serialized_value)?;
            crate::metrics::record_compress(backend_label.as_str(), compress_timer.elapsed());

            let compressed_len = compressed_value.len();

            let write_timer = Timer::new();
            let result = self
                .write(
                    key,
                    CacheValue::new(Bytes::from(compressed_value), value.expire, value.stale),
                )
                .await;
            crate::metrics::record_write(backend_label.as_str(), write_timer.elapsed());

            match result {
                Ok(()) => {
                    crate::metrics::record_write_bytes(backend_label.as_str(), compressed_len);
                    Ok(())
                }
                Err(e) => {
                    crate::metrics::record_write_error(backend_label.as_str());
                    Err(e)
                }
            }
        }
    }

    fn delete(
        &self,
        key: &CacheKey,
        _ctx: &mut BoxContext,
    ) -> impl Future<Output = BackendResult<DeleteStatus>> + Send {
        async move { self.remove(key).await }
    }
}

// Explicit CacheBackend implementations for trait objects
// These use the default implementations from the trait
impl CacheBackend for &dyn Backend {}

impl CacheBackend for Box<dyn Backend> {}

impl CacheBackend for Arc<UnsyncBackend> {}
impl CacheBackend for Arc<SyncBackend> {}
