//! Tower layer and builder for HTTP caching.
//!
//! This module provides [`Cache`], a Tower [`Layer`] that wraps services with
//! caching behavior, and [`CacheBuilder`] for fluent configuration.
//!
//! # Examples
//!
//! ```
//! use hitbox_tower::Cache;
//! use hitbox_moka::MokaBackend;
//!
//! let cache_layer = Cache::builder()
//!     .backend(MokaBackend::builder(1000).build())
//!     .build();
//! ```
//!
//! [`Layer`]: tower::Layer

use std::sync::Arc;

use hitbox::backend::CacheBackend;
use hitbox::concurrency::NoopConcurrencyManager;
use hitbox_core::DisabledOffload;
use hitbox_http::HttpEndpoint;
use hitbox_moka::MokaBackend;
use http::header::HeaderName;
use tower::Layer;

use crate::service::CacheService;

/// Default header name for cache status (HIT/MISS/STALE).
///
/// The value is `x-cache-status`. Use [`CacheBuilder::cache_status_header`] to customize.
pub const DEFAULT_CACHE_STATUS_HEADER: HeaderName = HeaderName::from_static("x-cache-status");

/// Tower [`Layer`] that adds HTTP caching to a service.
///
/// `Cache` wraps any Tower service with caching behavior. When a request arrives,
/// the layer evaluates predicates to determine cacheability, generates a cache key
/// using extractors, and either returns a cached response or forwards to the
/// upstream service.
///
/// # Type Parameters
///
/// * `B` - Cache backend (e.g., [`MokaBackend`], `RedisBackend`). Must implement
///   [`CacheBackend`].
/// * `C` - Configuration providing predicates, extractors, and policy. Use
///   [`HttpEndpoint`] for defaults or [`Endpoint`] for custom configuration.
/// * `CM` - Concurrency manager for dogpile prevention. Use [`NoopConcurrencyManager`]
///   to disable or [`BroadcastConcurrencyManager`] to enable.
/// * `O` - Offload strategy for background revalidation. Use [`DisabledOffload`]
///   for synchronous behavior.
///
/// # Examples
///
/// Create with the builder pattern:
///
/// ```
/// use hitbox_tower::Cache;
/// use hitbox_moka::MokaBackend;
///
/// let cache_layer = Cache::builder()
///     .backend(MokaBackend::builder(1000).build())
///     .build();
/// ```
///
/// [`Layer`]: tower::Layer
/// [`MokaBackend`]: hitbox_moka::MokaBackend
/// [`CacheBackend`]: hitbox::backend::CacheBackend
/// [`HttpEndpoint`]: hitbox_http::HttpEndpoint
/// [`Endpoint`]: hitbox_configuration::Endpoint
/// [`NoopConcurrencyManager`]: hitbox::concurrency::NoopConcurrencyManager
/// [`BroadcastConcurrencyManager`]: hitbox::concurrency::BroadcastConcurrencyManager
/// [`DisabledOffload`]: hitbox_core::DisabledOffload
#[derive(Clone)]
pub struct Cache<B, C, CM, O = DisabledOffload> {
    /// The cache backend for storing and retrieving responses.
    pub backend: Arc<B>,
    /// Configuration with predicates, extractors, and cache policy.
    pub configuration: C,
    /// Offload strategy for background tasks.
    pub offload: O,
    /// Concurrency manager for dogpile prevention.
    pub concurrency_manager: CM,
    /// Header name for cache status (HIT/MISS/STALE).
    pub cache_status_header: HeaderName,
}

impl<S, B, C, CM, O> Layer<S> for Cache<B, C, CM, O>
where
    C: Clone,
    CM: Clone,
    O: Clone,
{
    type Service = CacheService<S, B, C, CM, O>;

    fn layer(&self, upstream: S) -> Self::Service {
        CacheService::new(
            upstream,
            Arc::clone(&self.backend),
            self.configuration.clone(),
            self.offload.clone(),
            self.concurrency_manager.clone(),
            self.cache_status_header.clone(),
        )
    }
}

impl Cache<MokaBackend, HttpEndpoint, NoopConcurrencyManager, DisabledOffload> {
    /// Creates a new [`CacheBuilder`] with default configuration.
    ///
    /// The builder starts with:
    /// - No backend (must be set before calling [`build()`](CacheBuilder::build))
    /// - [`HttpEndpoint`] configuration (caches all requests)
    /// - [`NoopConcurrencyManager`] (no dogpile prevention)
    /// - [`DisabledOffload`] (synchronous revalidation)
    /// - `x-cache-status` header name
    ///
    /// # Examples
    ///
    /// ```
    /// use hitbox_tower::Cache;
    /// use hitbox_moka::MokaBackend;
    ///
    /// let cache_layer = Cache::builder()
    ///     .backend(MokaBackend::builder(1000).build())
    ///     .build();
    /// ```
    pub fn builder()
    -> CacheBuilder<MokaBackend, HttpEndpoint, NoopConcurrencyManager, DisabledOffload> {
        CacheBuilder::new()
    }
}

/// Fluent builder for constructing a [`Cache`] layer.
///
/// Use [`Cache::builder()`] to create a new builder. The only required method
/// is [`backend()`](Self::backend) — all other settings have sensible defaults.
///
/// # Type Parameters
///
/// The type parameters change as you call builder methods:
///
/// * `B` - Backend type, set by [`backend()`](Self::backend)
/// * `C` - Configuration type, set by [`config()`](Self::config)
/// * `CM` - Concurrency manager type, set by [`concurrency_manager()`](Self::concurrency_manager)
/// * `O` - Offload type, set by [`offload()`](Self::offload)
///
/// # Examples
///
/// Minimal configuration:
///
/// ```
/// use hitbox_tower::Cache;
/// use hitbox_moka::MokaBackend;
///
/// let layer = Cache::builder()
///     .backend(MokaBackend::builder(1000).build())
///     .build();
/// ```
///
/// Full configuration:
///
/// ```
/// use std::time::Duration;
/// use hitbox_tower::Cache;
/// use hitbox_moka::MokaBackend;
/// use hitbox_configuration::Endpoint;
/// use hitbox::policy::PolicyConfig;
/// use hitbox_http::{
///     extractors::{Method as MethodExtractor, path::PathExtractor},
///     predicates::request::Method,
/// };
/// use http::header::HeaderName;
///
/// # use bytes::Bytes;
/// # use http_body_util::Empty;
/// let config = Endpoint::builder()
///     .request_predicate(Method::new(http::Method::GET).unwrap())
///     .extractor(MethodExtractor::new().path("/{path}*"))
///     .policy(PolicyConfig::builder().ttl(Duration::from_secs(300)).build())
///     .build();
/// # let _: Endpoint<Empty<Bytes>, Empty<Bytes>> = config;
///
/// let layer = Cache::builder()
///     .backend(MokaBackend::builder(10_000).build())
///     .config(config)
///     .cache_status_header(HeaderName::from_static("x-custom-cache"))
///     .build();
/// ```
///
/// # Panics
///
/// [`build()`](Self::build) panics if no backend was set.
pub struct CacheBuilder<B, C, CM, O = DisabledOffload> {
    backend: Option<B>,
    configuration: C,
    offload: O,
    concurrency_manager: CM,
    cache_status_header: Option<HeaderName>,
}

impl CacheBuilder<MokaBackend, HttpEndpoint, NoopConcurrencyManager, DisabledOffload> {
    /// Creates a new builder with default settings.
    ///
    /// Prefer using [`Cache::builder()`] instead of calling this directly.
    pub fn new() -> Self {
        Self {
            backend: None,
            configuration: HttpEndpoint::default(),
            offload: DisabledOffload,
            concurrency_manager: NoopConcurrencyManager,
            cache_status_header: None,
        }
    }
}

impl Default for CacheBuilder<MokaBackend, HttpEndpoint, NoopConcurrencyManager, DisabledOffload> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B, C, CM, O> CacheBuilder<B, C, CM, O>
where
    B: CacheBackend,
{
    /// Sets the cache backend for storing responses.
    ///
    /// This is the only required builder method. Common backends:
    ///
    /// - [`MokaBackend`] — In-memory cache
    /// - `RedisBackend` — Distributed cache via Redis
    ///
    /// # Examples
    ///
    /// ```
    /// use hitbox_tower::Cache;
    /// use hitbox_moka::MokaBackend;
    ///
    /// let layer = Cache::builder()
    ///     .backend(MokaBackend::builder(1000).build())
    ///     .build();
    /// ```
    pub fn backend<NB: CacheBackend>(self, backend: NB) -> CacheBuilder<NB, C, CM, O> {
        CacheBuilder {
            backend: Some(backend),
            configuration: self.configuration,
            offload: self.offload,
            concurrency_manager: self.concurrency_manager,
            cache_status_header: self.cache_status_header,
        }
    }

    /// Sets the cache configuration with predicates, extractors, and policy.
    ///
    /// Use [`Endpoint::builder()`] to create a custom configuration with:
    /// - Request predicates (which requests to cache)
    /// - Response predicates (which responses to cache)
    /// - Extractors (how to generate cache keys)
    /// - Policy (TTL, stale handling)
    ///
    /// Defaults to [`HttpEndpoint`] which caches all requests with a 5-second TTL.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use hitbox_tower::Cache;
    /// use hitbox_moka::MokaBackend;
    /// use hitbox_configuration::Endpoint;
    /// use hitbox::policy::PolicyConfig;
    /// use hitbox_http::predicates::request::Method;
    ///
    /// # use bytes::Bytes;
    /// # use http_body_util::Empty;
    /// let config = Endpoint::builder()
    ///     .request_predicate(Method::new(http::Method::GET).unwrap())
    ///     .policy(PolicyConfig::builder().ttl(Duration::from_secs(60)).build())
    ///     .build();
    /// # let _: Endpoint<Empty<Bytes>, Empty<Bytes>> = config;
    ///
    /// let layer = Cache::builder()
    ///     .backend(MokaBackend::builder(1000).build())
    ///     .config(config)
    ///     .build();
    /// ```
    ///
    /// [`Endpoint::builder()`]: hitbox_configuration::Endpoint::builder
    /// [`HttpEndpoint`]: hitbox_http::HttpEndpoint
    pub fn config<NC>(self, configuration: NC) -> CacheBuilder<B, NC, CM, O> {
        CacheBuilder {
            backend: self.backend,
            configuration,
            offload: self.offload,
            concurrency_manager: self.concurrency_manager,
            cache_status_header: self.cache_status_header,
        }
    }

    /// Sets the concurrency manager for dogpile prevention.
    ///
    /// The dogpile effect occurs when a cache entry expires and multiple
    /// concurrent requests all try to refresh it simultaneously. A concurrency
    /// manager prevents this by coordinating requests.
    ///
    /// Options:
    /// - [`NoopConcurrencyManager`] — No coordination (default)
    /// - [`BroadcastConcurrencyManager`] — One request fetches, others wait
    ///
    /// [`NoopConcurrencyManager`]: hitbox::concurrency::NoopConcurrencyManager
    /// [`BroadcastConcurrencyManager`]: hitbox::concurrency::BroadcastConcurrencyManager
    pub fn concurrency_manager<NCM>(self, concurrency_manager: NCM) -> CacheBuilder<B, C, NCM, O> {
        CacheBuilder {
            backend: self.backend,
            configuration: self.configuration,
            offload: self.offload,
            concurrency_manager,
            cache_status_header: self.cache_status_header,
        }
    }

    /// Sets the offload strategy for background revalidation.
    ///
    /// When serving stale content, the offload strategy determines how
    /// background refresh is performed.
    ///
    /// Defaults to [`DisabledOffload`] (synchronous revalidation).
    ///
    /// [`DisabledOffload`]: hitbox_core::DisabledOffload
    pub fn offload<NO>(self, offload: NO) -> CacheBuilder<B, C, CM, NO> {
        CacheBuilder {
            backend: self.backend,
            configuration: self.configuration,
            offload,
            concurrency_manager: self.concurrency_manager,
            cache_status_header: self.cache_status_header,
        }
    }

    /// Sets the header name for cache status.
    ///
    /// The cache status header indicates whether a response was served from cache.
    /// Possible values are `HIT`, `MISS`, or `STALE`.
    ///
    /// Defaults to [`DEFAULT_CACHE_STATUS_HEADER`] (`x-cache-status`).
    ///
    /// # Examples
    ///
    /// ```
    /// use hitbox_tower::Cache;
    /// use hitbox_moka::MokaBackend;
    /// use http::header::HeaderName;
    ///
    /// let layer = Cache::builder()
    ///     .backend(MokaBackend::builder(1000).build())
    ///     .cache_status_header(HeaderName::from_static("x-custom-cache"))
    ///     .build();
    /// ```
    pub fn cache_status_header(self, header_name: HeaderName) -> Self {
        CacheBuilder {
            backend: self.backend,
            configuration: self.configuration,
            offload: self.offload,
            concurrency_manager: self.concurrency_manager,
            cache_status_header: Some(header_name),
        }
    }

    /// Builds the [`Cache`] layer.
    ///
    /// # Panics
    ///
    /// Panics if [`backend()`](Self::backend) was not called.
    pub fn build(self) -> Cache<B, C, CM, O> {
        Cache {
            backend: Arc::new(self.backend.expect("Please add a cache backend")),
            configuration: self.configuration,
            offload: self.offload,
            concurrency_manager: self.concurrency_manager,
            cache_status_header: self
                .cache_status_header
                .unwrap_or(DEFAULT_CACHE_STATUS_HEADER),
        }
    }
}
