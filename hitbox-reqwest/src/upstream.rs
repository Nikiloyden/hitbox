//! Upstream wrapper for reqwest-middleware's Next type.

use std::future::Future;
use std::pin::Pin;

use bytes::Bytes;
use hitbox_core::Upstream;
use hitbox_http::{BufferedBody, CacheableHttpRequest, CacheableHttpResponse};
use http::Extensions;
use reqwest_middleware::{Next, Result};

/// Upstream wrapper that bridges reqwest-middleware's `Next<'a>` to hitbox's `Upstream` trait.
///
/// This wrapper holds the middleware chain and converts between hitbox-http types
/// and reqwest types.
pub struct ReqwestUpstream<'a> {
    next: Next<'a>,
    extensions: Extensions,
}

impl<'a> ReqwestUpstream<'a> {
    /// Create a new upstream wrapper.
    pub fn new(next: Next<'a>, extensions: Extensions) -> Self {
        Self { next, extensions }
    }
}

impl<'a> Upstream<CacheableHttpRequest<reqwest::Body>> for ReqwestUpstream<'a> {
    type Response = Result<CacheableHttpResponse<reqwest::Body>>;
    type Future = Pin<Box<dyn Future<Output = Self::Response> + Send + 'a>>;

    fn call(&mut self, req: CacheableHttpRequest<reqwest::Body>) -> Self::Future {
        let next = self.next.clone();
        let mut extensions = std::mem::take(&mut self.extensions);

        Box::pin(async move {
            // Convert CacheableHttpRequest back to reqwest::Request
            let http_request = req.into_request();
            let (parts, buffered_body) = http_request.into_parts();

            // Convert BufferedBody back to reqwest::Body
            let body = buffered_body_to_reqwest(buffered_body);

            // Reconstruct http::Request and convert to reqwest::Request
            let http_request = http::Request::from_parts(parts, body);
            let reqwest_request: reqwest::Request = http_request
                .try_into()
                .map_err(|e: reqwest::Error| reqwest_middleware::Error::Reqwest(e))?;

            // Call the next middleware
            let response = next.run(reqwest_request, &mut extensions).await?;

            // Convert reqwest::Response to CacheableHttpResponse
            let http_response: http::Response<reqwest::Body> = response.into();
            let (parts, body) = http_response.into_parts();
            let buffered_body = BufferedBody::Passthrough(body);
            let http_response = http::Response::from_parts(parts, buffered_body);

            Ok(CacheableHttpResponse::from_response(http_response))
        })
    }
}

/// Convert BufferedBody to reqwest::Body.
///
/// This conversion is cheap for most cases:
/// - Passthrough: just unwraps the inner body (zero cost)
/// - Complete: creates body from bytes
/// - Partial: wraps the PartialBufferedBody which implements HttpBody
///   (handles prefix + remaining stream + error cases)
pub fn buffered_body_to_reqwest(buffered: BufferedBody<reqwest::Body>) -> reqwest::Body {
    match buffered {
        BufferedBody::Passthrough(body) => body,
        BufferedBody::Complete(Some(bytes)) => reqwest::Body::from(bytes),
        BufferedBody::Complete(None) => reqwest::Body::from(Bytes::new()),
        BufferedBody::Partial(partial) => {
            // PartialBufferedBody implements HttpBody, handling:
            // - prefix bytes (yielded first)
            // - remaining stream OR error
            reqwest::Body::wrap(partial)
        }
    }
}
