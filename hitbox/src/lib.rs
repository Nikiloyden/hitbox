//! # Hitbox
//!
//! Highly customizable async caching framework for Rust designed for high-performance applications.
//!
//! Hitbox is **protocol-agnostic** at its core, with first-class HTTP support via [`hitbox-http`].
//! It provides pluggable backends from in-memory ([Moka]) to distributed solutions ([Redis]).
//! Built on [Tower], it works with any tokio-based service.
//!
//! ## Quick Start
//!
//! If you need to add caching to your project, choose the appropriate integration based on your use case:
//!
//! | Use Case | Crate | Description |
//! |----------|-------|-------------|
//! | **Server-side** (Axum, Tower-based frameworks) | [`hitbox-tower`] | Tower middleware layer for HTTP handlers |
//! | **Client-side** (Hyper) | [`hitbox-tower`] | Tower middleware layer for hyper client |
//! | **Client-side** (Reqwest) | [`hitbox-reqwest`] | Cache responses from external APIs via reqwest-middleware |
//!
//! For detailed usage, see the documentation for these crates and the
//! [examples](https://github.com/hit-box/hitbox/tree/main/examples) directory.
//!
//! ## Understanding Hitbox
//!
//! ### The Hitbox Ecosystem
//!
//! Hitbox is organized as a collection of crates, each with a specific responsibility:
//!
//! | Crate | Description |
//! |-------|-------------|
//! | [`hitbox`] | Main crate — re-exports core types, policy configuration, error types |
//! | [`hitbox-core`] | Protocol-agnostic core traits ([`Predicate`], [`Extractor`], [`CacheableRequest`], [`CacheableResponse`]) |
//! | [`hitbox-backend`] | [`Backend`] trait and utilities for implementing storage backends |
//! | [`hitbox-http`] | HTTP-specific predicates and extractors for request/response caching |
//! | [`hitbox-tower`] | Tower middleware (`Cache` layer) for server-side caching |
//! | [`hitbox-moka`] | In-memory backend using [Moka] |
//! | [`hitbox-redis`] | Distributed backend using [Redis] (single node and cluster) |
//! | [`hitbox-feoxdb`] | Embedded persistent backend using FeOxDB |
//! | [`hitbox-reqwest`] | Client-side caching for Reqwest via reqwest-middleware |
//! | [`hitbox-test`] | Testing utilities including `MockTime` for deterministic cache tests |
//!
//! ### Core Principles
//!
//! Under the hood, Hitbox uses a **Finite State Machine (FSM)** to orchestrate cache operations.
//! The FSM operates with four abstract traits that make Hitbox extensible to any protocol:
//!
//! #### Backend
//!
//! A **Backend** is a storage where cached data lives. It can be:
//!
//! - **In-memory** — like [Moka] for single-instance, high-speed caching
//! - **Distributed** — like [Redis] (single node or cluster) for shared caching across instances
//! - **Embedded** — like [FeOxDB] for persistent local storage
//!
//! If you need a backend for a database that isn't in our list, you can add it by implementing
//! the [`Backend`] trait. See the [`hitbox-backend`] documentation for details.
//!
//! #### Upstream
//!
//! An [`Upstream`] is a source of data. It could be:
//!
//! - An HTTP handler in a web framework
//! - An external API call via Reqwest or Hyper
//! - Any async function that produces data
//!
//! The FSM calls the Upstream when the cache misses (data not found in the Backend).
//!
//! #### Predicate
//!
//! A **Predicate** answers the question: *"Should this request/response be cached?"*
//!
//! The easiest way to explain predicates is through HTTP examples:
//!
//! **Request predicates** — We might want to cache requests that:
//! - Have a specific HTTP method (e.g., only GET requests)
//! - Match a particular path pattern
//! - Do NOT contain a `Cache-Control: no-cache` header
//! - Do NOT have a `cache=false` query parameter
//!
//! **Response predicates** — We might want to cache responses that:
//! - Have a successful status code (2xx)
//! - Do NOT contain sensitive headers
//! - Have a body smaller than a certain size
//!
//! Predicates implement the [`Predicate`] trait, which takes a request or response and returns
//! `Cacheable` or `NonCacheable`. They can be combined using AND (chaining), OR, and NOT logic.
//!
//! #### Extractor
//!
//! An [`Extractor`] creates the cache key from request components.
//!
//! For HTTP, you might extract:
//! - **Method and path** as the base key
//! - **Path parameters** like `{user_id}` from `/api/users/{user_id}`
//! - **Query parameters** like `page` or `limit` that affect the response
//! - **Headers** like `Accept-Language` for localized content
//!
//! Multiple extractors can be chained together, each contributing parts to the final cache key.
//!
//! **Example:** For a request to `GET /api/users/123/posts?page=2` with `Accept-Language: en`,
//! an extractor configured for method, path params, query, and headers would produce a key
//! containing: `["GET", "123", "2", "en"]`.
//!
//! [`hitbox`]: https://docs.rs/hitbox
//! [`hitbox-core`]: https://docs.rs/hitbox-core
//! [`hitbox-backend`]: https://docs.rs/hitbox-backend
//! [`hitbox-http`]: https://docs.rs/hitbox-http
//! [`hitbox-tower`]: https://docs.rs/hitbox-tower
//! [`hitbox-moka`]: https://docs.rs/hitbox-moka
//! [`hitbox-redis`]: https://docs.rs/hitbox-redis
//! [`hitbox-feoxdb`]: https://docs.rs/hitbox-feoxdb
//! [`hitbox-reqwest`]: https://docs.rs/hitbox-reqwest
//! [`hitbox-test`]: https://docs.rs/hitbox-test
//! [`Backend`]: hitbox_backend::Backend
//! [`Upstream`]: hitbox_core::Upstream
//! [`Extractor`]: crate::Extractor
//! [Moka]: https://github.com/moka-rs/moka
//! [Redis]: https://redis.io/
//! [FeOxDB]: https://github.com/nicholasVilela/feoxdb
//! [Tower]: https://docs.rs/tower

#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

/// Backend-related re-exports and utilities.
///
/// This module provides access to the [`Backend`](hitbox_backend::Backend) trait
/// and related types for implementing custom storage backends.
pub mod backend;

/// Dogpile prevention via concurrency management.
///
/// When a cache entry expires, multiple simultaneous requests can trigger redundant
/// upstream calls — the "thundering herd" problem. This module provides
/// [`BroadcastConcurrencyManager`](concurrency::BroadcastConcurrencyManager) to prevent this
/// by allowing only N requests to proceed while others wait for the result.
pub mod concurrency;

/// Error types for cache operations.
///
/// Defines [`CacheError`] which covers:
/// - Backend errors (storage failures)
/// - Upstream errors (data source failures)
/// - Cache key generation failures
pub mod error;

/// Finite State Machine for cache orchestration.
///
/// The FSM coordinates cache lookups, upstream calls, and response handling
/// based on cache state (hit, miss, stale) and configured policies.
pub mod fsm;

/// Metrics collection for cache observability.
///
/// When the `metrics` feature is enabled, this module provides counters
/// and histograms for:
/// - Cache hits, misses, and stale responses
/// - Request latency and upstream call timing
/// - Backend read/write operations
pub mod metrics;

pub use error::CacheError;

pub use hitbox_core::{
    And, BackendLabel, CacheKey, CachePolicy, CacheState, CacheValue, CacheablePolicyData,
    CacheableRequest, CacheableResponse, EntityPolicyConfig, Extractor, KeyPart, KeyParts, Neutral,
    Not, Or, Predicate, PredicateExt, Raw, RequestCachePolicy, ResponseCachePolicy,
};

/// Cache configuration types.
///
/// Provides types for configuring cache behavior including TTL, stale windows,
/// and endpoint-specific settings.
pub mod config;

/// Cache context and status types.
///
/// This module provides:
/// - [`CacheContext`](context::CacheContext) — metadata passed through the request lifecycle
/// - [`CacheStatus`](context::CacheStatus) — indicates whether a response came from cache
/// - [`ResponseSource`](context::ResponseSource) — identifies where the response originated
pub mod context;

/// Background task offloading for stale-while-revalidate.
///
/// When using the `OffloadRevalidate` stale policy, expired cache entries are served
/// immediately while fresh data is fetched in the background. This module provides
/// the [`OffloadManager`](offload::OffloadManager) for handling these background tasks.
pub mod offload;

pub use config::{CacheConfig, Config, ConfigBuilder, NotSet};
pub use context::{BoxContext, CacheContext, CacheStatus, CacheStatusExt, Context, ResponseSource};

/// Policy configuration for cache behavior.
///
/// Defines [`PolicyConfig`](policy::PolicyConfig) with:
/// - **TTL** — how long cached data remains fresh
/// - **Stale window** — grace period after TTL where stale data can be served
/// - **Stale policy** — how to handle stale data (`Return`, `Revalidate`, `OffloadRevalidate`)
/// - **Concurrency** — limit for dogpile prevention
pub mod policy;

/// Predicate trait and combinators for cache decisions.
///
/// Re-exports from [`hitbox-core`](https://docs.rs/hitbox-core) including:
/// - [`Predicate`] trait — determines if a request/response should be cached
/// - [`And`], [`Or`], [`Not`] — logical combinators for composing predicates
/// - [`Neutral`] — a predicate that always returns `Cacheable`
pub mod predicate {
    pub use hitbox_core::predicate::{
        And, Neutral, Not, Or, Predicate, PredicateExt, PredicateResult, combinators, neutral,
    };
}

/// Extractor trait for cache key generation.
///
/// Re-exports the [`Extractor`] trait from [`hitbox-core`](https://docs.rs/hitbox-core).
/// Extractors pull components from requests to build unique cache keys.
pub mod extractor {
    pub use hitbox_core::Extractor;
}

/// The `hitbox` prelude.
///
/// Provides convenient access to the most commonly used types:
///
/// ```rust
/// use hitbox::prelude::*;
/// ```
///
/// This imports:
/// - [`CacheError`] — error type for cache operations
/// - [`CacheableRequest`] — trait for cacheable request types
/// - [`CacheableResponse`] — trait for cacheable response types
pub mod prelude {
    pub use crate::{CacheError, CacheableRequest, CacheableResponse};
}
