use hitbox::concurrency::ConcurrencyManager;
use hitbox::config::CacheConfig;
use hitbox::offload::OffloadManager;
use std::{fmt::Debug, sync::Arc};

use hitbox::{backend::CacheBackend, fsm::CacheFuture};
use hitbox_http::{BufferedBody, CacheableHttpRequest, CacheableHttpResponse};
use http::{Request, Response};
use hyper::body::Body as HttpBody;
use tower::Service;

use crate::future::CacheServiceFuture;
use crate::upstream::TowerUpstream;

pub struct CacheService<S, B, C, CM> {
    upstream: S,
    backend: Arc<B>,
    configuration: C,
    offload_manager: Option<OffloadManager>,
    concurrency_manager: CM,
}

impl<S, B, C, CM> CacheService<S, B, C, CM> {
    pub fn new(
        upstream: S,
        backend: Arc<B>,
        configuration: C,
        offload_manager: Option<OffloadManager>,
        concurrency_manager: CM,
    ) -> Self {
        CacheService {
            upstream,
            backend,
            configuration,
            offload_manager,
            concurrency_manager,
        }
    }
}

impl<S, B, C, CM> Clone for CacheService<S, B, C, CM>
where
    S: Clone,
    B: Clone,
    C: Clone,
    CM: Clone,
{
    fn clone(&self) -> Self {
        Self {
            upstream: self.upstream.clone(),
            backend: self.backend.clone(),
            configuration: self.configuration.clone(),
            offload_manager: self.offload_manager.clone(),
            concurrency_manager: self.concurrency_manager.clone(),
        }
    }
}

impl<S, B, C, CM, ReqBody, ResBody> Service<Request<ReqBody>> for CacheService<S, B, C, CM>
where
    S: Service<Request<BufferedBody<ReqBody>>, Response = Response<ResBody>>
        + Clone
        + Send
        + 'static,
    B: CacheBackend + Clone + Send + Sync + 'static,
    S::Future: Send,
    C: CacheConfig<CacheableHttpRequest<ReqBody>, CacheableHttpResponse<ResBody>>,
    CM: ConcurrencyManager<Result<CacheableHttpResponse<ResBody>, S::Error>> + Clone + 'static,
    // debug bounds
    ReqBody: Debug + HttpBody + Send + 'static,
    ReqBody::Error: Send,
    // Body: From<ReqBody>,
    ResBody: HttpBody + Send + 'static,
    ResBody::Error: Debug + Send,
    ResBody::Data: Send,
    S::Error: Debug + Send,
{
    type Response = Response<BufferedBody<ResBody>>;
    type Error = S::Error;
    type Future = CacheServiceFuture<
        CacheFuture<
            B,
            CacheableHttpRequest<ReqBody>,
            Result<CacheableHttpResponse<ResBody>, S::Error>,
            TowerUpstream<S, ReqBody, ResBody>,
            C::RequestPredicate,
            C::ResponsePredicate,
            C::Extractor,
            CM,
        >,
        ResBody,
        S::Error,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.upstream.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let configuration = &self.configuration;

        // Convert incoming Request<ReqBody> to CacheableHttpRequest<ReqBody>
        let (parts, body) = req.into_parts();
        let buffered_request = Request::from_parts(parts, BufferedBody::Passthrough(body));
        let cacheable_req = CacheableHttpRequest::from_request(buffered_request);

        // Create upstream adapter that handles Tower service calls
        let upstream = TowerUpstream::new(self.upstream.clone());

        // Create CacheFuture with cacheable types only
        let cache_future = CacheFuture::new(
            self.backend.clone(),
            cacheable_req,
            upstream,
            configuration.request_predicates(),
            configuration.response_predicates(),
            configuration.extractors(),
            Arc::new(configuration.policy().clone()),
            self.offload_manager.clone(),
            self.concurrency_manager.clone(),
        );

        // Wrap in CacheServiceFuture to add cache headers
        CacheServiceFuture::new(cache_future)
    }
}
