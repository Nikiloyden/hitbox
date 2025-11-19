use std::fmt::Debug;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::ready;
use hitbox_core::Upstream;
use hitbox_http::{BufferedBody, CacheableHttpRequest, CacheableHttpResponse};
use http::{Request, Response};
use hyper::body::Body as HttpBody;
use pin_project::pin_project;
use tower::Service;

/// Future returned by `TowerUpstream::call`.
/// Wraps the underlying service future and converts the response.
#[pin_project]
pub struct TowerUpstreamFuture<F, ResBody, E> {
    #[pin]
    inner: F,
    _phantom: PhantomData<(ResBody, E)>,
}

impl<F, ResBody, E> TowerUpstreamFuture<F, ResBody, E> {
    pub fn new(inner: F) -> Self {
        Self {
            inner,
            _phantom: PhantomData,
        }
    }
}

impl<F, ResBody, E> Future for TowerUpstreamFuture<F, ResBody, E>
where
    F: Future<Output = Result<Response<ResBody>, E>>,
    ResBody: HttpBody,
    E: Debug,
{
    type Output = Result<CacheableHttpResponse<ResBody>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        match ready!(this.inner.poll(cx)) {
            Ok(response) => {
                let (parts, body) = response.into_parts();
                let buffered = Response::from_parts(parts, BufferedBody::Passthrough(body));
                Poll::Ready(Ok(CacheableHttpResponse::from_response(buffered)))
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

/// Adapter that implements `Upstream` trait for Tower services.
/// Handles conversion between HTTP types and cacheable types.
pub struct TowerUpstream<S, ReqBody, ResBody> {
    service: S,
    _phantom: PhantomData<(ReqBody, ResBody)>,
}

impl<S, ReqBody, ResBody> TowerUpstream<S, ReqBody, ResBody> {
    pub fn new(service: S) -> Self {
        Self {
            service,
            _phantom: PhantomData,
        }
    }
}

impl<S, ReqBody, ResBody> Upstream<CacheableHttpRequest<ReqBody>>
    for TowerUpstream<S, ReqBody, ResBody>
where
    S: Service<Request<BufferedBody<ReqBody>>, Response = Response<ResBody>>
        + Clone
        + Send
        + 'static,
    S::Future: Send,
    S::Error: Debug + Send,
    ReqBody: HttpBody + Send + 'static,
    ReqBody::Error: Send,
    ResBody: HttpBody + Send + 'static,
{
    type Response = Result<CacheableHttpResponse<ResBody>, S::Error>;
    type Future = TowerUpstreamFuture<S::Future, ResBody, S::Error>;

    fn call(&mut self, req: CacheableHttpRequest<ReqBody>) -> Self::Future {
        let http_req = req.into_request();
        let inner = self.service.call(http_req);
        TowerUpstreamFuture::new(inner)
    }
}
