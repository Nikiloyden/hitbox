use std::fmt::Debug;

use futures::future::BoxFuture;
use hitbox_backend::BackendError;
use hitbox_core::{BoxContext, RequestCachePolicy, ResponseCachePolicy, Upstream};
use pin_project::pin_project;
use tokio::sync::OwnedSemaphorePermit;

use crate::{CacheValue, CacheableResponse};

pub type CacheResult<T> = Result<Option<CacheValue<T>>, BackendError>;
/// Future that polls the cache and returns (result, context)
pub type PollCacheFuture<T> = BoxFuture<'static, (CacheResult<T>, BoxContext)>;
/// Future that updates the cache and returns (backend_result, response, context)
pub type UpdateCache<T> = BoxFuture<'static, (Result<(), BackendError>, T, BoxContext)>;
pub type RequestCachePolicyFuture<T> = BoxFuture<'static, RequestCachePolicy<T>>;
pub type AwaitResponseFuture<T> =
    BoxFuture<'static, Result<T, crate::concurrency::ConcurrencyError>>;
/// Future that converts cached value to response and returns (response, context)
pub type ConvertResponseFuture<T> = BoxFuture<'static, (T, BoxContext)>;

#[allow(missing_docs)]
#[pin_project(project = StateProj)]
pub enum State<Res, Req, U>
where
    Res: CacheableResponse,
    U: Upstream<Req, Response = Res>,
{
    /// Initial state - context is created here
    Initial { ctx: Option<BoxContext> },
    /// Checking if request should be cached
    CheckRequestCachePolicy {
        #[pin]
        cache_policy_future: RequestCachePolicyFuture<Req>,
        ctx: Option<BoxContext>,
    },
    /// Polling the cache backend - context is captured in the future
    PollCache {
        #[pin]
        poll_cache: PollCacheFuture<Res::Cached>,
        request: Option<Req>,
        // Note: ctx is inside poll_cache future, returned with result
    },
    /// Converting cached value to response (cache hit, no refill needed)
    ConvertResponse {
        #[pin]
        response_future: ConvertResponseFuture<Res>,
        request: Option<Req>,
    },
    /// Handling stale cache hit - convert to response then apply stale policy
    HandleStale {
        #[pin]
        response_future: ConvertResponseFuture<Res>,
        request: Option<Req>,
    },
    /// Check concurrency policy
    CheckConcurrency {
        request: Option<Req>,
        ctx: Option<BoxContext>,
    },
    /// Concurrent upstream polling with concurrency control
    ConcurrentPollUpstream {
        request: Option<Req>,
        concurrency: usize,
        ctx: Option<BoxContext>,
    },
    /// Awaiting response from another concurrent request
    AwaitResponse {
        #[pin]
        await_response_future: AwaitResponseFuture<Res>,
        request: Option<Req>,
        ctx: Option<BoxContext>,
    },
    /// Polling upstream service
    PollUpstream {
        #[pin]
        upstream_future: U::Future,
        permit: Option<OwnedSemaphorePermit>,
        ctx: Option<BoxContext>,
    },
    /// Upstream response received
    UpstreamPolled {
        upstream_result: Option<Res>,
        permit: Option<OwnedSemaphorePermit>,
        ctx: Option<BoxContext>,
    },
    /// Checking if response should be cached
    CheckResponseCachePolicy {
        #[pin]
        cache_policy: BoxFuture<'static, ResponseCachePolicy<Res>>,
        permit: Option<OwnedSemaphorePermit>,
        ctx: Option<BoxContext>,
    },
    /// Updating cache with response - context is captured in the future
    UpdateCache {
        #[pin]
        update_cache_future: UpdateCache<Res>,
        // Note: ctx is inside update_cache_future, returned with result
    },
    /// Final state with response
    Response {
        response: Option<Res>,
        ctx: Option<BoxContext>,
    },
}

impl<Res, Req, U> Debug for State<Res, Req, U>
where
    Res: CacheableResponse,
    U: Upstream<Req, Response = Res>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            State::Initial { .. } => f.write_str("State::Initial"),
            State::CheckRequestCachePolicy { .. } => f.write_str("State::CheckRequestCachePolicy"),
            State::PollCache { .. } => f.write_str("State::PollCache"),
            State::ConvertResponse { .. } => f.write_str("State::ConvertResponse"),
            State::HandleStale { .. } => f.write_str("State::HandleStale"),
            State::CheckConcurrency { .. } => f.write_str("State::CheckConcurrency"),
            State::ConcurrentPollUpstream { .. } => f.write_str("State::ConcurrentPollUpstream"),
            State::AwaitResponse { .. } => f.write_str("State::AwaitResponse"),
            State::CheckResponseCachePolicy { .. } => {
                f.write_str("State::CheckResponseCachePolicy")
            }
            State::PollUpstream { .. } => f.write_str("State::PollUpstream"),
            State::UpstreamPolled { .. } => f.write_str("State::UpstreamPolled"),
            State::UpdateCache { .. } => f.write_str("State::UpdateCache"),
            State::Response { .. } => f.write_str("State::Response"),
        }
    }
}
