use std::{fmt::Debug, sync::Arc};

use async_trait::async_trait;

pub enum PredicateResult<S> {
    Cacheable(S),
    NonCacheable(S),
}

impl<S> PredicateResult<S> {
    /// Maps a `PredicateResult<S>` to `PredicateResult<T>` by applying a function to the contained value
    /// only if the result is `Cacheable`. If the result is `NonCacheable`, the value is still transformed
    /// but remains `NonCacheable`.
    pub async fn map<T, F, Fut>(self, f: F) -> PredicateResult<T>
    where
        F: FnOnce(S) -> Fut,
        Fut: std::future::Future<Output = PredicateResult<T>>,
    {
        match self {
            PredicateResult::Cacheable(value) => f(value).await,
            PredicateResult::NonCacheable(value) => match f(value).await {
                PredicateResult::Cacheable(t) => PredicateResult::NonCacheable(t),
                PredicateResult::NonCacheable(t) => PredicateResult::NonCacheable(t),
            },
        }
    }
}

// @FIX: remove Debug bound for Predicate
#[async_trait]
pub trait Predicate: Debug {
    type Subject;
    async fn check(&self, subject: Self::Subject) -> PredicateResult<Self::Subject>;
}

#[async_trait]
impl<T> Predicate for Box<T>
where
    T: Predicate + ?Sized + Sync,
    T::Subject: Send,
{
    type Subject = T::Subject;

    async fn check(&self, subject: T::Subject) -> PredicateResult<T::Subject> {
        self.as_ref().check(subject).await
    }
}

#[async_trait]
impl<T> Predicate for &T
where
    T: Predicate + ?Sized + Sync,
    T::Subject: Send,
{
    type Subject = T::Subject;

    async fn check(&self, subject: T::Subject) -> PredicateResult<T::Subject> {
        self.check(subject).await
    }
}

#[async_trait]
impl<T> Predicate for Arc<T>
where
    T: Predicate + Send + Sync + ?Sized,
    T::Subject: Send,
{
    type Subject = T::Subject;

    async fn check(&self, subject: T::Subject) -> PredicateResult<T::Subject> {
        self.as_ref().check(subject).await
    }
}
