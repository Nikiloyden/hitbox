mod cacheable;
mod extractor;
mod key;
mod policy;
mod predicate;
mod request;
mod response;
mod time_provider;
mod upstream;
mod value;

pub use cacheable::Cacheable;
pub use extractor::Extractor;
pub use key::{CacheKey, KeyPart, KeyParts};
pub use policy::{CachePolicy, EntityPolicyConfig};
pub use predicate::{Predicate, PredicateResult};
pub use request::{CacheablePolicyData, CacheableRequest, RequestCachePolicy};
pub use response::{CacheState, CacheableResponse, ResponseCachePolicy};
pub use time_provider::TimeProvider;
pub use upstream::Upstream;
pub use value::CacheValue;

/// Raw byte data type used for serialized cache values.
/// Using `Bytes` provides efficient zero-copy cloning via reference counting.
pub type Raw = bytes::Bytes;

// Export test helpers when the test-helpers feature is enabled (for integration tests)
// or when running unit tests
#[cfg(any(test, feature = "test-helpers"))]
pub use value::set_mock_time_provider;
