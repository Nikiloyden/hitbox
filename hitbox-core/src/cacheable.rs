//! Serialization trait for cached values.
//!
//! This module provides the [`Cacheable`] trait which defines the serialization
//! requirements for types that can be stored in cache backends.
//!
//! ## Feature-Dependent Bounds
//!
//! The trait has different bounds depending on enabled features:
//!
//! - **Default**: Requires `Serialize + DeserializeOwned + Send + Sync` (serde)
//! - **`rkyv_format`**: Additionally requires rkyv traits for zero-copy deserialization
//!
//! ## Blanket Implementation
//!
//! The trait is automatically implemented for all types that satisfy the bounds,
//! so you don't need to implement it manually. Just derive the required traits:
//!
//! ```ignore
//! #[derive(serde::Serialize, serde::Deserialize)]
//! #[cfg_attr(feature = "rkyv_format", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]
//! struct MyCachedData {
//!     value: String,
//! }
//! // MyCachedData automatically implements Cacheable
//! ```

use serde::{Serialize, de::DeserializeOwned};

/// Marker trait for types that can be cached.
///
/// This trait abstracts serialization requirements for cached values.
/// It has a blanket implementation for all types that satisfy the bounds,
/// so you never need to implement it manually.
///
/// # Feature-Dependent Bounds
///
/// ## Without `rkyv_format` (default)
///
/// Requires serde traits for JSON/binary serialization:
/// - `Serialize + DeserializeOwned + Send + Sync`
///
/// ## With `rkyv_format`
///
/// Additionally requires rkyv traits for zero-copy deserialization:
/// - `Archive` - Type can be archived
/// - `Serialize` - Can serialize to rkyv format
/// - `Archived: CheckBytes` - Archived form can be validated
/// - `Archived: Deserialize` - Can deserialize from archived form
///
/// # Example
///
/// ```ignore
/// // Works with default features (serde only)
/// #[derive(serde::Serialize, serde::Deserialize)]
/// struct BasicData {
///     value: String,
/// }
///
/// // Works with rkyv_format feature
/// #[derive(serde::Serialize, serde::Deserialize)]
/// #[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
/// #[rkyv(check_bytes)]
/// struct RkyvData {
///     value: String,
/// }
/// ```
#[cfg(not(feature = "rkyv_format"))]
pub trait Cacheable: Serialize + DeserializeOwned + Send + Sync {}

#[cfg(not(feature = "rkyv_format"))]
impl<T> Cacheable for T where T: Serialize + DeserializeOwned + Send + Sync {}

/// Marker trait for types that can be cached.
///
/// This trait abstracts serialization requirements for cached values.
/// It has a blanket implementation for all types that satisfy the bounds,
/// so you never need to implement it manually.
///
/// # Feature-Dependent Bounds
///
/// ## Without `rkyv_format` (default)
///
/// Requires serde traits for JSON/binary serialization:
/// - `Serialize + DeserializeOwned + Send + Sync`
///
/// ## With `rkyv_format`
///
/// Additionally requires rkyv traits for zero-copy deserialization:
/// - `Archive` - Type can be archived
/// - `Serialize` - Can serialize to rkyv format
/// - `Archived: CheckBytes` - Archived form can be validated
/// - `Archived: Deserialize` - Can deserialize from archived form
///
/// # Example
///
/// ```ignore
/// // Works with default features (serde only)
/// #[derive(serde::Serialize, serde::Deserialize)]
/// struct BasicData {
///     value: String,
/// }
///
/// // Works with rkyv_format feature
/// #[derive(serde::Serialize, serde::Deserialize)]
/// #[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
/// #[rkyv(check_bytes)]
/// struct RkyvData {
///     value: String,
/// }
/// ```
#[cfg(feature = "rkyv_format")]
pub trait Cacheable:
    Serialize
    + DeserializeOwned
    + Send
    + Sync
    + rkyv::Archive<
        Archived: for<'a> rkyv::bytecheck::CheckBytes<
            rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>,
        > + rkyv::Deserialize<
            Self,
            rkyv::rancor::Strategy<rkyv::de::Pool, rkyv::rancor::Error>,
        >,
    > + for<'a> rkyv::Serialize<
        rkyv::api::high::HighSerializer<
            rkyv::util::AlignedVec,
            rkyv::ser::allocator::ArenaHandle<'a>,
            rkyv::rancor::Error,
        >,
    >
{
}

#[cfg(feature = "rkyv_format")]
impl<T> Cacheable for T
where
    T: Serialize + DeserializeOwned + Send + Sync + rkyv::Archive,
    T::Archived: for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>
        + rkyv::Deserialize<T, rkyv::rancor::Strategy<rkyv::de::Pool, rkyv::rancor::Error>>,
    T: for<'a> rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                rkyv::util::AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                rkyv::rancor::Error,
            >,
        >,
{
}
