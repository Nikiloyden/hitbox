//! FSM state types and resolved state structs.
//!
//! Each state struct represents resolved async data and has a `.transition()` method
//! that returns the appropriate transition enum. The transition enum then has
//! `.into_state()` to convert to the outer `State` enum.
//!
//! Flow: poll future → create state struct → `.transition()` → `.into_state()`

use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use hitbox_backend::BackendError;
use hitbox_core::{
    BoxContext, CachePolicy, CacheValue, Cacheable, CacheablePolicyData, DebugState,
    EntityPolicyConfig, Predicate, ReadMode, RequestCachePolicy, ResponseCachePolicy, Upstream,
};
use pin_project::pin_project;
use tokio::sync::OwnedSemaphorePermit;
use tracing::debug;

use crate::backend::CacheBackend;
use crate::concurrency::{ConcurrencyDecision, ConcurrencyError, ConcurrencyManager};
use crate::fsm::transitions::{
    AwaitResponseTransition, CheckRequestCachePolicyTransition, CheckResponseCachePolicyTransition,
    ConvertResponseTransition, HandleStaleTransition, InitialTransition, PollCacheTransition,
    PollUpstreamTransition, UpdateCacheTransition,
};
use crate::policy::{EnabledCacheConfig, PolicyConfig, StalePolicy};
use crate::{CacheKey, CacheState, CacheStatus, CacheableRequest, CacheableResponse, Extractor};

// =============================================================================
// Type Aliases
// =============================================================================

pub type CacheResult<T> = Result<Option<CacheValue<T>>, BackendError>;
/// Future that polls the cache and returns (result, context)
pub type PollCacheFuture<T> = BoxFuture<'static, (CacheResult<T>, BoxContext)>;
/// Future that updates the cache and returns (backend_result, response, context)
pub type UpdateCache<T> = BoxFuture<'static, (Result<(), BackendError>, T, BoxContext)>;
pub type RequestCachePolicyFuture<T> = BoxFuture<'static, RequestCachePolicy<T>>;
pub type AwaitResponseFuture<T> = BoxFuture<'static, Result<T, ConcurrencyError>>;
/// Future that converts cached value to response and returns (response, context)
pub type ConvertResponseFuture<T> = BoxFuture<'static, (T, BoxContext)>;

// =============================================================================
// State Enum
// =============================================================================

#[allow(missing_docs)]
#[pin_project(project = StateProj)]
pub enum State<Res, Req, U, ReqP, E>
where
    Res: CacheableResponse,
    Req: CacheableRequest,
    U: Upstream<Req, Response = Res>,
    ReqP: Predicate<Subject = Req>,
    E: Extractor<Subject = Req>,
{
    /// Initial state - all data needed for the first transition
    Initial(Option<Initial<Req, ReqP, E, U>>),
    /// Checking if request should be cached
    CheckRequestCachePolicy {
        #[pin]
        cache_policy_future: RequestCachePolicyFuture<Req>,
        state: Option<CheckRequestCachePolicy<U>>,
    },
    /// Polling the cache backend - context is captured in the future
    PollCache {
        #[pin]
        poll_cache: PollCacheFuture<Res::Cached>,
        state: Option<PollCache<Req, U>>,
    },
    /// Converting cached value to response (cache hit, no refill needed)
    ConvertResponse {
        #[pin]
        response_future: ConvertResponseFuture<Res>,
        state: Option<ConvertResponse>,
    },
    /// Handling stale cache hit - convert to response then apply stale policy
    HandleStale {
        #[pin]
        response_future: ConvertResponseFuture<Res>,
        state: Option<HandleStale<Req, U>>,
    },
    /// Awaiting response from another concurrent request
    AwaitResponse {
        #[pin]
        await_response_future: AwaitResponseFuture<Res>,
        state: Option<AwaitResponse<Req, U>>,
    },
    /// Polling upstream service
    PollUpstream {
        #[pin]
        upstream_future: U::Future,
        state: Option<PollUpstream>,
    },
    /// Checking if response should be cached
    CheckResponseCachePolicy {
        #[pin]
        cache_policy: BoxFuture<'static, ResponseCachePolicy<Res>>,
        state: Option<CheckResponseCachePolicy>,
    },
    /// Updating cache with response - context is captured in the future
    UpdateCache {
        #[pin]
        update_cache_future: UpdateCache<Res>,
    },
    /// Final state with response
    Response(Option<Response<Res>>),
}

impl<Res, Req, U, ReqP, E> Debug for State<Res, Req, U, ReqP, E>
where
    Res: CacheableResponse,
    Req: CacheableRequest,
    U: Upstream<Req, Response = Res>,
    ReqP: Predicate<Subject = Req>,
    E: Extractor<Subject = Req>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            State::Initial(_) => f.write_str("State::Initial"),
            State::CheckRequestCachePolicy { .. } => f.write_str("State::CheckRequestCachePolicy"),
            State::PollCache { .. } => f.write_str("State::PollCache"),
            State::ConvertResponse { .. } => f.write_str("State::ConvertResponse"),
            State::HandleStale { .. } => f.write_str("State::HandleStale"),
            State::AwaitResponse { .. } => f.write_str("State::AwaitResponse"),
            State::CheckResponseCachePolicy { .. } => {
                f.write_str("State::CheckResponseCachePolicy")
            }
            State::PollUpstream { .. } => f.write_str("State::PollUpstream"),
            State::UpdateCache { .. } => f.write_str("State::UpdateCache"),
            State::Response(_) => f.write_str("State::Response"),
        }
    }
}

// =============================================================================
// Initial
// =============================================================================

/// Data gathered from Initial state (synchronous).
pub struct Initial<Req, ReqP, E, U> {
    pub request: Req,
    pub predicates: ReqP,
    pub extractors: E,
    pub ctx: BoxContext,
    pub upstream: U,
}

impl<Req, ReqP, E, U> Initial<Req, ReqP, E, U>
where
    Req: CacheableRequest + Send + 'static,
    ReqP: Predicate<Subject = Req> + Send + Sync + 'static,
    E: Extractor<Subject = Req> + Send + Sync + 'static,
    U: Upstream<Req>,
{
    /// Transition from Initial state.
    ///
    /// Based on policy configuration:
    /// - Enabled: create CheckRequestCachePolicy future
    /// - Disabled: call upstream directly
    pub fn transition(mut self, policy: &PolicyConfig) -> InitialTransition<Req, U> {
        self.ctx.record_state(DebugState::Initial);
        match policy {
            PolicyConfig::Enabled(_) => {
                // cache_policy returns BoxFuture via async_trait, no need to wrap again
                let cache_policy_future =
                    self.request.cache_policy(self.predicates, self.extractors);
                InitialTransition::CheckRequestCachePolicy {
                    cache_policy_future,
                    ctx: self.ctx,
                    upstream: self.upstream,
                }
            }
            PolicyConfig::Disabled => {
                let upstream_future = self.upstream.call(self.request);
                InitialTransition::PollUpstream {
                    upstream_future,
                    ctx: self.ctx,
                }
            }
        }
    }
}

impl<Req, ReqP, E, U> std::fmt::Debug for Initial<Req, ReqP, E, U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Initial").finish_non_exhaustive()
    }
}

// =============================================================================
// Response
// =============================================================================

/// Terminal state with final response.
pub struct Response<Res> {
    pub response: Res,
    pub ctx: BoxContext,
}

impl<Res> std::fmt::Debug for Response<Res> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Response").finish_non_exhaustive()
    }
}

// =============================================================================
// PollUpstream
// =============================================================================

/// Data for PollUpstream state (non-pinned part).
///
/// The upstream future is stored separately in the State enum to allow pinning.
/// When the future completes, this data is taken and passed to transition().
pub struct PollUpstream {
    pub permit: Option<OwnedSemaphorePermit>,
    pub ctx: BoxContext,
    pub cache_key: Option<CacheKey>,
}

impl PollUpstream {
    /// Transition from PollUpstream state after future completes.
    ///
    /// This merges the old PollUpstream → UpstreamPolled → next state transitions
    /// into a single step, since UpstreamPolled was a synchronous state.
    pub fn transition<Res, ResP>(
        mut self,
        upstream_result: Res,
        predicates: ResP,
        policy: &PolicyConfig,
    ) -> PollUpstreamTransition<Res>
    where
        Res: CacheableResponse + Send + 'static,
        ResP: Predicate<Subject = Res::Subject> + Send + Sync + 'static,
    {
        self.ctx.record_state(DebugState::PollUpstream);
        self.ctx.record_state(DebugState::UpstreamPolled);

        match self.cache_key {
            Some(cache_key) => {
                let entity_config = match policy {
                    PolicyConfig::Enabled(config) => EntityPolicyConfig {
                        ttl: config.ttl.map(|s| Duration::from_secs(s as u64)),
                        stale_ttl: config.stale.map(|s| Duration::from_secs(s as u64)),
                    },
                    PolicyConfig::Disabled => EntityPolicyConfig::default(),
                };
                PollUpstreamTransition::CheckResponseCachePolicy {
                    cache_policy_future: Box::pin(async move {
                        upstream_result
                            .cache_policy(predicates, &entity_config)
                            .await
                    }),
                    permit: self.permit,
                    ctx: self.ctx,
                    cache_key,
                }
            }
            None => PollUpstreamTransition::Response(Response {
                response: upstream_result,
                ctx: self.ctx,
            }),
        }
    }
}

impl std::fmt::Debug for PollUpstream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PollUpstream")
            .field("has_permit", &self.permit.is_some())
            .field("cache_key", &self.cache_key)
            .finish_non_exhaustive()
    }
}

// =============================================================================
// CheckResponseCachePolicy
// =============================================================================

/// Data for CheckResponseCachePolicy state (non-pinned part).
///
/// The cache policy future is stored separately in the State enum to allow pinning.
/// When the future completes, this data is taken and passed to transition().
pub struct CheckResponseCachePolicy {
    pub permit: Option<OwnedSemaphorePermit>,
    pub ctx: BoxContext,
    pub cache_key: CacheKey,
}

impl CheckResponseCachePolicy {
    /// Transition from CheckResponseCachePolicy state after future completes.
    pub fn transition<Res, B, C>(
        mut self,
        policy: CachePolicy<CacheValue<Res::Cached>, Res>,
        backend: Arc<B>,
        concurrency_manager: &C,
    ) -> CheckResponseCachePolicyTransition<Res>
    where
        Res: CacheableResponse + Send + 'static,
        Res::Cached: Cacheable + Send,
        B: CacheBackend + Send + Sync + 'static,
        C: ConcurrencyManager<Res>,
    {
        self.ctx.record_state(DebugState::CheckResponseCachePolicy);

        match policy {
            CachePolicy::Cacheable(cache_value) => {
                if self.permit.is_some() {
                    concurrency_manager.resolve(&self.cache_key, &cache_value);
                }
                let cache_key = self.cache_key;
                let mut ctx = self.ctx;
                let update_cache_future = Box::pin(async move {
                    let update_cache_result =
                        backend.set::<Res>(&cache_key, &cache_value, &mut ctx).await;
                    let upstream_result = Res::from_cached(cache_value.into_inner()).await;
                    (update_cache_result, upstream_result, ctx)
                });
                CheckResponseCachePolicyTransition::UpdateCache {
                    update_cache_future,
                }
            }
            CachePolicy::NonCacheable(response) => {
                if self.permit.is_some() {
                    concurrency_manager.cleanup(&self.cache_key);
                }
                CheckResponseCachePolicyTransition::Response(Response {
                    response,
                    ctx: self.ctx,
                })
            }
        }
    }
}

impl std::fmt::Debug for CheckResponseCachePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckResponseCachePolicy")
            .field("has_permit", &self.permit.is_some())
            .field("cache_key", &self.cache_key)
            .finish_non_exhaustive()
    }
}

// =============================================================================
// CheckRequestCachePolicy
// =============================================================================

/// Data for CheckRequestCachePolicy state (non-pinned part).
///
/// The cache policy future is stored separately in the State enum to allow pinning.
/// When the future completes, this data is taken and passed to transition().
pub struct CheckRequestCachePolicy<U> {
    pub ctx: BoxContext,
    pub upstream: U,
}

impl<U> CheckRequestCachePolicy<U> {
    /// Transition from CheckRequestCachePolicy state after future completes.
    pub fn transition<Req, Res, B>(
        mut self,
        policy: RequestCachePolicy<Req>,
        backend: Arc<B>,
        cache_key_storage: &mut Option<CacheKey>,
    ) -> CheckRequestCachePolicyTransition<Req, Res, U>
    where
        Req: CacheableRequest,
        Res: CacheableResponse,
        Res::Cached: Cacheable + Send,
        U: Upstream<Req, Response = Res>,
        B: CacheBackend + Send + Sync + 'static,
    {
        self.ctx.record_state(DebugState::CheckRequestCachePolicy);
        match policy {
            CachePolicy::Cacheable(CacheablePolicyData { key, request }) => {
                let cache_key_for_get = key.clone();
                debug!(?key, "FSM looking up cache key");
                let _ = cache_key_storage.insert(key.clone());
                let mut ctx = self.ctx;
                let poll_cache = Box::pin(async move {
                    let result = backend.get::<Res>(&cache_key_for_get, &mut ctx).await;
                    debug!(
                        found = result.as_ref().map(|r| r.is_some()).unwrap_or(false),
                        "FSM cache lookup result"
                    );
                    (result, ctx)
                });
                CheckRequestCachePolicyTransition::PollCache {
                    poll_cache,
                    request,
                    cache_key: key,
                    upstream: self.upstream,
                }
            }
            CachePolicy::NonCacheable(request) => {
                let upstream_future = self.upstream.call(request);
                CheckRequestCachePolicyTransition::PollUpstream {
                    upstream_future,
                    ctx: self.ctx,
                }
            }
        }
    }
}

impl<U> std::fmt::Debug for CheckRequestCachePolicy<U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckRequestCachePolicy")
            .finish_non_exhaustive()
    }
}

// =============================================================================
// PollCache
// =============================================================================

/// Data for PollCache state (non-pinned part).
///
/// The poll cache future is stored separately in the State enum to allow pinning.
/// When the future completes, this data is taken and passed to transition().
pub struct PollCache<Req, U> {
    pub request: Req,
    pub cache_key: CacheKey,
    pub upstream: U,
}

impl<Req, U> PollCache<Req, U> {
    /// Transition from PollCache state after future completes.
    ///
    /// On cache miss or expired, checks concurrency policy and transitions directly
    /// to either `ConcurrentPollUpstream` or `PollUpstream`.
    pub fn transition<Res, B, C>(
        self,
        cache_result: CacheResult<Res::Cached>,
        mut ctx: BoxContext,
        backend: Arc<B>,
        policy: &PolicyConfig,
        concurrency_manager: &C,
    ) -> PollCacheTransition<Res, Req, U>
    where
        Res: CacheableResponse + Send + 'static,
        Res::Cached: Cacheable + Send,
        B: CacheBackend + Send + Sync + 'static,
        U: Upstream<Req, Response = Res>,
        C: ConcurrencyManager<Res>,
    {
        ctx.record_state(DebugState::PollCache);
        let cached = cache_result.unwrap_or_else(|_err| None);

        match cached {
            Some(cached_value) => {
                let cache_state = cached_value.cache_state();
                ctx.set_status(CacheStatus::Hit);

                match cache_state {
                    CacheState::Actual(value) => {
                        if ctx.read_mode() == ReadMode::Refill {
                            let cache_key = self.cache_key;
                            let update_cache_future = Box::pin(async move {
                                let update_result =
                                    backend.set::<Res>(&cache_key, &value, &mut ctx).await;
                                let response = Res::from_cached(value.into_inner()).await;
                                (update_result, response, ctx)
                            });
                            PollCacheTransition::UpdateCache {
                                update_cache_future,
                            }
                        } else {
                            let cache_key = self.cache_key;
                            let response_future = Box::pin(async move {
                                let response = Res::from_cached(value.into_inner()).await;
                                (response, ctx)
                            });
                            PollCacheTransition::ConvertResponse {
                                response_future,
                                cache_key,
                            }
                        }
                    }
                    CacheState::Stale(value) => {
                        let cache_key = self.cache_key;
                        let request = self.request;
                        let upstream = self.upstream;
                        let response_future = Box::pin(async move {
                            let response = Res::from_cached(value.into_inner()).await;
                            (response, ctx)
                        });
                        PollCacheTransition::HandleStale {
                            response_future,
                            request,
                            cache_key,
                            upstream,
                        }
                    }
                    CacheState::Expired(_value) => {
                        ctx.set_status(CacheStatus::Miss);
                        self.transition_to_upstream(ctx, policy, concurrency_manager)
                    }
                }
            }
            None => self.transition_to_upstream(ctx, policy, concurrency_manager),
        }
    }

    /// Helper to transition to upstream based on concurrency policy.
    fn transition_to_upstream<Res, C>(
        mut self,
        mut ctx: BoxContext,
        policy: &PolicyConfig,
        concurrency_manager: &C,
    ) -> PollCacheTransition<Res, Req, U>
    where
        Res: CacheableResponse,
        U: Upstream<Req, Response = Res>,
        C: ConcurrencyManager<Res>,
    {
        ctx.record_state(DebugState::ConcurrentPollUpstream);
        match policy {
            PolicyConfig::Enabled(EnabledCacheConfig {
                concurrency: Some(concurrency),
                ..
            }) => match concurrency_manager.check(&self.cache_key, *concurrency as usize) {
                ConcurrencyDecision::Proceed(permit) => {
                    let upstream_future = self.upstream.call(self.request);
                    PollCacheTransition::PollUpstream {
                        upstream_future,
                        permit: Some(permit),
                        ctx,
                        cache_key: self.cache_key,
                    }
                }
                ConcurrencyDecision::ProceedWithoutPermit => {
                    let upstream_future = self.upstream.call(self.request);
                    PollCacheTransition::PollUpstream {
                        upstream_future,
                        permit: None,
                        ctx,
                        cache_key: self.cache_key,
                    }
                }
                ConcurrencyDecision::Await(await_future) => PollCacheTransition::AwaitResponse {
                    await_response_future: await_future,
                    request: self.request,
                    ctx,
                    cache_key: self.cache_key,
                    upstream: self.upstream,
                },
            },
            _ => {
                let upstream_future = self.upstream.call(self.request);
                PollCacheTransition::PollUpstream {
                    upstream_future,
                    permit: None,
                    ctx,
                    cache_key: self.cache_key,
                }
            }
        }
    }
}

impl<Req, U> std::fmt::Debug for PollCache<Req, U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PollCache")
            .field("cache_key", &self.cache_key)
            .finish_non_exhaustive()
    }
}

// =============================================================================
// ConvertResponse
// =============================================================================

/// Data for ConvertResponse state (non-pinned part).
///
/// The response future is stored separately in the State enum to allow pinning.
/// When the future completes, this data is taken and passed to transition().
pub struct ConvertResponse {
    /// Cache key for logging/tracing purposes.
    pub cache_key: CacheKey,
}

impl ConvertResponse {
    /// Transition from ConvertResponse state after future completes.
    pub fn transition<Res>(
        self,
        response: Res,
        mut ctx: BoxContext,
    ) -> ConvertResponseTransition<Res> {
        ctx.record_state(DebugState::ConvertResponse);
        debug!(cache_key = ?self.cache_key, "ConvertResponse transition");
        ConvertResponseTransition::Response(Response { response, ctx })
    }
}

impl std::fmt::Debug for ConvertResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConvertResponse")
            .field("cache_key", &self.cache_key)
            .finish_non_exhaustive()
    }
}

// =============================================================================
// HandleStale
// =============================================================================

/// Data for HandleStale state (non-pinned part).
///
/// The response future is stored separately in the State enum to allow pinning.
/// When the future completes, this data is taken and passed to transition().
pub struct HandleStale<Req, U> {
    pub request: Req,
    pub cache_key: CacheKey,
    pub upstream: U,
}

/// Data needed for background offload revalidation.
pub struct OffloadData<Req, U> {
    pub request: Req,
    pub cache_key: CacheKey,
    pub upstream: U,
}

/// Result of HandleStale transition, including optional offload data.
pub struct HandleStaleResult<Res, Req, U>
where
    U: Upstream<Req, Response = Res>,
{
    pub transition: HandleStaleTransition<Res, Req, U>,
    pub offload_data: Option<OffloadData<Req, U>>,
}

impl<Req, U> HandleStale<Req, U> {
    /// Transition from HandleStale state after future completes.
    ///
    /// Returns a result containing the transition and optional offload data.
    /// For `StalePolicy::OffloadRevalidate`, the caller is responsible for spawning
    /// the background revalidation using the returned `offload_data`.
    pub fn transition<Res>(
        mut self,
        response: Res,
        mut ctx: BoxContext,
        policy: &PolicyConfig,
    ) -> HandleStaleResult<Res, Req, U>
    where
        Res: CacheableResponse,
        U: Upstream<Req, Response = Res>,
    {
        ctx.record_state(DebugState::HandleStale);

        let stale_policy = match policy {
            PolicyConfig::Enabled(EnabledCacheConfig { policy, .. }) => policy.stale,
            PolicyConfig::Disabled => StalePolicy::Return,
        };

        match stale_policy {
            StalePolicy::Return => {
                ctx.set_status(CacheStatus::Stale);
                HandleStaleResult {
                    transition: HandleStaleTransition::Response(Response { response, ctx }),
                    offload_data: None,
                }
            }
            StalePolicy::Revalidate => {
                ctx.set_status(CacheStatus::Miss);
                let upstream_future = self.upstream.call(self.request);
                HandleStaleResult {
                    transition: HandleStaleTransition::Revalidate {
                        upstream_future,
                        ctx,
                        cache_key: self.cache_key,
                    },
                    offload_data: None,
                }
            }
            StalePolicy::OffloadRevalidate => {
                ctx.set_status(CacheStatus::Stale);
                HandleStaleResult {
                    transition: HandleStaleTransition::Response(Response { response, ctx }),
                    offload_data: Some(OffloadData {
                        request: self.request,
                        cache_key: self.cache_key,
                        upstream: self.upstream,
                    }),
                }
            }
        }
    }
}

impl<Req, U> std::fmt::Debug for HandleStale<Req, U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandleStale")
            .field("cache_key", &self.cache_key)
            .finish_non_exhaustive()
    }
}

// =============================================================================
// AwaitResponse
// =============================================================================

/// Data for AwaitResponse state (non-pinned part).
///
/// The await response future is stored separately in the State enum to allow pinning.
/// When the future completes, this data is taken and passed to transition().
pub struct AwaitResponse<Req, U> {
    pub request: Req,
    pub ctx: BoxContext,
    pub cache_key: CacheKey,
    pub upstream: U,
}

impl<Req, U> AwaitResponse<Req, U> {
    /// Transition from AwaitResponse state after future completes.
    ///
    /// On success, returns the response directly.
    /// On concurrency error, falls back to calling upstream.
    pub fn transition<Res, C>(
        mut self,
        result: Result<Res, ConcurrencyError>,
        concurrency_manager: &C,
    ) -> AwaitResponseTransition<Res, Req, U>
    where
        Res: CacheableResponse,
        U: Upstream<Req, Response = Res>,
        C: ConcurrencyManager<Res>,
    {
        let mut ctx = self.ctx;
        ctx.record_state(DebugState::AwaitResponse);

        match result {
            Ok(response) => AwaitResponseTransition::Response(Response { response, ctx }),
            Err(ref concurrency_error) => {
                match concurrency_error {
                    ConcurrencyError::Lagged(n) => {
                        debug!(
                            "Concurrency channel lagged by {} messages, falling back to upstream",
                            n
                        );
                    }
                    ConcurrencyError::Closed => {
                        debug!(
                            "Concurrency channel closed, cleaning up stale entry and falling back to upstream"
                        );
                        concurrency_manager.cleanup(&self.cache_key);
                    }
                }

                let upstream_future = self.upstream.call(self.request);

                AwaitResponseTransition::PollUpstream {
                    upstream_future,
                    ctx,
                    cache_key: self.cache_key,
                }
            }
        }
    }
}

impl<Req, U> std::fmt::Debug for AwaitResponse<Req, U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwaitResponse")
            .field("cache_key", &self.cache_key)
            .finish_non_exhaustive()
    }
}

// =============================================================================
// UpdateCacheState
// =============================================================================

/// Resolved state after UpdateCache future completes.
pub struct UpdateCacheState<Res> {
    pub response: Res,
    pub ctx: BoxContext,
}

impl<Res> UpdateCacheState<Res> {
    /// Transition from UpdateCache resolved state.
    pub fn transition(mut self) -> UpdateCacheTransition<Res> {
        self.ctx.record_state(DebugState::UpdateCache);
        UpdateCacheTransition::Response(Response {
            response: self.response,
            ctx: self.ctx,
        })
    }
}

impl<Res> std::fmt::Debug for UpdateCacheState<Res> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateCacheState").finish_non_exhaustive()
    }
}
