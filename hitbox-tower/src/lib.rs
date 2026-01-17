//! Tower middleware integration for the Hitbox caching framework.
//!
//! This crate provides [`Cache`], a Tower [`Layer`] that adds transparent HTTP caching
//! to any Tower service. It evaluates requests against predicates, generates cache keys
//! using extractors, and stores/retrieves responses through pluggable backends.
//!
//! # When to Use This Crate
//!
//! Use `hitbox-tower` when you have a Tower-based HTTP server (e.g., Axum, Hyper)
//! and want to add caching as middleware.
//!
//! # Core Concepts
//!
//! - **[`Cache`]**: A Tower [`Layer`] that wraps services with caching behavior.
//!   Use [`Cache::builder()`] to configure and construct the layer.
//!
//! - **[Predicate]**: Determines if a request/response should be cached.
//!   Returns `Cacheable` or `NonCacheable`. See [`hitbox_http::predicates`].
//!
//! - **[Extractor]**: Generates cache key parts from HTTP components (method, path,
//!   headers, query etc.). See [`hitbox_http::extractors`].
//!
//! - **[Backend]**: Storage for cached responses. Use [`hitbox_moka`] for in-memory,
//!   or other backends for distributed caching.
//!
//! - **[Policy]**: Controls TTL, stale-while-revalidate, and other timing behavior.
//!
//! [Predicate]: hitbox_core::Predicate
//! [Extractor]: hitbox_core::Extractor
//! [Backend]: hitbox::backend::CacheBackend
//! [Policy]: hitbox::policy::PolicyConfig
//! [`Layer`]: tower::Layer
//! [`hitbox_moka`]: https://docs.rs/hitbox-moka
//!
//! # Quick Start
//!
//! ## Minimal Setup (Default Configuration)
//!
//! The simplest way to add caching uses [`HttpEndpoint`] defaults, which cache
//! all requests with a 5-second TTL:
//!
//! ```
//! use hitbox_tower::Cache;
//! use hitbox_moka::MokaBackend;
//! use tower::{ServiceBuilder, service_fn};
//!
//! # use std::convert::Infallible;
//! # use bytes::Bytes;
//! # use http_body_util::Full;
//! // Create in-memory cache with 1000 entry capacity
//! let backend = MokaBackend::builder().max_entries(1000).build();
//!
//! // Build the cache layer with defaults
//! let cache_layer = Cache::builder()
//!     .backend(backend)
//!     .build();
//!
//! // Apply to a Tower service
//! let service = ServiceBuilder::new()
//!     .layer(cache_layer)
//!     .service(service_fn(|_req: http::Request<Full<Bytes>>| async {
//!         Ok::<_, Infallible>(http::Response::new(Full::new(Bytes::from("Hello"))))
//!     }));
//! ```
//!
//! ## Custom Configuration
//!
//! For production use, configure predicates, extractors, and TTL:
//!
//! ```
//! use std::time::Duration;
//! use hitbox_tower::Cache;
//! use hitbox_moka::MokaBackend;
//! use hitbox_configuration::Endpoint;
//! use hitbox::policy::PolicyConfig;
//! use hitbox_http::{
//!     extractors::{Method as MethodExtractor, path::PathExtractor},
//!     predicates::request::Method,
//! };
//!
//! # use bytes::Bytes;
//! # use http_body_util::Empty;
//! // 1. Create backend
//! let backend = MokaBackend::builder().max_entries(10_000).build();
//!
//! // 2. Configure caching behavior
//! let config = Endpoint::builder()
//!     // Only cache GET requests
//!     .request_predicate(Method::new(http::Method::GET).unwrap())
//!     // Build cache key from method and path
//!     .extractor(MethodExtractor::new().path("/{path}*"))
//!     // Cache for 5 minutes
//!     .policy(PolicyConfig::builder().ttl(Duration::from_secs(300)).build())
//!     .build();
//! # let _: Endpoint<Empty<Bytes>, Empty<Bytes>> = config;
//!
//! // 3. Build the cache layer
//! let cache_layer = Cache::builder()
//!     .backend(backend)
//!     .config(config)
//!     .build();
//!
//! // 4. Apply to a Tower service
//! # use std::convert::Infallible;
//! use tower::{ServiceBuilder, service_fn};
//! let service = ServiceBuilder::new()
//!     .layer(cache_layer)
//!     .service(service_fn(|_req: http::Request<Empty<Bytes>>| async {
//!         Ok::<_, Infallible>(http::Response::new(Empty::<Bytes>::new()))
//!     }));
//! ```
//!
//! # Response Headers
//!
//! The middleware adds a cache status header to every response:
//!
//! | Header Value | Meaning |
//! |--------------|---------|
//! | `HIT` | Response served from cache |
//! | `MISS` | Response fetched from upstream (may be cached for future requests) |
//! | `STALE` | Stale cache entry served (background refresh may occur) |
//!
//! The default header name is `x-cache-status`. Customize it with
//! [`CacheBuilder::cache_status_header`]:
//!
//! ```
//! use hitbox_tower::Cache;
//! use hitbox_moka::MokaBackend;
//! use http::header::HeaderName;
//!
//! let cache_layer = Cache::builder()
//!     .backend(MokaBackend::builder().max_entries(1000).build())
//!     .cache_status_header(HeaderName::from_static("x-custom-cache"))
//!     .build();
//! ```
//!
//! # Main Types
//!
//! | Type | Description |
//! |------|-------------|
//! | [`Cache`] | Tower `Layer` — the main entry point |
//! | [`CacheBuilder`] | Fluent builder for configuring the cache layer |
//! | [`service::CacheService`] | The Tower `Service` that performs caching |
//! | [`TowerUpstream`] | Adapter bridging Tower services to Hitbox's upstream interface |
//! | [`HttpEndpoint`] | Default configuration (caches everything) |
//! | [`Endpoint`] | Custom configuration with predicates and extractors |
//!
//! # Re-exports
//!
//! This crate re-exports commonly used types for convenience:
//!
//! - [`http::Method`], [`http::StatusCode`] — HTTP types for predicates
//! - [`CacheConfig`] — Trait for cache configuration
//! - [`Endpoint`], [`ConfigEndpoint`] — Configuration types from `hitbox-configuration`
//! - [`HttpEndpoint`] — Default HTTP configuration from `hitbox-http`
//!
//! For predicates and extractors, import from [`hitbox_http`]:
//!
//! ```
//! use hitbox_http::predicates::request::Method;
//! use hitbox_http::predicates::response::StatusCode;
//! use hitbox_http::extractors::{Method as MethodExtractor, path::PathExtractor};
//! ```
//!
//! # Examples
//!
//! For complete, runnable examples see the `examples/` directory:
//!
//! - **[tower.rs]** — Plain Tower service with Hyper server
//! - **[axum.rs]** — Axum web framework with per-route caching
//!
//! Run with:
//! ```text
//! cargo run -p hitbox-examples --example tower
//! cargo run -p hitbox-examples --example axum
//! ```
//!
//! [tower.rs]: https://github.com/hit-box/hitbox/blob/main/examples/examples/tower.rs
//! [axum.rs]: https://github.com/hit-box/hitbox/blob/main/examples/examples/axum.rs
//!
//! # Feature Flags
//!
//! This crate has no feature flags. Backend selection is done by depending on
//! the appropriate backend crate (e.g., `hitbox-moka`, `hitbox-redis`).

#![warn(missing_docs)]

/// Future types for the cache service.
pub mod future;
/// Tower layer and builder for cache configuration.
pub mod layer;
/// The Tower service implementation that performs caching.
pub mod service;
/// Upstream adapter for bridging Tower services to Hitbox.
pub mod upstream;

pub use ::http::{Method, StatusCode};
pub use hitbox::config::CacheConfig;
pub use hitbox_configuration::{ConfigEndpoint, Endpoint};
pub use hitbox_http::HttpEndpoint;
pub use layer::{Cache, CacheBuilder, DEFAULT_CACHE_STATUS_HEADER};
pub use upstream::TowerUpstream;
