use serde::{Serialize, de::DeserializeOwned};

/// Trait that abstracts serialization requirements for cached values.
/// This trait provides different bounds based on whether the `rkyv_format` feature is enabled.
///
/// Without rkyv_format: requires Serde traits + Send + Sync
/// With rkyv_format: requires Serde + rkyv traits + Send + Sync
#[cfg(not(feature = "rkyv_format"))]
pub trait Cacheable: Serialize + DeserializeOwned + Send + Sync {}

#[cfg(not(feature = "rkyv_format"))]
impl<T> Cacheable for T where T: Serialize + DeserializeOwned + Send + Sync {}

/// rkyv 0.8 Cacheable trait.
/// Uses rkyv's high-level API (to_bytes/from_bytes) which handles serialization internally.
/// Requires:
/// - Archive + Serialize for serialization via to_bytes
/// - Archived type must implement CheckBytes for validation
/// - Archived type must implement Deserialize for from_bytes
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
