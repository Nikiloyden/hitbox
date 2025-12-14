use async_trait::async_trait;
use hitbox::Neutral;
use hitbox::predicate::{Predicate, PredicateResult};
use http::Version;

use super::operation::Operation;

/// Predicate for checking HTTP version
#[derive(Debug)]
pub struct HttpVersion<P> {
    pub(crate) operation: Operation,
    pub(crate) inner: P,
}

impl<S> HttpVersion<Neutral<S>> {
    pub fn new(operation: Operation) -> Self {
        Self {
            operation,
            inner: Neutral::new(),
        }
    }
}

/// Extension trait for adding version predicates
pub trait VersionPredicate: Sized {
    fn version(self, operation: Operation) -> HttpVersion<Self>;
}

impl<P> VersionPredicate for P
where
    P: Predicate,
{
    fn version(self, operation: Operation) -> HttpVersion<Self> {
        HttpVersion {
            operation,
            inner: self,
        }
    }
}

/// Trait for subjects that have an HTTP version
pub trait HasVersion {
    fn http_version(&self) -> Version;
}

// Generic implementation for any subject that has an HTTP version
#[async_trait]
impl<P> Predicate for HttpVersion<P>
where
    P: Predicate + Send + Sync,
    P::Subject: HasVersion + Send,
{
    type Subject = P::Subject;

    async fn check(&self, subject: Self::Subject) -> PredicateResult<Self::Subject> {
        match self.inner.check(subject).await {
            PredicateResult::Cacheable(subject) => {
                if self.operation.check(subject.http_version()) {
                    PredicateResult::Cacheable(subject)
                } else {
                    PredicateResult::NonCacheable(subject)
                }
            }
            PredicateResult::NonCacheable(subject) => PredicateResult::NonCacheable(subject),
        }
    }
}
