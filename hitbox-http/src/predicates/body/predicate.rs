use std::fmt::Debug;

use async_trait::async_trait;
use hitbox::predicate::{Predicate, PredicateResult};
use hyper::body::Body as HttpBody;

use super::operation::Operation;
use crate::CacheableSubject;

#[derive(Debug)]
pub struct Body<P> {
    pub(crate) operation: Operation,
    pub(crate) inner: P,
}

pub trait BodyPredicate: Sized {
    fn body(self, operation: Operation) -> Body<Self>;
}

impl<P> BodyPredicate for P
where
    P: Predicate,
{
    fn body(self, operation: Operation) -> Body<Self> {
        Body {
            operation,
            inner: self,
        }
    }
}

// Generic implementation for any CacheableSubject
#[async_trait]
impl<P> Predicate for Body<P>
where
    P: Predicate + Send + Sync,
    P::Subject: CacheableSubject + Send,
    <P::Subject as CacheableSubject>::Body: Send + Unpin + 'static,
    <P::Subject as CacheableSubject>::Parts: Send,
    <<P::Subject as CacheableSubject>::Body as HttpBody>::Error: Debug + Send,
    <<P::Subject as CacheableSubject>::Body as HttpBody>::Data: Send,
{
    type Subject = P::Subject;

    async fn check(&self, subject: Self::Subject) -> PredicateResult<Self::Subject> {
        self.inner
            .check(subject)
            .await
            .map(|subject| async move {
                let (parts, body) = subject.into_parts();

                // Delegate to Operation::check
                let result = self.operation.check(body).await;

                // Convert back using CacheableSubject
                match result {
                    PredicateResult::Cacheable(buffered_body) => {
                        PredicateResult::Cacheable(P::Subject::from_parts(parts, buffered_body))
                    }
                    PredicateResult::NonCacheable(buffered_body) => {
                        PredicateResult::NonCacheable(P::Subject::from_parts(parts, buffered_body))
                    }
                }
            })
            .await
    }
}
