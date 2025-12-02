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

/// Redis cache backend based on redis-rs crate.
///
/// This struct provides Redis as a storage [`Backend`] for hitbox.
/// It uses a [`ConnectionManager`] for asynchronous network interaction.
///
/// [`ConnectionManager`]: redis::aio::ConnectionManager
/// [`Backend`]: hitbox_backend::Backend
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
    name: BackendLabel,
}

impl RedisBackend<BincodeFormat, PassthroughCompressor> {
    /// Create new backend instance with default settings.
    ///
    /// # Examples
    /// ```
    /// use hitbox_redis::RedisBackend;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let backend = RedisBackend::new();
    /// }
    /// ```
    pub fn new() -> Result<Self, BackendError> {
        Ok(Self::builder().build()?)
    }

    /// Creates new RedisBackend builder with default settings.
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
    /// Create lazy connection to redis via [`ConnectionManager`]
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

/// Part of builder pattern implementation for RedisBackend.
pub struct RedisBackendBuilder<S = BincodeFormat, C = PassthroughCompressor>
where
    S: Format,
    C: Compressor,
{
    connection_info: String,
    serializer: S,
    key_format: CacheKeyFormat,
    compressor: C,
    name: BackendLabel,
}

impl Default for RedisBackendBuilder<BincodeFormat, PassthroughCompressor> {
    fn default() -> Self {
        Self {
            connection_info: "redis://127.0.0.1/".to_owned(),
            serializer: BincodeFormat,
            key_format: CacheKeyFormat::default(),
            compressor: PassthroughCompressor,
            name: BackendLabel::new_static("redis"),
        }
    }
}

impl<S, C> RedisBackendBuilder<S, C>
where
    S: Format,
    C: Compressor,
{
    /// Set connection info (host, port, database, etc.) for RedisBackend.
    pub fn server(mut self, connection_info: impl Into<String>) -> Self {
        self.connection_info = connection_info.into();
        self
    }

    /// Set value serialization format (JSON, Bincode, etc.)
    pub fn value_format<NewS>(self, serializer: NewS) -> RedisBackendBuilder<NewS, C>
    where
        NewS: Format,
    {
        RedisBackendBuilder {
            connection_info: self.connection_info,
            serializer,
            key_format: self.key_format,
            compressor: self.compressor,
            name: self.name,
        }
    }

    /// Set key serialization format (String, JSON, Bincode, UrlEncoded)
    pub fn key_format(mut self, key_format: CacheKeyFormat) -> Self {
        self.key_format = key_format;
        self
    }

    /// Set a custom name for this backend.
    ///
    /// The name is used for source path composition in multi-layer caches.
    /// For example, with name "sessions", the source path might be "composition.L1.sessions".
    pub fn name(mut self, name: impl Into<BackendLabel>) -> Self {
        self.name = name.into();
        self
    }

    /// Set compressor for value compression
    pub fn compressor<NewC>(self, compressor: NewC) -> RedisBackendBuilder<S, NewC>
    where
        NewC: Compressor,
    {
        RedisBackendBuilder {
            connection_info: self.connection_info,
            serializer: self.serializer,
            key_format: self.key_format,
            compressor,
            name: self.name,
        }
    }

    /// Create new instance of Redis backend with passed settings.
    pub fn build(self) -> Result<RedisBackend<S, C>, Error> {
        Ok(RedisBackend {
            client: Client::open(self.connection_info)?,
            connection: OnceCell::new(),
            serializer: self.serializer,
            key_format: self.key_format,
            compressor: self.compressor,
            name: self.name,
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
        cmd.arg(&cache_key).arg("d").arg(value.data.as_ref());
        if let Some(stale) = value.stale {
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

    fn name(&self) -> BackendLabel {
        self.name.clone()
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
