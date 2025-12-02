//! An a implementation and infrastructure for asynchronous and clear cache integration.
//!
//! # A quick tour of hitbox
//!
//! Our crates consist of next main part:
//! * [Backend] trait and its implementation ([RedisBackend]).
//! * [CacheableResponse] trait.
//! * Cache implementation and framework integrations.
//!
//! ## Features
//! - [x] Automatic cache key generation.
//! - [x] Framework integrations:
//!     - [x] Actix ([hitbox-actix])
//!     - [ ] Actix-Web
//! - [x] Multiple cache backend implementations:
//!     - [x] [RedisBackend]
//!     - [ ] In-memory backend
//! - [x] Stale cache mechanics.
//! - [ ] Cache locks for [dogpile effect] preventions.
//! - [ ] Distributed cache locks.
//! - [ ] Detailed metrics out of the box.
//!
//! ## Feature flags
//! * derive - Support for deriving cache-related traits.
//! * metrics - Support for metrics.
//!
//! ## Restrictions
//! Default cache key implementation based on serde_qs crate
//! and have some [restrictions](https://docs.rs/serde_qs/latest/serde_qs/#supported-types).
//!
//! ## Example
//! See the [examples](https://github.com/hit-box/hitbox/tree/main/examples) directory for
//! complete usage examples with Tower, Axum, and various backends.
//!
//! [CacheableResponse]: crate::CacheableResponse
//! [Backend]: hitbox_backend::Backend
//! [RedisBackend]: https://docs.rs/hitbox_redis/
//! [hitbox-actix]: https://docs.rs/hitbox_actix/
//! [dogpile effect]: https://www.sobstel.org/blog/preventing-dogpile-effect/
#![allow(missing_docs)] // TODO: replace to warn
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod backend;
pub mod concurrency;
pub mod error;
pub mod fsm;
pub mod metrics;
pub use error::CacheError;
pub use hitbox_core::{
    CacheKey, CachePolicy, CacheState, CacheValue, CacheablePolicyData, CacheableRequest,
    CacheableResponse, EntityPolicyConfig, Extractor, KeyPart, KeyParts, Predicate, Raw,
    RequestCachePolicy, ResponseCachePolicy,
};
#[cfg(feature = "rkyv_format")]
pub use hitbox_core::{RkyvDeserializeError, RkyvDeserializer};

pub mod config;
pub mod context;
pub mod offload;

pub use context::{BoxContext, CacheContext, CacheStatus, Context, ResponseSource};
pub mod policy;

pub mod predicate {
    pub use hitbox_core::{Predicate, PredicateResult};
}

pub mod extractor {
    pub use hitbox_core::Extractor;
}

/// The `hitbox` prelude.
pub mod prelude {
    pub use crate::{CacheError, CacheableRequest, CacheableResponse};
}
