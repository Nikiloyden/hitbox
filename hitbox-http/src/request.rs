use async_trait::async_trait;
use hitbox::{
    CacheablePolicyData, RequestCachePolicy,
    predicate::{Predicate, PredicateResult},
    {CachePolicy, CacheableRequest, Extractor},
};
use http::{Request, request::Parts};
use hyper::body::Body as HttpBody;

use crate::CacheableSubject;
use crate::body::BufferedBody;
use crate::predicates::header::HasHeaders;

#[derive(Debug)]
pub struct CacheableHttpRequest<ReqBody>
where
    ReqBody: HttpBody,
{
    parts: Parts,
    body: BufferedBody<ReqBody>,
}

impl<ReqBody> CacheableHttpRequest<ReqBody>
where
    ReqBody: HttpBody,
{
    pub fn from_request(request: Request<BufferedBody<ReqBody>>) -> Self {
        let (parts, body) = request.into_parts();
        Self { parts, body }
    }

    pub fn into_request(self) -> Request<BufferedBody<ReqBody>> {
        Request::from_parts(self.parts, self.body)
    }

    pub fn parts(&self) -> &Parts {
        &self.parts
    }

    pub fn into_parts(self) -> (Parts, BufferedBody<ReqBody>) {
        (self.parts, self.body)
    }
}

impl<ReqBody> CacheableSubject for CacheableHttpRequest<ReqBody>
where
    ReqBody: HttpBody,
{
    type Body = ReqBody;
    type Parts = Parts;

    fn into_parts(self) -> (Self::Parts, BufferedBody<Self::Body>) {
        (self.parts, self.body)
    }

    fn from_parts(parts: Self::Parts, body: BufferedBody<Self::Body>) -> Self {
        Self { parts, body }
    }
}

impl<ReqBody> HasHeaders for CacheableHttpRequest<ReqBody>
where
    ReqBody: HttpBody,
{
    fn headers(&self) -> &http::HeaderMap {
        &self.parts.headers
    }
}

#[async_trait]
impl<ReqBody> CacheableRequest for CacheableHttpRequest<ReqBody>
where
    ReqBody: HttpBody + Send + 'static,
    ReqBody::Error: Send,
{
    async fn cache_policy<P, E>(self, predicates: P, extractors: E) -> RequestCachePolicy<Self>
    where
        P: Predicate<Subject = Self> + Send + Sync,
        E: Extractor<Subject = Self> + Send + Sync,
    {
        //dbg!("CacheableHttpRequest::cache_policy");
        let (request, key) = extractors.get(self).await.into_cache_key();

        match predicates.check(request).await {
            PredicateResult::Cacheable(request) => {
                CachePolicy::Cacheable(CacheablePolicyData { key, request })
            }
            PredicateResult::NonCacheable(request) => CachePolicy::NonCacheable(request),
        }
    }
}
