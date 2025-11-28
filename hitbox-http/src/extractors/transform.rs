//! Shared value transformations for extractors.

use sha2::{Digest, Sha256};

/// Value transformation.
#[derive(Debug, Clone, Copy)]
pub enum Transform {
    /// SHA256 hash (truncated to 16 hex chars)
    Hash,
    /// Convert to lowercase
    Lowercase,
    /// Convert to uppercase
    Uppercase,
}

/// Apply SHA256 hash to value (truncated to 16 hex chars).
pub fn apply_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

/// Apply a single transform to a value.
pub fn apply_single_transform(value: String, transform: &Transform) -> String {
    match transform {
        Transform::Hash => apply_hash(&value),
        Transform::Lowercase => value.to_lowercase(),
        Transform::Uppercase => value.to_uppercase(),
    }
}

/// Apply a chain of transforms to a value.
pub fn apply_transform_chain(mut value: String, chain: &[Transform]) -> String {
    for transform in chain {
        value = apply_single_transform(value, transform);
    }
    value
}
