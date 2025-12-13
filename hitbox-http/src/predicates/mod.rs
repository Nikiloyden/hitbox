use hitbox::Neutral;

use crate::{CacheableHttpRequest, CacheableHttpResponse};

pub mod body;
pub mod conditions;
pub mod header;
pub mod request;
pub mod response;
pub mod version;

/// A neutral predicate for HTTP requests that always returns `Cacheable`.
pub type NeutralRequestPredicate<ReqBody> = Neutral<CacheableHttpRequest<ReqBody>>;

/// A neutral predicate for HTTP responses that always returns `Cacheable`.
pub type NeutralResponsePredicate<ResBody> = Neutral<CacheableHttpResponse<ResBody>>;
