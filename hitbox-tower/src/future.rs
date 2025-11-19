use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Future;
use futures::ready;
use hitbox::{CacheContext, CacheStatus};
use hitbox_http::{BufferedBody, CacheableHttpResponse};
use http::{HeaderValue, Response};
use pin_project::pin_project;

/// Wrapper future that adds cache status headers to the response.
/// This future wraps `CacheFuture` and handles the final transformation
/// from cacheable response to HTTP response with cache headers.
#[pin_project]
pub struct CacheServiceFuture<F, ResBody, E>
where
    F: Future<Output = (Result<CacheableHttpResponse<ResBody>, E>, CacheContext)>,
    ResBody: hyper::body::Body,
{
    #[pin]
    inner: F,
}

impl<F, ResBody, E> CacheServiceFuture<F, ResBody, E>
where
    F: Future<Output = (Result<CacheableHttpResponse<ResBody>, E>, CacheContext)>,
    ResBody: hyper::body::Body,
{
    pub fn new(inner: F) -> Self {
        Self { inner }
    }
}

impl<F, ResBody, E> Future for CacheServiceFuture<F, ResBody, E>
where
    F: Future<Output = (Result<CacheableHttpResponse<ResBody>, E>, CacheContext)>,
    ResBody: hyper::body::Body,
    E: Debug,
{
    type Output = Result<Response<BufferedBody<ResBody>>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        // Poll the inner CacheFuture
        let (result, cache_context) = ready!(this.inner.poll(cx));

        // Transform the response and add cache headers
        let response = result.map(|cacheable_response| {
            let mut response = cacheable_response.into_response();

            // Add X-Cache-Status header based on cache context
            let status_value = match cache_context.status {
                CacheStatus::Hit => HeaderValue::from_static("HIT"),
                CacheStatus::Miss => HeaderValue::from_static("MISS"),
                CacheStatus::Stale => HeaderValue::from_static("STALE"),
            };
            response
                .headers_mut()
                .insert("X-Cache-Status", status_value);

            response
        });

        Poll::Ready(response)
    }
}
