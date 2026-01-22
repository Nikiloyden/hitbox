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
/// let backend = MokaBackend::builder().max_entries(10_000).build();
/// ```
///
/// With custom serialization format:
///
/// ```
/// use hitbox_moka::MokaBackend;
/// use hitbox_backend::format::BincodeFormat;
///
/// let backend = MokaBackend::builder()
///     .max_entries(10_000)
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
    pub(crate) cache: Cache<CacheKey, CacheValue<Raw>>,
    pub(crate) key_format: CacheKeyFormat,
    pub(crate) serializer: S,
    pub(crate) compressor: C,
    pub(crate) label: BackendLabel,
}

impl<S, C> MokaBackend<S, C>
where
    S: Format,
    C: Compressor,
{
    /// Returns a reference to the underlying Moka cache.
    ///
    /// This provides direct access to Moka-specific features like
    /// [`run_pending_tasks()`](Cache::run_pending_tasks) for synchronizing
    /// eviction in tests.
    pub fn cache(&self) -> &Cache<CacheKey, CacheValue<Raw>> {
        &self.cache
    }
}

impl MokaBackend<JsonFormat, PassthroughCompressor> {
    /// Creates a new builder for `MokaBackend`.
    ///
    /// You must configure capacity using [`max_entries()`] or [`max_bytes()`] before
    /// calling `build()`.
    ///
    /// [`max_entries()`]: crate::builder::MokaBackendBuilder::max_entries
    /// [`max_bytes()`]: crate::builder::MokaBackendBuilder::max_bytes
    pub fn builder() -> crate::builder::MokaBackendBuilder<
        crate::builder::NoCapacity,
        JsonFormat,
        PassthroughCompressor,
    > {
        crate::builder::MokaBackendBuilder::new()
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
