//! Error types for backend operations.

use crate::compressor::CompressionError;
use crate::format::FormatError;
use thiserror::Error;

/// Error type for backend operations.
///
/// This enum categorizes errors that can occur during cache backend interactions
/// into distinct groups for appropriate handling.
#[derive(Debug, Error)]
pub enum BackendError {
    /// Internal backend error, state or computation error.
    ///
    /// Any error not related to network interaction.
    #[error(transparent)]
    InternalError(Box<dyn std::error::Error + Send>),

    /// Network interaction error.
    ///
    /// Errors occurring during communication with remote backends (e.g., Redis).
    #[error(transparent)]
    ConnectionError(Box<dyn std::error::Error + Send>),

    /// Serialization or deserialization error.
    #[error(transparent)]
    FormatError(#[from] FormatError),

    /// Compression or decompression error.
    #[error(transparent)]
    CompressionError(#[from] CompressionError),
}
