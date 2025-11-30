use std::sync::Arc;

use hitbox::backend::CacheBackend;
use hitbox::concurrency::NoopConcurrencyManager;
use hitbox::offload::OffloadManager;
use hitbox_moka::MokaBackend;
use tower::Layer;

use crate::service::CacheService;

#[derive(Clone)]
pub struct Cache<B, C, CM> {
    pub backend: Arc<B>,
    pub configuration: C,
    pub offload_manager: Option<OffloadManager>,
    pub concurrency_manager: CM,
}

impl<B, C, CM> Cache<B, C, CM>
where
    C: Default,
{
    pub fn new(backend: B) -> Cache<B, C, NoopConcurrencyManager> {
        Cache {
            backend: Arc::new(backend),
            configuration: Default::default(),
            offload_manager: None,
            concurrency_manager: NoopConcurrencyManager,
        }
    }
}

impl<S, B, C, CM> Layer<S> for Cache<B, C, CM>
where
    C: Clone,
    CM: Clone,
{
    type Service = CacheService<S, B, C, CM>;

    fn layer(&self, upstream: S) -> Self::Service {
        CacheService::new(
            upstream,
            Arc::clone(&self.backend),
            self.configuration.clone(),
            self.offload_manager.clone(),
            self.concurrency_manager.clone(),
        )
    }
}

impl Cache<MokaBackend, (), NoopConcurrencyManager> {
    pub fn builder() -> CacheBuilder<MokaBackend, (), NoopConcurrencyManager> {
        CacheBuilder::new()
    }
}

pub struct CacheBuilder<B, C, CM> {
    backend: Option<B>,
    configuration: Option<C>,
    offload_manager: Option<OffloadManager>,
    concurrency_manager: CM,
}

impl CacheBuilder<MokaBackend, (), NoopConcurrencyManager> {
    pub fn new() -> Self {
        Self {
            backend: None,
            configuration: None,
            offload_manager: None,
            concurrency_manager: NoopConcurrencyManager,
        }
    }
}

impl Default for CacheBuilder<MokaBackend, (), NoopConcurrencyManager> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B, C, CM> CacheBuilder<B, C, CM>
where
    B: CacheBackend,
{
    pub fn backend<NB: CacheBackend>(self, backend: NB) -> CacheBuilder<NB, C, CM> {
        CacheBuilder {
            backend: Some(backend),
            configuration: self.configuration,
            offload_manager: self.offload_manager,
            concurrency_manager: self.concurrency_manager,
        }
    }

    pub fn config<NC>(self, configuration: NC) -> CacheBuilder<B, NC, CM> {
        CacheBuilder {
            backend: self.backend,
            configuration: Some(configuration),
            offload_manager: self.offload_manager,
            concurrency_manager: self.concurrency_manager,
        }
    }

    pub fn concurrency_manager<NCM>(self, concurrency_manager: NCM) -> CacheBuilder<B, C, NCM> {
        CacheBuilder {
            backend: self.backend,
            configuration: self.configuration,
            offload_manager: self.offload_manager,
            concurrency_manager,
        }
    }

    pub fn offload_manager(self, manager: OffloadManager) -> Self {
        CacheBuilder {
            offload_manager: Some(manager),
            ..self
        }
    }

    pub fn build(self) -> Cache<B, C, CM> {
        Cache {
            backend: Arc::new(self.backend.expect("Please add a cache backend")),
            configuration: self.configuration.expect("Please add a configuration"),
            offload_manager: self.offload_manager,
            concurrency_manager: self.concurrency_manager,
        }
    }
}
