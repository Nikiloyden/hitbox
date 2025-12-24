use async_trait::async_trait;
use hitbox::Neutral;
use hitbox::predicate::{Predicate, PredicateResult};
use http::HeaderMap;

use super::operation::Operation;

#[derive(Debug)]
pub struct Header<P> {
    pub(crate) operation: Operation,
    pub(crate) inner: P,
}

impl<S> Header<Neutral<S>> {
    pub fn new(operation: Operation) -> Self {
        Self {
            operation,
            inner: Neutral::new(),
        }
    }
}

pub trait HeaderPredicate: Sized {
    fn header(self, operation: Operation) -> Header<Self>;
}

impl<P> HeaderPredicate for P
where
    P: Predicate,
{
    fn header(self, operation: Operation) -> Header<Self> {
        Header {
            operation,
            inner: self,
        }
    }
}

/// Trait for extracting headers from a subject
pub trait HasHeaders {
    fn headers(&self) -> &HeaderMap;
}

// Generic implementation for any subject that has headers
#[async_trait]
impl<P> Predicate for Header<P>
where
    P: Predicate + Send + Sync,
    P::Subject: HasHeaders + Send,
{
    type Subject = P::Subject;

    async fn check(&self, subject: Self::Subject) -> PredicateResult<Self::Subject> {
        match self.inner.check(subject).await {
            PredicateResult::Cacheable(subject) => {
                let is_cacheable = self.operation.check(subject.headers());
                if is_cacheable {
                    PredicateResult::Cacheable(subject)
                } else {
                    PredicateResult::NonCacheable(subject)
                }
            }
            PredicateResult::NonCacheable(subject) => PredicateResult::NonCacheable(subject),
        }
    }
}
