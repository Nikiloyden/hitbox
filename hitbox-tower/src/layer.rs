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
pub const DEFAULT_CACHE_STATUS_HEADER: HeaderName = HeaderName::from_static("x-cache-status");

#[derive(Clone)]
pub struct Cache<B, C, CM, O = DisabledOffload> {
    pub backend: Arc<B>,
    pub configuration: C,
    pub offload: O,
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
    pub fn builder()
    -> CacheBuilder<MokaBackend, HttpEndpoint, NoopConcurrencyManager, DisabledOffload> {
        CacheBuilder::new()
    }
}

pub struct CacheBuilder<B, C, CM, O = DisabledOffload> {
    backend: Option<B>,
    configuration: C,
    offload: O,
    concurrency_manager: CM,
    cache_status_header: Option<HeaderName>,
}

impl CacheBuilder<MokaBackend, HttpEndpoint, NoopConcurrencyManager, DisabledOffload> {
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
    pub fn backend<NB: CacheBackend>(self, backend: NB) -> CacheBuilder<NB, C, CM, O> {
        CacheBuilder {
            backend: Some(backend),
            configuration: self.configuration,
            offload: self.offload,
            concurrency_manager: self.concurrency_manager,
            cache_status_header: self.cache_status_header,
        }
    }

    pub fn config<NC>(self, configuration: NC) -> CacheBuilder<B, NC, CM, O> {
        CacheBuilder {
            backend: self.backend,
            configuration,
            offload: self.offload,
            concurrency_manager: self.concurrency_manager,
            cache_status_header: self.cache_status_header,
        }
    }

    pub fn concurrency_manager<NCM>(self, concurrency_manager: NCM) -> CacheBuilder<B, C, NCM, O> {
        CacheBuilder {
            backend: self.backend,
            configuration: self.configuration,
            offload: self.offload,
            concurrency_manager,
            cache_status_header: self.cache_status_header,
        }
    }

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
    /// Defaults to `x-cache-status` if not set.
    pub fn cache_status_header(self, header_name: HeaderName) -> Self {
        CacheBuilder {
            backend: self.backend,
            configuration: self.configuration,
            offload: self.offload,
            concurrency_manager: self.concurrency_manager,
            cache_status_header: Some(header_name),
        }
    }

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
