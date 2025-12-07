use hitbox::concurrency::ConcurrencyManager;
use hitbox::config::CacheConfig;
use hitbox_core::{DisabledOffload, Offload};
use std::{fmt::Debug, sync::Arc};

use hitbox::{backend::CacheBackend, fsm::CacheFuture};
use hitbox_http::{BufferedBody, CacheableHttpRequest, CacheableHttpResponse};
use http::{Request, Response};
use hyper::body::Body as HttpBody;
use tower::Service;

use crate::future::CacheServiceFuture;
use crate::upstream::TowerUpstream;

pub struct CacheService<S, B, C, CM, O = DisabledOffload> {
    upstream: S,
    backend: Arc<B>,
    configuration: C,
    offload: O,
    concurrency_manager: CM,
}

impl<S, B, C, CM, O> CacheService<S, B, C, CM, O> {
    pub fn new(
        upstream: S,
        backend: Arc<B>,
        configuration: C,
        offload: O,
        concurrency_manager: CM,
    ) -> Self {
        CacheService {
            upstream,
            backend,
            configuration,
            offload,
            concurrency_manager,
        }
    }
}

impl<S, B, C, CM, O> Clone for CacheService<S, B, C, CM, O>
where
    S: Clone,
    B: Clone,
    C: Clone,
    CM: Clone,
    O: Clone,
{
    fn clone(&self) -> Self {
        Self {
            upstream: self.upstream.clone(),
            backend: self.backend.clone(),
            configuration: self.configuration.clone(),
            offload: self.offload.clone(),
            concurrency_manager: self.concurrency_manager.clone(),
        }
    }
}

impl<S, B, C, CM, O, ReqBody, ResBody> Service<Request<ReqBody>> for CacheService<S, B, C, CM, O>
where
    S: Service<Request<BufferedBody<ReqBody>>, Response = Response<ResBody>>
        + Clone
        + Send
        + 'static,
    B: CacheBackend + Clone + Send + Sync + 'static,
    S::Future: Send,
    C: CacheConfig<CacheableHttpRequest<ReqBody>, CacheableHttpResponse<ResBody>>,
    CM: ConcurrencyManager<Result<CacheableHttpResponse<ResBody>, S::Error>> + Clone + 'static,
    O: Offload<'static> + Clone,
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
            'static,
            B,
            CacheableHttpRequest<ReqBody>,
            Result<CacheableHttpResponse<ResBody>, S::Error>,
            TowerUpstream<S, ReqBody, ResBody>,
            C::RequestPredicate,
            C::ResponsePredicate,
            C::Extractor,
            CM,
            O,
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
            self.offload.clone(),
            self.concurrency_manager.clone(),
        );

        // Wrap in CacheServiceFuture to add cache headers
        CacheServiceFuture::new(cache_future)
    }
}
