use std::fmt::Debug;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project::pin_project;

use crate::{
    CachePolicy, EntityPolicyConfig,
    predicate::{Predicate, PredicateResult},
    value::{CacheValue, current_time},
};

/// This trait determines which types should be cached or not.
// pub enum CachePolicy<C>
// where
//     C: CacheableResponse,
// {
//     /// This variant should be stored in cache backend
//     Cacheable(CachedValue<C::Cached>),
//     /// This variant shouldn't be stored in the cache backend.
//     NonCacheable(C),
// }
pub type ResponseCachePolicy<C> = CachePolicy<CacheValue<<C as CacheableResponse>::Cached>, C>;

#[derive(Debug, PartialEq, Eq)]
pub enum CacheState<Cached> {
    Stale(Cached),
    Actual(Cached),
    Expired(Cached),
}

pub trait CacheableResponse
where
    Self: Sized + Send + 'static,
    Self::Cached: Clone,
{
    type Cached;
    type Subject: CacheableResponse;

    /// Future type for `into_cached` method
    type IntoCachedFuture: Future<Output = CachePolicy<Self::Cached, Self>> + Send;
    /// Future type for `from_cached` method
    type FromCachedFuture: Future<Output = Self> + Send;

    fn cache_policy<P>(
        self,
        predicates: P,
        config: &EntityPolicyConfig,
    ) -> impl Future<Output = ResponseCachePolicy<Self>> + Send
    where
        P: Predicate<Subject = Self::Subject> + Send + Sync;

    fn into_cached(self) -> Self::IntoCachedFuture;

    fn from_cached(cached: Self::Cached) -> Self::FromCachedFuture;
}

// =============================================================================
// Result<T, E> wrapper futures
// =============================================================================

/// Future for `Result<T, E>::into_cached` that wraps `T::IntoCachedFuture`.
#[pin_project(project = ResultIntoCachedProj)]
pub enum ResultIntoCachedFuture<T, E>
where
    T: CacheableResponse,
{
    /// Ok variant - wraps the inner type's future
    Ok(#[pin] T::IntoCachedFuture),
    /// Err variant - contains the error to return immediately
    Err(Option<E>),
}

impl<T, E> Future for ResultIntoCachedFuture<T, E>
where
    T: CacheableResponse,
{
    type Output = CachePolicy<T::Cached, Result<T, E>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project() {
            ResultIntoCachedProj::Ok(fut) => fut.poll(cx).map(|policy| match policy {
                CachePolicy::Cacheable(res) => CachePolicy::Cacheable(res),
                CachePolicy::NonCacheable(res) => CachePolicy::NonCacheable(Ok(res)),
            }),
            ResultIntoCachedProj::Err(e) => {
                Poll::Ready(CachePolicy::NonCacheable(Err(e.take().unwrap())))
            }
        }
    }
}

/// Future for `Result<T, E>::from_cached` that wraps `T::FromCachedFuture`.
#[pin_project]
pub struct ResultFromCachedFuture<T, E>
where
    T: CacheableResponse,
{
    #[pin]
    inner: T::FromCachedFuture,
    _marker: PhantomData<E>,
}

impl<T, E> Future for ResultFromCachedFuture<T, E>
where
    T: CacheableResponse,
{
    type Output = Result<T, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().inner.poll(cx).map(Ok)
    }
}

// =============================================================================
// Result<T, E> implementation
// =============================================================================

impl<T, E> CacheableResponse for Result<T, E>
where
    T: CacheableResponse + 'static,
    E: Send + 'static,
    T::Cached: Send,
{
    type Cached = <T as CacheableResponse>::Cached;
    type Subject = T;
    type IntoCachedFuture = ResultIntoCachedFuture<T, E>;
    type FromCachedFuture = ResultFromCachedFuture<T, E>;

    async fn cache_policy<P>(
        self,
        predicates: P,
        config: &EntityPolicyConfig,
    ) -> ResponseCachePolicy<Self>
    where
        P: Predicate<Subject = Self::Subject> + Send + Sync,
    {
        match self {
            Ok(response) => match predicates.check(response).await {
                PredicateResult::Cacheable(cacheable) => match cacheable.into_cached().await {
                    CachePolicy::Cacheable(res) => CachePolicy::Cacheable(CacheValue::new(
                        res,
                        config.ttl.map(|duration| current_time() + duration),
                        config.stale_ttl.map(|duration| current_time() + duration),
                    )),
                    CachePolicy::NonCacheable(res) => CachePolicy::NonCacheable(Ok(res)),
                },
                PredicateResult::NonCacheable(res) => CachePolicy::NonCacheable(Ok(res)),
            },
            Err(error) => ResponseCachePolicy::NonCacheable(Err(error)),
        }
    }

    fn into_cached(self) -> Self::IntoCachedFuture {
        match self {
            Ok(response) => ResultIntoCachedFuture::Ok(response.into_cached()),
            Err(error) => ResultIntoCachedFuture::Err(Some(error)),
        }
    }

    fn from_cached(cached: Self::Cached) -> Self::FromCachedFuture {
        ResultFromCachedFuture {
            inner: T::from_cached(cached),
            _marker: PhantomData,
        }
    }
}
