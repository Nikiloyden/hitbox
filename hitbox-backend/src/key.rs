//! Cache key serialization formats.
//!
//! This module provides different strategies for serializing [`CacheKey`] to bytes
//! for storage in backends.
//!
//! # Available Formats
//!
//! | Format | Size | Reversible | Use Case |
//! |--------|------|------------|----------|
//! | [`Bitcode`](CacheKeyFormat::Bitcode) | Compact | Yes | Default, most backends |
//! | [`UrlEncoded`](CacheKeyFormat::UrlEncoded) | Larger | Yes | Debugging, human-readable keys |

use std::{iter::once, str::from_utf8};

use hitbox_core::{CacheKey, KeyPart};

use crate::format::FormatError;

const PREFIX_KEY: &str = "_prefix";
const VERSION_KEY: &str = "_version";

/// Cache key serialization format.
///
/// Determines how [`CacheKey`] is converted to bytes for storage.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheKeyFormat {
    /// Compact binary format using bitcode.
    ///
    /// Produces the smallest keys. This is the default and recommended format.
    #[default]
    Bitcode,

    /// URL-encoded query string format.
    ///
    /// Produces human-readable keys like `_prefix=api&_version=1&user=123`.
    /// Useful for debugging and human-readable storage keys.
    UrlEncoded,
}

impl CacheKeyFormat {
    /// Serialize a cache key to bytes.
    pub fn serialize(&self, key: &CacheKey) -> Result<Vec<u8>, FormatError> {
        match self {
            CacheKeyFormat::Bitcode => Ok(bitcode::encode(key)),
            CacheKeyFormat::UrlEncoded => {
                let pairs = once((PREFIX_KEY, key.prefix().to_string()))
                    .chain(once((VERSION_KEY, key.version().to_string())))
                    .chain(
                        key.parts()
                            .map(|p| (p.key(), p.value().unwrap_or_default().to_string())),
                    )
                    .collect::<Vec<_>>();

                serde_urlencoded::to_string(pairs)
                    .map(String::into_bytes)
                    .map_err(|err| FormatError::Serialize(Box::new(err)))
            }
        }
    }

    /// Deserialize bytes back to a cache key.
    pub fn deserialize(&self, data: &[u8]) -> Result<CacheKey, FormatError> {
        match self {
            CacheKeyFormat::Bitcode => {
                bitcode::decode(data).map_err(|err| FormatError::Deserialize(Box::new(err)))
            }
            CacheKeyFormat::UrlEncoded => {
                let input =
                    from_utf8(data).map_err(|err| FormatError::Deserialize(Box::new(err)))?;

                let pairs: Vec<(String, String)> = serde_urlencoded::from_str(input)
                    .map_err(|err| FormatError::Deserialize(Box::new(err)))?;

                let (mut prefix, mut version, mut parts) = (String::new(), 0u32, Vec::new());

                for (key, value) in pairs {
                    match key.as_str() {
                        PREFIX_KEY => prefix = value,
                        VERSION_KEY => {
                            version = value
                                .parse()
                                .map_err(|err| FormatError::Deserialize(Box::new(err)))?;
                        }
                        _ => {
                            let v = if value.is_empty() { None } else { Some(value) };
                            parts.push(KeyPart::new(key, v));
                        }
                    }
                }

                Ok(CacheKey::new(prefix, version, parts))
            }
        }
    }
}
