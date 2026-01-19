//! Error types for Redis backend operations.
//!
//! This module provides error types specific to the Redis backend. All errors
//! can be converted to [`BackendError`] for uniform error handling across
//! different cache backends.
//!
//! [`BackendError`]: hitbox_backend::BackendError

use hitbox_backend::BackendError;
use redis::RedisError;

/// Error type for Redis backend operations.
///
/// Wraps errors from the underlying [`redis`] crate. This enum may be extended
/// in the future to distinguish between different error categories (connection,
/// protocol, timeout, etc.).
///
/// # When You'll Encounter This
///
/// You typically don't handle this error directly. It appears when:
///
/// - Using [`RedisBackendBuilder::build`] with an invalid connection URL
/// - Performing the first cache operation when Redis is unreachable
///   (connection is established lazily)
/// - Performing cache operations when the Redis server returns an error
///
/// In most cases, this error is automatically converted to [`BackendError`]
/// and propagated through the cache layer.
///
/// # Examples
///
/// ```no_run
/// use hitbox_redis::{RedisBackend, ConnectionMode};
///
/// # fn main() {
/// // Invalid URL returns Error::Redis
/// let result = RedisBackend::builder()
///     .connection(ConnectionMode::single("not-a-valid-url"))
///     .build();
///
/// match result {
///     Ok(_) => println!("Connected"),
///     Err(e) => println!("Failed: {}", e),
/// }
/// # }
/// ```
///
/// [`RedisBackendBuilder::build`]: crate::RedisBackendBuilder::build
/// [`BackendError`]: hitbox_backend::BackendError
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An error from the underlying Redis client.
    ///
    /// This includes connection failures, protocol errors, authentication
    /// failures, and command execution errors.
    #[error("Redis backend error: {0}")]
    Redis(#[from] RedisError),

    /// Connection mode was not specified when building the backend.
    ///
    /// Call [`RedisBackendBuilder::connection`] before [`RedisBackendBuilder::build`].
    ///
    /// [`RedisBackendBuilder::connection`]: crate::RedisBackendBuilder::connection
    /// [`RedisBackendBuilder::build`]: crate::RedisBackendBuilder::build
    #[error("Connection mode not specified. Call .connection() before .build()")]
    MissingConnectionMode,
}

impl From<Error> for BackendError {
    fn from(error: Error) -> Self {
        Self::InternalError(Box::new(error))
    }
}
