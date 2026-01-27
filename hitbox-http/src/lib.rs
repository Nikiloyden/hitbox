#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

pub mod body;
mod cache_status;
mod cacheable;
pub mod extractors;
pub mod predicates;
pub mod query;
mod request;
mod response;

pub use body::{BufferedBody, CollectExactResult, PartialBufferedBody, Remaining};
pub use cache_status::DEFAULT_CACHE_STATUS_HEADER;
pub use cacheable::CacheableSubject;
pub use request::CacheableHttpRequest;
pub use response::{CacheableHttpResponse, SerializableHttpResponse};
