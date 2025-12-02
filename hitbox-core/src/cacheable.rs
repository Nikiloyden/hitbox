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

/// Error type for rkyv deserialization failures.
/// This is a simple wrapper around a string message to allow deserialization
/// to fail with meaningful error messages.
#[cfg(feature = "rkyv_format")]
#[derive(Debug)]
pub struct RkyvDeserializeError {
    pub message: String,
}

#[cfg(feature = "rkyv_format")]
impl RkyvDeserializeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[cfg(feature = "rkyv_format")]
impl std::fmt::Display for RkyvDeserializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rkyv deserialization error: {}", self.message)
    }
}

#[cfg(feature = "rkyv_format")]
impl std::error::Error for RkyvDeserializeError {}

/// A deserializer for rkyv that can produce errors during deserialization.
/// Unlike `rkyv::Infallible`, this allows deserialization to fail with
/// meaningful error messages.
#[cfg(feature = "rkyv_format")]
#[derive(Debug, Default)]
pub struct RkyvDeserializer;

#[cfg(feature = "rkyv_format")]
impl rkyv::Fallible for RkyvDeserializer {
    type Error = RkyvDeserializeError;
}

#[cfg(feature = "rkyv_format")]
pub trait Cacheable: Serialize
    + DeserializeOwned
    + Send
    + Sync
    + rkyv_dyn::SerializeDyn
    + rkyv::Archive<
        Archived: rkyv::Deserialize<Self, RkyvDeserializer>
                      + for<'a> rkyv::CheckBytes<rkyv::validation::validators::DefaultValidator<'a>>,
    >
{
}

#[cfg(feature = "rkyv_format")]
impl<T> Cacheable for T
where
    T: Serialize + DeserializeOwned + Send + Sync + rkyv_dyn::SerializeDyn + rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, RkyvDeserializer>
        + for<'a> rkyv::CheckBytes<rkyv::validation::validators::DefaultValidator<'a>>,
{
}
