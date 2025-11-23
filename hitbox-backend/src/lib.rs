// #![warn(missing_docs)]
//! Traits and structs for hitbox backend interaction.
//!
//! If you want implement your own backend, you in the right place.
mod backend;
pub mod composition;
pub mod compressor;
pub mod context;
mod key;
pub mod format;

pub use backend::{Backend, BackendResult, CacheBackend};
pub use composition::{Compose, CompositionBackend};
pub use context::{BackendContext, BackendPolicy};
#[cfg(feature = "gzip")]
pub use compressor::GzipCompressor;
#[cfg(feature = "zstd")]
pub use compressor::ZstdCompressor;
pub use compressor::{CompressionError, Compressor, PassthroughCompressor};
pub use key::{CacheKeyFormat, KeySerializer, UrlEncodedKeySerializer};
#[cfg(feature = "rkyv_format")]
pub use format::RkyvFormat;
use format::FormatError;
use thiserror::Error;

/// Proxy Error describes general groups of errors in backend interaction process.
#[derive(Debug, Error)]
pub enum BackendError {
    /// Internal backend error, state or computation error.
    ///
    /// Any error not bounded with network interaction.
    #[error(transparent)]
    InternalError(Box<dyn std::error::Error + Send>),
    /// Network interaction error.
    #[error(transparent)]
    ConnectionError(Box<dyn std::error::Error + Send>),
    /// Serializing\Deserializing data error.
    #[error(transparent)]
    FormatError(#[from] FormatError),
    /// Compressing\Decompressing data error.
    #[error(transparent)]
    CompressionError(#[from] CompressionError),
}

/// Status of deleting result.
#[derive(Debug, PartialEq, Eq)]
pub enum DeleteStatus {
    /// Record successfully deleted.
    Deleted(u32),
    /// Record already missing.
    Missing,
}

/// Enum for representing status of Lock object in backend.
#[derive(Debug, PartialEq, Eq)]
pub enum LockStatus {
    /// Lock successfully created and acquired.
    Acquired,
    /// Lock object already acquired (locked).
    Locked,
}
