use std::sync::Arc;

use crate::Extractor;
use crate::policy::PolicyConfig;
use crate::predicate::Predicate;

pub type BoxPredicate<R> = Box<dyn Predicate<Subject = R> + Send + Sync>;
pub type BoxExtractor<Req> = Box<dyn Extractor<Subject = Req> + Send + Sync>;

pub trait CacheConfig<Req, Res> {
    type RequestPredicate: Predicate<Subject = Req> + Send + Sync + 'static;
    type ResponsePredicate: Predicate<Subject = Res> + Send + Sync + 'static;
    type Extractor: Extractor<Subject = Req> + Send + Sync + 'static;

    fn request_predicates(&self) -> Self::RequestPredicate;
    fn response_predicates(&self) -> Self::ResponsePredicate;
    fn extractors(&self) -> Self::Extractor;
    fn policy(&self) -> &PolicyConfig;
}

impl<T, Req, Res> CacheConfig<Req, Res> for Arc<T>
where
    T: CacheConfig<Req, Res>,
{
    type RequestPredicate = T::RequestPredicate;
    type ResponsePredicate = T::ResponsePredicate;
    type Extractor = T::Extractor;

    fn request_predicates(&self) -> Self::RequestPredicate {
        self.as_ref().request_predicates()
    }

    fn response_predicates(&self) -> Self::ResponsePredicate {
        self.as_ref().response_predicates()
    }

    fn extractors(&self) -> Self::Extractor {
        self.as_ref().extractors()
    }

    fn policy(&self) -> &PolicyConfig {
        self.as_ref().policy()
    }
}
