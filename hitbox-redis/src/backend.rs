//! Redis backend implementation.

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use hitbox::{BackendLabel, CacheKey, CacheValue, Raw};
use hitbox_backend::{
    Backend, BackendError, BackendResult, CacheKeyFormat, Compressor, DeleteStatus,
    PassthroughCompressor,
    format::{BincodeFormat, Format},
};
use redis::{Client, aio::ConnectionManager};
use tokio::sync::OnceCell;
use tracing::trace;

use crate::error::Error;

/// Distributed cache backend powered by Redis.
///
/// `RedisBackend` provides a high-performance distributed cache using Redis
/// as the storage layer. It uses a multiplexed connection ([`ConnectionManager`])
/// for efficient async operations, allowing many concurrent requests to share
/// a single underlying connection.
///
/// # Type Parameters
///
/// * `S` - Serialization format for cache values. Implements [`Format`].
///   Default: [`BincodeFormat`] (compact binary, recommended for production).
/// * `C` - Compression strategy for cache values. Implements [`Compressor`].
///   Default: [`PassthroughCompressor`] (no compression).
///
/// # Examples
///
/// Basic usage with defaults:
///
/// ```no_run
/// use hitbox_redis::RedisBackend;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = RedisBackend::builder()
///     .server("redis://localhost:6379/")
///     .build()?;
/// # Ok(())
/// # }
/// ```
///
/// With custom serialization format:
///
/// ```no_run
/// use hitbox_redis::RedisBackend;
/// use hitbox_backend::format::JsonFormat;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = RedisBackend::builder()
///     .server("redis://localhost:6379/")
///     .value_format(JsonFormat)
///     .build()?;
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// - **Read operations**: Single pipelined request (`HMGET` + `PTTL`)
/// - **Write operations**: Single pipelined request (`HSET` + `EXPIRE`)
/// - **Connection**: Lazy initialization, multiplexed for concurrent access
///
/// # Caveats
///
/// - **Network latency**: Expect ~0.5-2ms per operation depending on network
/// - **Connection failure**: Operations will fail if Redis is unreachable
/// - **Expire time approximation**: The `expire` timestamp returned on read is
///   calculated as `now + PTTL`, which may drift by the network round-trip time
///
/// # Design Rationale
///
/// ## Why Hash instead of String?
///
/// Redis Hashes (`HSET`/`HMGET`) are used instead of simple strings (`SET`/`GET`)
/// to store metadata alongside the cached data:
///
/// - The `"d"` field holds the serialized (and optionally compressed) data
/// - The `"s"` field holds the stale timestamp for stale-while-revalidate support
/// - The TTL is stored using Redis's native `EXPIRE` mechanism
///
/// This separation allows the backend to support cache staleness semantics
/// without encoding metadata into the serialized value.
///
/// ## Why Lazy Connection?
///
/// The connection to Redis is established on first use, not at construction time.
/// This allows creating backend instances without blocking, and avoids connection
/// overhead for backends that may not be used (e.g., L2 in a composition where
/// L1 always hits).
///
/// [`Format`]: hitbox_backend::format::Format
/// [`BincodeFormat`]: hitbox_backend::format::BincodeFormat
/// [`Compressor`]: hitbox_backend::Compressor
/// [`PassthroughCompressor`]: hitbox_backend::PassthroughCompressor
/// [`ConnectionManager`]: redis::aio::ConnectionManager
#[derive(Clone)]
pub struct RedisBackend<S = BincodeFormat, C = PassthroughCompressor>
where
    S: Format,
    C: Compressor,
{
    client: Client,
    connection: OnceCell<ConnectionManager>,
    serializer: S,
    key_format: CacheKeyFormat,
    compressor: C,
    label: BackendLabel,
}

impl RedisBackend<BincodeFormat, PassthroughCompressor> {
    /// Creates a new backend instance with default settings.
    ///
    /// Connects to `redis://127.0.0.1/` with [`BincodeFormat`] serialization
    /// and no compression.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] if the Redis connection URL is invalid.
    /// Note that actual connection errors occur lazily on first operation.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use hitbox_redis::RedisBackend;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = RedisBackend::new()?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`BincodeFormat`]: hitbox_backend::format::BincodeFormat
    /// [`BackendError`]: hitbox_backend::BackendError
    pub fn new() -> Result<Self, BackendError> {
        Ok(Self::builder().build()?)
    }

    /// Creates a new builder for `RedisBackend` with default settings.
    ///
    /// Use the builder to configure the connection URL, serialization format,
    /// key format, compression, and label.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use hitbox_redis::RedisBackend;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = RedisBackend::builder()
    ///     .server("redis://redis.example.com:6379/0")
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn builder() -> RedisBackendBuilder<BincodeFormat, PassthroughCompressor> {
        RedisBackendBuilder::default()
    }
}

impl<S, C> RedisBackend<S, C>
where
    S: Format,
    C: Compressor,
{
    /// Returns a reference to the Redis connection manager.
    ///
    /// The connection is established lazily on first call. Subsequent calls
    /// return the cached connection manager without reconnecting.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] if the connection to Redis fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use hitbox_redis::RedisBackend;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = RedisBackend::new()?;
    ///
    /// // Connection is established on first access
    /// let conn = backend.connection().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`BackendError`]: hitbox_backend::BackendError
    pub async fn connection(&self) -> Result<&ConnectionManager, BackendError> {
        trace!("Get connection manager");
        let manager = self
            .connection
            .get_or_try_init(|| {
                trace!("Initialize new redis connection manager");
                self.client.get_connection_manager()
            })
            .await
            .map_err(Error::from)?;
        Ok(manager)
    }
}

/// Builder for creating and configuring a [`RedisBackend`].
///
/// Use [`RedisBackend::builder`] to create a new builder instance.
///
/// # Examples
///
/// Basic usage:
///
/// ```no_run
/// use hitbox_redis::RedisBackend;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = RedisBackend::builder()
///     .server("redis://localhost:6379/")
///     .build()?;
/// # Ok(())
/// # }
/// ```
///
/// With custom configuration:
///
/// ```no_run
/// use hitbox_redis::RedisBackend;
/// use hitbox_backend::format::JsonFormat;
/// use hitbox_backend::CacheKeyFormat;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = RedisBackend::builder()
///     .server("redis://redis.example.com:6379/0")
///     .label("sessions")
///     .key_format(CacheKeyFormat::UrlEncoded)
///     .value_format(JsonFormat)
///     .build()?;
/// # Ok(())
/// # }
/// ```
pub struct RedisBackendBuilder<S = BincodeFormat, C = PassthroughCompressor>
where
    S: Format,
    C: Compressor,
{
    connection_info: String,
    serializer: S,
    key_format: CacheKeyFormat,
    compressor: C,
    label: BackendLabel,
}

impl Default for RedisBackendBuilder<BincodeFormat, PassthroughCompressor> {
    fn default() -> Self {
        Self {
            connection_info: "redis://127.0.0.1/".to_owned(),
            serializer: BincodeFormat,
            key_format: CacheKeyFormat::default(),
            compressor: PassthroughCompressor,
            label: BackendLabel::new_static("redis"),
        }
    }
}

impl<S, C> RedisBackendBuilder<S, C>
where
    S: Format,
    C: Compressor,
{
    /// Sets the Redis server connection URL.
    ///
    /// The URL format is `redis://[<username>][:<password>@]<host>[:<port>][/<database>]`.
    ///
    /// # Default
    ///
    /// `redis://127.0.0.1/`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use hitbox_redis::RedisBackend;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Simple connection
    /// let backend = RedisBackend::builder()
    ///     .server("redis://localhost:6379/")
    ///     .build()?;
    ///
    /// // With authentication and database selection
    /// let backend = RedisBackend::builder()
    ///     .server("redis://:password@redis.example.com:6379/2")
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn server(mut self, connection_info: impl Into<String>) -> Self {
        self.connection_info = connection_info.into();
        self
    }

    /// Sets the cache value serialization format.
    ///
    /// The value format determines how cached data is serialized before storage.
    ///
    /// # Default
    ///
    /// [`BincodeFormat`] (compact binary, recommended for production)
    ///
    /// # Options
    ///
    /// | Format | Speed | Size | Human-readable |
    /// |--------|-------|------|----------------|
    /// | [`BincodeFormat`] | Fast | Compact | No |
    /// | [`JsonFormat`](hitbox_backend::format::JsonFormat) | Slow | Large | Yes |
    /// | [`RonFormat`](hitbox_backend::format::RonFormat) | Medium | Medium | Yes |
    ///
    /// [`BincodeFormat`]: hitbox_backend::format::BincodeFormat
    pub fn value_format<NewS>(self, serializer: NewS) -> RedisBackendBuilder<NewS, C>
    where
        NewS: Format,
    {
        RedisBackendBuilder {
            connection_info: self.connection_info,
            serializer,
            key_format: self.key_format,
            compressor: self.compressor,
            label: self.label,
        }
    }

    /// Sets the cache key serialization format.
    ///
    /// The key format determines how [`CacheKey`] values are serialized for
    /// storage as Redis keys. This affects key size and debuggability.
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
    /// # Examples
    ///
    /// ```no_run
    /// use hitbox_redis::RedisBackend;
    /// use hitbox_backend::CacheKeyFormat;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // URL-encoded keys for easier debugging with redis-cli
    /// let backend = RedisBackend::builder()
    ///     .key_format(CacheKeyFormat::UrlEncoded)
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`CacheKey`]: hitbox::CacheKey
    pub fn key_format(mut self, key_format: CacheKeyFormat) -> Self {
        self.key_format = key_format;
        self
    }

    /// Sets a custom label for this backend.
    ///
    /// The label identifies this backend in multi-tier cache compositions and
    /// appears in metrics and debug output.
    ///
    /// # Default
    ///
    /// `"redis"`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use hitbox_redis::RedisBackend;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = RedisBackend::builder()
    ///     .label("sessions-redis")
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn label(mut self, label: impl Into<BackendLabel>) -> Self {
        self.label = label.into();
        self
    }

    /// Sets the compression strategy for cache values.
    ///
    /// Compression reduces network bandwidth and Redis memory usage at the cost
    /// of CPU time. For Redis backends, compression is often beneficial since
    /// network I/O is typically the bottleneck.
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
    /// - Cached values larger than ~1KB
    /// - High network latency to Redis
    /// - Redis memory is constrained
    /// - Using Redis persistence (RDB/AOF)
    ///
    /// [`PassthroughCompressor`]: hitbox_backend::PassthroughCompressor
    pub fn compressor<NewC>(self, compressor: NewC) -> RedisBackendBuilder<S, NewC>
    where
        NewC: Compressor,
    {
        RedisBackendBuilder {
            connection_info: self.connection_info,
            serializer: self.serializer,
            key_format: self.key_format,
            compressor,
            label: self.label,
        }
    }

    /// Builds the [`RedisBackend`] with the configured settings.
    ///
    /// Consumes the builder and returns a fully configured backend ready for use.
    /// Note that the actual Redis connection is established lazily on first operation.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Redis`] if the connection URL is invalid.
    ///
    /// [`Error::Redis`]: crate::error::Error::Redis
    pub fn build(self) -> Result<RedisBackend<S, C>, Error> {
        Ok(RedisBackend {
            client: Client::open(self.connection_info)?,
            connection: OnceCell::new(),
            serializer: self.serializer,
            key_format: self.key_format,
            compressor: self.compressor,
            label: self.label,
        })
    }
}

#[async_trait]
impl<S, C> Backend for RedisBackend<S, C>
where
    S: Format + Send + Sync,
    C: Compressor + Send + Sync,
{
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        let mut con = self.connection().await?.clone();
        let cache_key = self.key_format.serialize(key)?;

        // Pipeline: HMGET (data, stale) + PTTL with typed decoding
        let ((data, stale_ms), pttl): ((Option<Vec<u8>>, Option<i64>), i64) = redis::pipe()
            .cmd("HMGET")
            .arg(&cache_key)
            .arg("d")
            .arg("s")
            .cmd("PTTL")
            .arg(&cache_key)
            .query_async(&mut con)
            .await
            .map_err(Error::from)?;

        // If data is None, key doesn't exist
        let data = match data {
            Some(data) => Bytes::from(data),
            None => return Ok(None),
        };

        // Convert stale millis to DateTime
        let stale = stale_ms.and_then(DateTime::from_timestamp_millis);

        // Calculate expire from PTTL (milliseconds remaining)
        // PTTL returns: -2 if key doesn't exist, -1 if no TTL, else milliseconds
        let expire = (pttl > 0).then(|| Utc::now() + chrono::Duration::milliseconds(pttl));

        Ok(Some(CacheValue::new(data, expire, stale)))
    }

    async fn write(&self, key: &CacheKey, value: CacheValue<Raw>) -> BackendResult<()> {
        let mut con = self.connection().await?.clone();
        let cache_key = self.key_format.serialize(key)?;

        // Build HSET command with data field, optionally add stale field
        let mut cmd = redis::cmd("HSET");
        cmd.arg(&cache_key).arg("d").arg(value.data().as_ref());
        if let Some(stale) = value.stale() {
            cmd.arg("s").arg(stale.timestamp_millis());
        }

        // Pipeline: HSET + optional EXPIRE (computed from value.ttl())
        let mut pipe = redis::pipe();
        pipe.add_command(cmd).ignore();
        if let Some(ttl_duration) = value.ttl() {
            pipe.cmd("EXPIRE")
                .arg(&cache_key)
                .arg(ttl_duration.as_secs())
                .ignore();
        }

        pipe.query_async::<()>(&mut con)
            .await
            .map_err(Error::from)?;
        Ok(())
    }

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        let mut con = self.connection().await?.clone();
        let cache_key = self.key_format.serialize(key)?;

        let deleted: i32 = redis::cmd("DEL")
            .arg(cache_key)
            .query_async(&mut con)
            .await
            .map_err(Error::from)?;

        if deleted > 0 {
            Ok(DeleteStatus::Deleted(deleted as u32))
        } else {
            Ok(DeleteStatus::Missing)
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
impl<S, C> hitbox_backend::CacheBackend for RedisBackend<S, C>
where
    S: Format + Send + Sync,
    C: Compressor + Send + Sync,
{
}
