use crate::EndpointConfig;
use std::sync::Arc;

use hitbox::backend::CacheBackend;
use hitbox::offload::OffloadManager;
use hitbox_moka::MokaBackend;
use tower::Layer;

use crate::service::CacheService;

#[derive(Clone)]
pub struct Cache<B, C> {
    pub backend: Arc<B>,
    pub configuration: C,
    pub offload_manager: Option<OffloadManager>,
}

impl<B, C> Cache<B, C>
where
    C: Default,
{
    pub fn new(backend: B) -> Cache<B, C> {
        Cache {
            backend: Arc::new(backend),
            configuration: Default::default(),
            offload_manager: None,
        }
    }
}

impl<S, B, C> Layer<S> for Cache<B, C>
where
    C: Clone,
{
    type Service = CacheService<S, B, C>;

    fn layer(&self, upstream: S) -> Self::Service {
        CacheService::new(
            upstream,
            Arc::clone(&self.backend),
            self.configuration.clone(),
            self.offload_manager.clone(),
        )
    }
}

impl Cache<MokaBackend, EndpointConfig> {
    pub fn builder() -> CacheBuilder<MokaBackend, EndpointConfig> {
        CacheBuilder::default()
    }
}

pub struct CacheBuilder<B, C> {
    backend: Option<B>,
    configuration: C,
    offload_manager: Option<OffloadManager>,
}

impl<B, C> CacheBuilder<B, C>
where
    B: CacheBackend,
    C: Default,
{
    pub fn backend<NB: CacheBackend>(self, backend: NB) -> CacheBuilder<NB, C> {
        CacheBuilder {
            backend: Some(backend),
            configuration: self.configuration,
            offload_manager: self.offload_manager,
        }
    }

    pub fn config<NC>(self, configuration: NC) -> CacheBuilder<B, NC> {
        CacheBuilder {
            backend: self.backend,
            configuration,
            offload_manager: self.offload_manager,
        }
    }

    pub fn offload_manager(self, manager: OffloadManager) -> Self {
        CacheBuilder {
            offload_manager: Some(manager),
            ..self
        }
    }

    pub fn build(self) -> Cache<B, C> {
        Cache {
            backend: Arc::new(self.backend.expect("Please add some cache backend")),
            configuration: self.configuration,
            offload_manager: self.offload_manager,
        }
    }
}

impl<B, C> Default for CacheBuilder<B, C>
where
    C: Default,
{
    fn default() -> Self {
        Self {
            backend: None,
            configuration: Default::default(),
            offload_manager: None,
        }
    }
}
