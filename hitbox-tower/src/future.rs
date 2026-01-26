//! Future types for the cache service.
//!
//! This module provides [`CacheServiceFuture`](crate::future::CacheServiceFuture),
//! the future returned by [`CacheService::call`]. It wraps the inner cache future
//! and adds cache status headers to responses.
//!
//! Users typically don't interact with this module directly.
//!
//! [`CacheService::call`]: crate::service::CacheService

use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Future;
use futures::ready;
use hitbox::{CacheContext, CacheStatusExt};
use hitbox_http::{BufferedBody, CacheableHttpResponse};
use http::Response;
use http::header::HeaderName;
use pin_project::pin_project;

/// Future returned by [`CacheService::call`](crate::service::CacheService).
///
/// This future wraps the inner `CacheFuture` and performs the final transformation:
/// converting [`CacheableHttpResponse`] to `http::Response` and adding the cache
/// status header (`HIT`/`MISS`/`STALE`).
///
/// # When You'll Encounter This
///
/// You typically don't create this directly. It's the `Future` type returned when
/// calling the [`CacheService`](crate::service::CacheService) as a Tower service.
///
/// # Type Parameters
///
/// * `F` - The inner future (typically `CacheFuture`)
/// * `ResBody` - Response body type
/// * `E` - Error type from the upstream service
///
/// [`CacheableHttpResponse`]: hitbox_http::CacheableHttpResponse
#[pin_project]
pub struct CacheServiceFuture<F, ResBody, E>
where
    F: Future<Output = (Result<CacheableHttpResponse<ResBody>, E>, CacheContext)>,
    ResBody: hyper::body::Body,
{
    #[pin]
    inner: F,
    cache_status_header: HeaderName,
}

impl<F, ResBody, E> CacheServiceFuture<F, ResBody, E>
where
    F: Future<Output = (Result<CacheableHttpResponse<ResBody>, E>, CacheContext)>,
    ResBody: hyper::body::Body,
{
    /// Creates a new future that will add cache status headers to the response.
    pub fn new(inner: F, cache_status_header: HeaderName) -> Self {
        Self {
            inner,
            cache_status_header,
        }
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
        let response = result.map(|mut cacheable_response| {
            // Add cache status header based on cache context
            cacheable_response.cache_status(cache_context.status, this.cache_status_header);

            cacheable_response.into_response()
        });

        Poll::Ready(response)
    }
}
