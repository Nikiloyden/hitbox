use serde::{Serialize, de::DeserializeOwned};

/// Trait that abstracts serialization requirements for cached values.
/// This trait provides different bounds based on whether the `rkyv_format` feature is enabled.
///
/// Without rkyv_format: requires Serde traits + Send + Sync
/// With rkyv_format: requires Serde + rkyv traits + Send + Sync
///
/// The rkyv version uses associated type bounds to eliminate the need for explicit
/// `Archived` bounds at usage sites.
#[cfg(not(feature = "rkyv_format"))]
pub trait Cacheable: Serialize + DeserializeOwned + Send + Sync {}

#[cfg(not(feature = "rkyv_format"))]
impl<T> Cacheable for T where T: Serialize + DeserializeOwned + Send + Sync {}

#[cfg(feature = "rkyv_format")]
pub trait Cacheable: Serialize
    + DeserializeOwned
    + Send
    + Sync
    + rkyv_dyn::SerializeDyn
    + rkyv::Archive<
        Archived: rkyv::Deserialize<Self, rkyv::Infallible>
                      + for<'a> rkyv::CheckBytes<rkyv::validation::validators::DefaultValidator<'a>>,
    >
{
}

#[cfg(feature = "rkyv_format")]
impl<T> Cacheable for T
where
    T: Serialize + DeserializeOwned + Send + Sync + rkyv_dyn::SerializeDyn + rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, rkyv::Infallible>
        + for<'a> rkyv::CheckBytes<rkyv::validation::validators::DefaultValidator<'a>>,
{
}
