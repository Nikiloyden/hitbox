//! Cache status extension for HTTP responses.
//!
//! This module provides the [`CacheStatusExt`] implementation for HTTP responses,
//! allowing cache status information to be attached as headers.

use hitbox::{CacheStatus, CacheStatusExt};
use http::{HeaderValue, header::HeaderName};
use hyper::body::Body as HttpBody;

use crate::CacheableHttpResponse;

/// Default header name for cache status (HIT/MISS/STALE).
///
/// The value is `x-cache-status`. Use builder methods on cache middleware
/// to customize the header name.
pub const DEFAULT_CACHE_STATUS_HEADER: HeaderName = HeaderName::from_static("x-cache-status");

impl<ResBody> CacheStatusExt for CacheableHttpResponse<ResBody>
where
    ResBody: HttpBody,
{
    type Config = HeaderName;

    fn cache_status(&mut self, status: CacheStatus, config: &Self::Config) {
        let value = match status {
            CacheStatus::Hit => HeaderValue::from_static("HIT"),
            CacheStatus::Miss => HeaderValue::from_static("MISS"),
            CacheStatus::Stale => HeaderValue::from_static("STALE"),
        };
        self.parts.headers.insert(config.clone(), value);
    }
}
