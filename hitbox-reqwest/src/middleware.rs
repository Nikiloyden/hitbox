//! Cache middleware for reqwest-middleware.

use std::sync::Arc;

use async_trait::async_trait;
use hitbox::backend::CacheBackend;
use hitbox::concurrency::NoopConcurrencyManager;
use hitbox::config::CacheConfig;
use hitbox::context::CacheStatus;
use hitbox::fsm::CacheFuture;
use hitbox_core::DisabledOffload;
use hitbox_http::{BufferedBody, CacheableHttpRequest, CacheableHttpResponse};
use http::Extensions;
use http::header::HeaderValue;
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next, Result};

use crate::upstream::{ReqwestUpstream, buffered_body_to_reqwest};

/// Cache middleware for reqwest-middleware.
///
/// This middleware intercepts HTTP requests and responses, caching them
/// according to the configured policy and predicates.
///
/// # Type Parameters
///
/// * `B` - Cache backend (e.g., MokaBackend, RedisBackend)
/// * `C` - Configuration implementing `CacheConfig`
pub struct CacheMiddleware<B, C> {
    backend: Arc<B>,
    configuration: C,
}

impl<B, C> CacheMiddleware<B, C> {
    /// Create a new cache middleware.
    pub fn new(backend: Arc<B>, configuration: C) -> Self {
        Self {
            backend,
            configuration,
        }
    }
}

impl<B, C> Clone for CacheMiddleware<B, C>
where
    C: Clone,
{
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
            configuration: self.configuration.clone(),
        }
    }
}

#[async_trait]
impl<B, C> Middleware for CacheMiddleware<B, C>
where
    B: CacheBackend + Send + Sync + 'static,
    C: CacheConfig<CacheableHttpRequest<reqwest::Body>, CacheableHttpResponse<reqwest::Body>>
        + Clone
        + Send
        + Sync
        + 'static,
    C::RequestPredicate: Clone + Send + Sync + 'static,
    C::ResponsePredicate: Clone + Send + Sync + 'static,
    C::Extractor: Clone + Send + Sync + 'static,
{
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        // Convert reqwest::Request to http::Request<reqwest::Body>
        let http_request: http::Request<reqwest::Body> = req
            .try_into()
            .map_err(|e: reqwest::Error| reqwest_middleware::Error::Reqwest(e))?;

        // Wrap body with BufferedBody and create CacheableHttpRequest
        let (parts, body) = http_request.into_parts();
        let buffered_request = http::Request::from_parts(parts, BufferedBody::Passthrough(body));
        let cacheable_req = CacheableHttpRequest::from_request(buffered_request);

        // Create upstream wrapper
        let upstream = ReqwestUpstream::new(next.clone(), extensions.clone());

        // Create CacheFuture with DisabledOffload (no background revalidation)
        // This allows us to use non-'static lifetimes
        let cache_future: CacheFuture<
            '_,
            B,
            CacheableHttpRequest<reqwest::Body>,
            Result<CacheableHttpResponse<reqwest::Body>>,
            ReqwestUpstream<'_>,
            C::RequestPredicate,
            C::ResponsePredicate,
            C::Extractor,
            NoopConcurrencyManager,
            DisabledOffload,
        > = CacheFuture::new(
            self.backend.clone(),
            cacheable_req,
            upstream,
            self.configuration.request_predicates(),
            self.configuration.response_predicates(),
            self.configuration.extractors(),
            Arc::new(self.configuration.policy().clone()),
            None::<DisabledOffload>,
            NoopConcurrencyManager,
        );

        // Execute cache future
        let (response, cache_context) = cache_future.await;

        // Convert CacheableHttpResponse back to reqwest::Response
        let cacheable_response = response?;
        let mut http_response = cacheable_response.into_response();

        // Add X-Cache-Status header based on cache context
        let status_value = match cache_context.status {
            CacheStatus::Hit => HeaderValue::from_static("HIT"),
            CacheStatus::Miss => HeaderValue::from_static("MISS"),
            CacheStatus::Stale => HeaderValue::from_static("STALE"),
        };
        http_response
            .headers_mut()
            .insert("X-Cache-Status", status_value);

        let (parts, buffered_body) = http_response.into_parts();

        // Convert BufferedBody back to reqwest::Body
        let body = buffered_body_to_reqwest(buffered_body);
        let http_response = http::Response::from_parts(parts, body);

        // Convert to reqwest::Response
        Ok(http_response.into())
    }
}
