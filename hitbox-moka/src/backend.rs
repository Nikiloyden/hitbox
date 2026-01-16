//! Moka backend implementation.

use async_trait::async_trait;
use hitbox::{BackendLabel, CacheKey, CacheValue, Raw};
use hitbox_backend::Backend;
use hitbox_backend::format::{Format, JsonFormat};
use hitbox_backend::{
    BackendResult, CacheKeyFormat, Compressor, DeleteStatus, PassthroughCompressor,
};
use moka::future::Cache;

/// In-memory cache backend powered by Moka.
///
/// `MokaBackend` provides a high-performance, concurrent in-memory cache with
/// automatic entry expiration. It uses Moka's async cache internally, which
/// offers lock-free reads and fine-grained locking for writes.
///
/// # Type Parameters
///
/// * `S` - Serialization format for cache values. Implements [`Format`].
///   Default: [`JsonFormat`].
/// * `C` - Compression strategy for cache values. Implements [`Compressor`].
///   Default: [`PassthroughCompressor`] (no compression).
///
/// # Examples
///
/// Basic usage with defaults:
///
/// ```
/// use hitbox_moka::MokaBackend;
///
/// let backend = MokaBackend::builder(10_000).build();
/// ```
///
/// With custom serialization format:
///
/// ```
/// use hitbox_moka::MokaBackend;
/// use hitbox_backend::format::BincodeFormat;
///
/// let backend = MokaBackend::builder(10_000)
///     .value_format(BincodeFormat)
///     .build();
/// ```
///
/// # Performance
///
/// - **Read operations**: Lock-free, O(1) average
/// - **Write operations**: Fine-grained locking, O(1) average
/// - **Memory**: Bounded by `max_capacity` entries
///
/// # Caveats
///
/// - Data is **not persisted** — cache is lost on process restart
/// - Data is **not shared** across processes — use Redis for distributed caching
/// - Expiration is **best-effort** — expired entries may briefly remain readable
///   until Moka's background eviction runs
///
/// [`Format`]: hitbox_backend::format::Format
/// [`JsonFormat`]: hitbox_backend::format::JsonFormat
/// [`Compressor`]: hitbox_backend::Compressor
/// [`PassthroughCompressor`]: hitbox_backend::PassthroughCompressor
#[derive(Clone)]
pub struct MokaBackend<S = JsonFormat, C = PassthroughCompressor>
where
    S: Format,
    C: Compressor,
{
    /// The underlying Moka async cache instance.
    pub cache: Cache<CacheKey, CacheValue<Raw>>,
    /// Format used to serialize cache keys.
    pub key_format: CacheKeyFormat,
    /// Format used to serialize cache values.
    pub serializer: S,
    /// Compressor used for cache values.
    pub compressor: C,
    /// Label identifying this backend in multi-tier compositions.
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
    /// Creates a new builder for `MokaBackend` with the specified maximum capacity.
    ///
    /// The `max_capacity` determines the maximum number of entries the cache can hold.
    /// When the cache reaches capacity, the least recently used entries are evicted.
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
