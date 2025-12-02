mod cacheable;
mod context;
mod extractor;
mod key;
mod label;
mod offload;
mod policy;
mod predicate;
mod request;
mod response;
mod time_provider;
mod upstream;
mod value;

pub use cacheable::Cacheable;
#[cfg(feature = "rkyv_format")]
pub use cacheable::{RkyvDeserializeError, RkyvDeserializer};
pub use context::{
    BoxContext, CacheContext, CacheStatus, Context, DebugState, ReadMode, ResponseSource,
    finalize_context,
};
pub use extractor::Extractor;
pub use key::{CacheKey, KeyPart, KeyParts};
pub use label::BackendLabel;
pub use offload::Offload;
pub use policy::{CachePolicy, EntityPolicyConfig};
pub use predicate::{Predicate, PredicateResult};
pub use request::{CacheablePolicyData, CacheableRequest, RequestCachePolicy};
pub use response::{CacheState, CacheableResponse, ResponseCachePolicy};
pub use smallbox::space::S4;
pub use smol_str::SmolStr;
pub use time_provider::TimeProvider;
pub use upstream::Upstream;
pub use value::CacheValue;

/// Raw byte data type used for serialized cache values.
/// Using `Bytes` provides efficient zero-copy cloning via reference counting.
pub type Raw = bytes::Bytes;

// Export test helpers when the test-helpers feature is enabled (for integration tests)
// or when running unit tests
#[cfg(any(test, feature = "test-helpers"))]
pub use value::{current_time, set_mock_time_provider};
