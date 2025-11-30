use std::fmt::Debug;

use hitbox::config::CacheConfig;
use hitbox::policy::PolicyConfig;

use crate::extractors::NeutralExtractor;
use crate::predicates::{NeutralRequestPredicate, NeutralResponsePredicate};
use crate::{CacheableHttpRequest, CacheableHttpResponse};

#[derive(Debug, Clone, Default)]
pub struct HttpEndpoint {
    pub policy: PolicyConfig,
}

impl<ReqBody, ResBody> CacheConfig<CacheableHttpRequest<ReqBody>, CacheableHttpResponse<ResBody>>
    for HttpEndpoint
where
    ReqBody: hyper::body::Body + Send + Debug + 'static,
    ReqBody::Error: Send,
    ReqBody::Data: Send,
    ResBody: hyper::body::Body + Send + 'static,
    ResBody::Error: Send,
    ResBody::Data: Send,
{
    type RequestPredicate = NeutralRequestPredicate<ReqBody>;
    type ResponsePredicate = NeutralResponsePredicate<ResBody>;
    type Extractor = NeutralExtractor<ReqBody>;

    fn request_predicates(&self) -> Self::RequestPredicate {
        NeutralRequestPredicate::new()
    }

    fn response_predicates(&self) -> Self::ResponsePredicate {
        NeutralResponsePredicate::new()
    }

    fn extractors(&self) -> Self::Extractor {
        NeutralExtractor::new()
    }

    fn policy(&self) -> &PolicyConfig {
        &self.policy
    }
}
