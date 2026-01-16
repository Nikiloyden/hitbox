//! Builder for configuring [`MokaBackend`].

use std::time::{Duration, Instant};

use chrono::Utc;
use moka::Expiry;
use moka::future::{Cache, CacheBuilder};

use crate::backend::MokaBackend;
use hitbox::{BackendLabel, CacheKey, CacheValue, Raw};
use hitbox_backend::format::{Format, JsonFormat};
use hitbox_backend::{CacheKeyFormat, Compressor, PassthroughCompressor};

/// Custom expiration policy that calculates TTL from [`CacheValue::expire`] timestamps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Expiration;

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
        // IMPORTANT: Always use the NEW value's expiration time.
        //
        // Moka's default `expire_after_update` returns `duration_until_expiry`,
        // which preserves the OLD expiration time. This causes premature expiration
        // when updating a cache entry with a new (longer) TTL.
        Self::calculate_ttl(value)
    }
}

impl Expiration {
    fn calculate_ttl(value: &CacheValue<Raw>) -> Option<Duration> {
        value.expire().map(|expiration| {
            let delta = expiration - Utc::now();
            let millis = delta.num_milliseconds();
            if millis <= 0 {
                Duration::ZERO
            } else {
                Duration::from_millis(millis as u64)
            }
        })
    }
}

/// Builder for creating and configuring a [`MokaBackend`].
///
/// Use [`MokaBackend::builder`] to create a new builder instance.
///
/// # Examples
///
/// Basic usage:
///
/// ```
/// use hitbox_moka::MokaBackend;
///
/// let backend = MokaBackend::builder(10_000).build();
/// ```
///
/// With custom configuration:
///
/// ```ignore
/// use hitbox_moka::MokaBackend;
/// use hitbox_backend::format::BincodeFormat;
/// use hitbox_backend::{CacheKeyFormat, GzipCompressor};
///
/// let backend = MokaBackend::builder(50_000)
///     .label("sessions")
///     .key_format(CacheKeyFormat::UrlEncoded)
///     .value_format(BincodeFormat)
///     .compressor(GzipCompressor::default())
///     .build();
/// ```
///
/// **Note:** [`GzipCompressor`](hitbox_backend::GzipCompressor) and
/// [`ZstdCompressor`](hitbox_backend::ZstdCompressor) require enabling the `gzip`
/// or `zstd` feature on `hitbox-backend`.
pub struct MokaBackendBuilder<S = JsonFormat, C = PassthroughCompressor>
where
    S: Format,
    C: Compressor,
{
    builder: CacheBuilder<CacheKey, CacheValue<Raw>, Cache<CacheKey, CacheValue<Raw>>>,
    key_format: CacheKeyFormat,
    serializer: S,
    compressor: C,
    label: BackendLabel,
}

impl MokaBackendBuilder<JsonFormat, PassthroughCompressor> {
    /// Creates a new builder with the specified maximum capacity.
    ///
    /// When the cache exceeds `max_capacity` entries, least recently used
    /// entries are evicted.
    pub fn new(max_capacity: u64) -> Self {
        let builder = CacheBuilder::new(max_capacity);
        Self {
            builder,
            key_format: CacheKeyFormat::Bitcode,
            serializer: JsonFormat,
            compressor: PassthroughCompressor,
            label: BackendLabel::new_static("moka"),
        }
    }
}

impl<S, C> MokaBackendBuilder<S, C>
where
    S: Format,
    C: Compressor,
{
    /// Sets a custom label for this backend.
    ///
    /// The label identifies this backend in multi-tier cache compositions and
    /// appears in metrics and debug output.
    ///
    /// # Default
    ///
    /// `"moka"`
    pub fn label(mut self, label: impl Into<BackendLabel>) -> Self {
        self.label = label.into();
        self
    }

    /// Sets the cache key serialization format.
    ///
    /// The key format determines how [`CacheKey`] values are serialized for
    /// storage. This affects key size and debuggability.
    ///
    /// # Default
    ///
    /// [`CacheKeyFormat::Bitcode`]
    ///
    /// # Options
    ///
    /// | Format | Size | Human-readable |
    /// |--------|------|----------------|
    /// | [`Bitcode`](CacheKeyFormat::Bitcode) | Compact | No |
    /// | [`UrlEncoded`](CacheKeyFormat::UrlEncoded) | Larger | Yes |
    ///
    /// [`CacheKey`]: hitbox_core::CacheKey
    pub fn key_format(mut self, format: CacheKeyFormat) -> Self {
        self.key_format = format;
        self
    }

    /// Sets the cache value serialization format.
    ///
    /// The value format determines how cached data is serialized before storage.
    ///
    /// # Default
    ///
    /// [`JsonFormat`]
    ///
    /// # Options
    ///
    /// | Format | Speed | Size | Human-readable |
    /// |--------|-------|------|----------------|
    /// | [`JsonFormat`] | Slow | Large | Yes |
    /// | [`BincodeFormat`](hitbox_backend::format::BincodeFormat) | Fast | Compact | No |
    /// | [`RonFormat`](hitbox_backend::format::RonFormat) | Medium | Medium | Yes |
    pub fn value_format<NewS>(self, serializer: NewS) -> MokaBackendBuilder<NewS, C>
    where
        NewS: Format,
    {
        MokaBackendBuilder {
            builder: self.builder,
            key_format: self.key_format,
            serializer,
            compressor: self.compressor,
            label: self.label,
        }
    }

    /// Sets the compression strategy for cache values.
    ///
    /// Compression reduces memory usage at the cost of CPU time. For in-memory
    /// caches like Moka, compression is typically **not recommended** since
    /// memory access is fast and compression adds latency.
    ///
    /// # Default
    ///
    /// [`PassthroughCompressor`] (no compression)
    ///
    /// # Options
    ///
    /// | Compressor | Ratio | Speed | Feature flag |
    /// |------------|-------|-------|--------------|
    /// | [`PassthroughCompressor`] | None | Fastest | — |
    /// | [`GzipCompressor`](hitbox_backend::GzipCompressor) | Good | Medium | `gzip` |
    /// | [`ZstdCompressor`](hitbox_backend::ZstdCompressor) | Best | Fast | `zstd` |
    ///
    /// # When to Use Compression
    ///
    /// - Large cached values (>10KB)
    /// - Memory-constrained environments
    /// - When composing with network backends (compression done once, reused)
    pub fn compressor<NewC>(self, compressor: NewC) -> MokaBackendBuilder<S, NewC>
    where
        NewC: Compressor,
    {
        MokaBackendBuilder {
            builder: self.builder,
            key_format: self.key_format,
            serializer: self.serializer,
            compressor,
            label: self.label,
        }
    }

    /// Builds the [`MokaBackend`] with the configured settings.
    ///
    /// Consumes the builder and returns a fully configured backend ready for use.
    pub fn build(self) -> MokaBackend<S, C> {
        let expiry = Expiration;
        let cache = self.builder.expire_after(expiry).build();
        MokaBackend {
            cache,
            key_format: self.key_format,
            serializer: self.serializer,
            compressor: self.compressor,
            label: self.label,
        }
    }
}
