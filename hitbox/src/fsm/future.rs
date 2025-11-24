use std::{
    fmt::Debug,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{self, Poll},
    time::Duration,
};

use crate::{
    CacheContext, CachePolicy, CacheState, CacheStatus, CacheableResponse, ResponseSource,
    offload::OffloadManager,
    policy::{EnabledCacheConfig, PolicyConfig, StalePolicy},
};
use futures::ready;
use hitbox_core::{Cacheable, CacheablePolicyData, EntityPolicyConfig, Upstream};
use pin_project::pin_project;
use tracing::debug;

use crate::{
    CacheKey, CacheableRequest, Extractor, Predicate,
    backend::CacheBackend,
    concurrency::{ConcurrencyDecision, ConcurrencyManager, NoopConcurrencyManager},
    fsm::{PollCacheFuture, State, states::StateProj},
};

const POLL_AFTER_READY_ERROR: &str = "CacheFuture can't be polled after finishing";
const CONTEXT_TAKEN_ERROR: &str = "Context already taken from state";
const UPSTREAM_TAKEN_ERROR: &str = "Upstream already taken (used for offload revalidation)";

#[pin_project]
pub struct CacheFuture<B, Req, Res, U, C>
where
    U: Upstream<Req, Response = Res>,
    B: CacheBackend,
    Res: CacheableResponse,
    Req: CacheableRequest,
    C: ConcurrencyManager<Res>,
{
    upstream: Option<U>,
    backend: Arc<B>,
    request: Option<Req>,
    cache_key: Option<CacheKey>,
    #[pin]
    state: State<Res, Req>,
    #[pin]
    poll_cache: Option<PollCacheFuture<Res>>,
    request_predicates: Arc<dyn Predicate<Subject = Req> + Send + Sync>,
    response_predicates: Arc<dyn Predicate<Subject = Res::Subject> + Send + Sync>,
    key_extractors: Arc<dyn Extractor<Subject = Req> + Send + Sync>,
    policy: Arc<crate::policy::PolicyConfig>,
    /// Optional offload manager for background revalidation (SWR).
    offload_manager: Option<OffloadManager>,
    /// Whether this is a background revalidation task.
    is_revalidation: bool,
    concurrency_manager: Arc<C>,
}

impl<B, Req, Res, U, C> CacheFuture<B, Req, Res, U, C>
where
    U: Upstream<Req, Response = Res>,
    B: CacheBackend,
    Res: CacheableResponse,
    Req: CacheableRequest,
    C: ConcurrencyManager<Res>,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        backend: Arc<B>,
        request: Req,
        upstream: U,
        request_predicates: Arc<dyn Predicate<Subject = Req> + Send + Sync>,
        response_predicates: Arc<dyn Predicate<Subject = Res::Subject> + Send + Sync>,
        key_extractors: Arc<dyn Extractor<Subject = Req> + Send + Sync>,
        policy: Arc<crate::policy::PolicyConfig>,
        offload_manager: Option<OffloadManager>,
        concurrency_manager: Arc<C>,
    ) -> Self {
        CacheFuture {
            upstream: Some(upstream),
            backend,
            cache_key: None,
            request: Some(request),
            state: State::Initial {
                ctx: Some(CacheContext::default().boxed()),
            },
            poll_cache: None,
            request_predicates,
            response_predicates,
            key_extractors,
            policy,
            offload_manager,
            is_revalidation: false,
            concurrency_manager,
        }
    }
}

impl<B, Req, Res, U> CacheFuture<B, Req, Res, U, NoopConcurrencyManager>
where
    U: Upstream<Req, Response = Res>,
    U::Future: Send + 'static,
    B: CacheBackend,
    Res: CacheableResponse,
    Req: CacheableRequest,
{
    /// Create a CacheFuture for background revalidation (Stale-While-Revalidate).
    ///
    /// This constructor initializes the FSM at `PollUpstream` state, skipping
    /// the cache lookup phase. Use this when you want to refresh a stale cache
    /// entry in the background.
    ///
    /// # Arguments
    /// * `backend` - Cache backend for storing the refreshed value
    /// * `cache_key` - Key to update in the cache
    /// * `request` - Request to send to upstream
    /// * `upstream` - Upstream service to call
    /// * `request_predicates` - Request predicates (not used, required for type consistency)
    /// * `response_predicates` - Predicates to check if response should be cached
    /// * `key_extractors` - Key extractors (not used, required for type consistency)
    /// * `policy` - Cache policy configuration (TTL, stale TTL)
    pub fn revalidate(
        backend: Arc<B>,
        cache_key: CacheKey,
        request: Req,
        mut upstream: U,
        request_predicates: Arc<dyn Predicate<Subject = Req> + Send + Sync>,
        response_predicates: Arc<dyn Predicate<Subject = Res::Subject> + Send + Sync>,
        key_extractors: Arc<dyn Extractor<Subject = Req> + Send + Sync>,
        policy: Arc<crate::policy::PolicyConfig>,
    ) -> Self {
        let upstream_future = Box::pin(upstream.call(request));

        CacheFuture {
            upstream: Some(upstream),
            backend,
            cache_key: Some(cache_key),
            request: None,
            state: State::PollUpstream {
                upstream_future,
                permit: None,
                ctx: Some(CacheContext::default().boxed()),
            },
            poll_cache: None,
            request_predicates,
            response_predicates,
            key_extractors,
            policy,
            // Revalidation tasks don't spawn further revalidations
            offload_manager: None,
            is_revalidation: true,
            // Revalidation tasks don't need concurrency control
            concurrency_manager: Arc::new(NoopConcurrencyManager),
        }
    }
}

impl<B, Req, Res, U, C> Future for CacheFuture<B, Req, Res, U, C>
where
    U: Upstream<Req, Response = Res> + Send + 'static,
    U::Future: Send + 'static,
    B: CacheBackend + Send + Sync + 'static,
    Res: CacheableResponse + Send,
    Res::Cached: Cacheable + Send,
    Req: CacheableRequest + Send + 'static,
    C: ConcurrencyManager<Res> + 'static,
    // Debug bounds
    Req: Debug,
    Res::Cached: Debug,
{
    type Output = (Res, CacheContext);

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            let state = match this.state.as_mut().project() {
                StateProj::Initial { ctx } => {
                    let predicates = this.request_predicates.clone();
                    let extractors = this.key_extractors.clone();
                    let request = this.request.take().expect(POLL_AFTER_READY_ERROR);
                    let ctx = ctx.take().expect(CONTEXT_TAKEN_ERROR);
                    match this.policy.as_ref() {
                        PolicyConfig::Enabled(_) => {
                            let cache_policy_future = Box::pin(async move {
                                request.cache_policy(predicates, extractors).await
                            });
                            State::CheckRequestCachePolicy {
                                cache_policy_future,
                                ctx: Some(ctx),
                            }
                        }
                        PolicyConfig::Disabled => {
                            let upstream = this.upstream.as_mut().expect(UPSTREAM_TAKEN_ERROR);
                            let upstream_future = Box::pin(upstream.call(request));
                            State::PollUpstream {
                                upstream_future,
                                permit: None,
                                ctx: Some(ctx),
                            }
                        }
                    }
                }
                StateProj::CheckRequestCachePolicy {
                    cache_policy_future,
                    ctx,
                } => {
                    let policy = ready!(cache_policy_future.poll(cx));
                    let mut ctx = ctx.take().expect(CONTEXT_TAKEN_ERROR);
                    match policy {
                        CachePolicy::Cacheable(CacheablePolicyData { key, request }) => {
                            let backend = this.backend.clone();
                            let cache_key = key.clone();
                            debug!(?cache_key, "FSM looking up cache key");
                            let _ = this.cache_key.insert(key);
                            let poll_cache = Box::pin(async move {
                                let result = backend.get::<Res>(&cache_key, &mut ctx).await;
                                debug!(
                                    found = result.as_ref().map(|r| r.is_some()).unwrap_or(false),
                                    "FSM cache lookup result"
                                );
                                (result, ctx)
                            });
                            State::PollCache {
                                poll_cache,
                                request: Some(request),
                            }
                        }
                        CachePolicy::NonCacheable(request) => {
                            let upstream = this.upstream.as_mut().expect(UPSTREAM_TAKEN_ERROR);
                            let upstream_future = Box::pin(upstream.call(request));
                            State::PollUpstream {
                                upstream_future,
                                permit: None,
                                ctx: Some(ctx),
                            }
                        }
                    }
                }
                StateProj::PollCache {
                    poll_cache,
                    request,
                } => {
                    let (cache_result, ctx) = ready!(poll_cache.poll(cx));
                    let cached = cache_result.unwrap_or_else(|_err| {
                        //println!("cache backend error: {err}");
                        None
                    });
                    match cached {
                        Some(cached_value) => State::CheckCacheState {
                            cache_state: Box::pin(cached_value.cache_state()),
                            request: request.take(),
                            ctx: Some(ctx),
                        },
                        None => State::CheckConcurrency {
                            request: request.take(),
                            ctx: Some(ctx),
                        },
                    }
                }
                StateProj::CheckConcurrency { request, ctx } => {
                    let request = request.take().expect(POLL_AFTER_READY_ERROR);
                    let ctx = ctx.take().expect(CONTEXT_TAKEN_ERROR);
                    match this.policy.as_ref() {
                        PolicyConfig::Enabled(crate::policy::EnabledCacheConfig {
                            concurrency: Some(concurrency),
                            ..
                        }) => State::ConcurrentPollUpstream {
                            request: Some(request),
                            concurrency: *concurrency as usize,
                            ctx: Some(ctx),
                        },
                        _ => {
                            let upstream = this.upstream.as_mut().expect(UPSTREAM_TAKEN_ERROR);
                            let upstream_future = Box::pin(upstream.call(request));
                            State::PollUpstream {
                                upstream_future,
                                permit: None,
                                ctx: Some(ctx),
                            }
                        }
                    }
                }
                StateProj::ConcurrentPollUpstream {
                    request,
                    concurrency,
                    ctx,
                } => {
                    let request = request.take().expect(POLL_AFTER_READY_ERROR);
                    let concurrency = *concurrency;
                    let ctx = ctx.take().expect(CONTEXT_TAKEN_ERROR);
                    let cache_key = this
                        .cache_key
                        .as_ref()
                        .expect("CacheKey not found for concurrency check");
                    match this.concurrency_manager.check(cache_key, concurrency) {
                        ConcurrencyDecision::Proceed(permit) => {
                            let upstream = this.upstream.as_mut().expect(UPSTREAM_TAKEN_ERROR);
                            let upstream_future = Box::pin(upstream.call(request));
                            State::PollUpstream {
                                upstream_future,
                                permit: Some(permit),
                                ctx: Some(ctx),
                            }
                        }
                        ConcurrencyDecision::ProceedWithoutPermit => {
                            let upstream = this.upstream.as_mut().expect(UPSTREAM_TAKEN_ERROR);
                            let upstream_future = Box::pin(upstream.call(request));
                            State::PollUpstream {
                                upstream_future,
                                permit: None,
                                ctx: Some(ctx),
                            }
                        }
                        ConcurrencyDecision::Await(await_future) => State::AwaitResponse {
                            await_response_future: await_future,
                            request: Some(request),
                            ctx: Some(ctx),
                        },
                    }
                }
                StateProj::AwaitResponse {
                    await_response_future,
                    request,
                    ctx,
                } => {
                    let result = ready!(await_response_future.poll(cx));
                    match result {
                        Ok(response) => {
                            // Successfully received response from concurrent request
                            State::Response {
                                response: Some(response),
                                ctx: ctx.take(),
                            }
                        }
                        Err(concurrency_error) => {
                            // Concurrency error (Lagged or Closed)
                            // Fallback to direct upstream call and cache the result normally
                            let ctx = ctx.take().expect(CONTEXT_TAKEN_ERROR);
                            match &concurrency_error {
                                crate::concurrency::ConcurrencyError::Lagged(n) => {
                                    debug!(
                                        "Concurrency channel lagged by {} messages, falling back to upstream",
                                        n
                                    );
                                }
                                crate::concurrency::ConcurrencyError::Closed => {
                                    debug!(
                                        "Concurrency channel closed, cleaning up stale entry and falling back to upstream"
                                    );
                                    // Cleanup stale entry from in-flight map (permit holder likely crashed)
                                    if let Some(cache_key) = this.cache_key.as_ref() {
                                        this.concurrency_manager.cleanup(cache_key);
                                    }
                                }
                            }

                            let request = request.take().expect(POLL_AFTER_READY_ERROR);
                            let upstream_future = Box::pin(this.upstream.call(request));
                            State::PollUpstream {
                                upstream_future,
                                permit: None,
                                ctx: Some(ctx),
                            }
                        }
                    }
                }
                StateProj::CheckCacheState {
                    cache_state,
                    request,
                    ctx,
                } => {
                    let state = ready!(cache_state.as_mut().poll(cx));
                    // Set status on the context while it's still in the Option
                    ctx.as_mut()
                        .expect(CONTEXT_TAKEN_ERROR)
                        .set_status(CacheStatus::Hit);
                    match state {
                        CacheState::Actual(response) => State::Response {
                            response: Some(response),
                            ctx: ctx.take(),
                        },
                        CacheState::Stale(response) => {
                            let stale_policy = match this.policy.as_ref() {
                                PolicyConfig::Enabled(EnabledCacheConfig { policy, .. }) => {
                                    policy.stale
                                }
                                PolicyConfig::Disabled => StalePolicy::Return,
                            };

                            match stale_policy {
                                StalePolicy::Return => {
                                    // Just return stale data, no revalidation
                                    ctx.as_mut()
                                        .expect(CONTEXT_TAKEN_ERROR)
                                        .set_status(CacheStatus::Stale);
                                    State::Response {
                                        response: Some(response),
                                        ctx: ctx.take(),
                                    }
                                }
                                StalePolicy::Revalidate => {
                                    // Treat stale as expired - block and wait for fresh data
                                    let mut ctx = ctx.take().expect(CONTEXT_TAKEN_ERROR);
                                    ctx.set_status(CacheStatus::Miss);
                                    let upstream =
                                        this.upstream.as_mut().expect(UPSTREAM_TAKEN_ERROR);
                                    let upstream_future = Box::pin(
                                        upstream
                                            .call(request.take().expect(POLL_AFTER_READY_ERROR)),
                                    );
                                    State::PollUpstream {
                                        upstream_future,
                                        permit: None,
                                        ctx: Some(ctx),
                                    }
                                }
                                StalePolicy::OffloadRevalidate => {
                                    // Return stale data immediately, spawn background revalidation
                                    match (this.offload_manager.as_ref(), this.cache_key.clone()) {
                                        (Some(offload_manager), Some(cache_key)) => {
                                            if let (Some(req), Some(upstream)) =
                                                (request.take(), this.upstream.take())
                                            {
                                                let revalidation_future = CacheFuture::revalidate(
                                                    this.backend.clone(),
                                                    cache_key.clone(),
                                                    req,
                                                    upstream,
                                                    this.request_predicates.clone(),
                                                    this.response_predicates.clone(),
                                                    this.key_extractors.clone(),
                                                    this.policy.clone(),
                                                );

                                                offload_manager.spawn_with_key(
                                                    cache_key,
                                                    async move {
                                                        let (_response, ctx) =
                                                            revalidation_future.await;
                                                        debug!(
                                                            status = ?ctx.status,
                                                            source = ?ctx.source,
                                                            "Revalidation completed"
                                                        );
                                                    },
                                                );
                                            }
                                        }
                                        (None, _) => {
                                            tracing::warn!(
                                                "StalePolicy::OffloadRevalidate is configured but \
                                                 OffloadManager is not provided. \
                                                 Falling back to returning stale data without revalidation."
                                            );
                                        }
                                        (_, None) => {
                                            tracing::warn!(
                                                "StalePolicy::OffloadRevalidate is configured but \
                                                 cache_key is not available. \
                                                 Falling back to returning stale data without revalidation."
                                            );
                                        }
                                    }

                                    ctx.as_mut()
                                        .expect(CONTEXT_TAKEN_ERROR)
                                        .set_status(CacheStatus::Stale);
                                    State::Response {
                                        response: Some(response),
                                        ctx: ctx.take(),
                                    }
                                }
                            }
                        }
                        CacheState::Expired(_response) => {
                            ctx.as_mut()
                                .expect(CONTEXT_TAKEN_ERROR)
                                .set_status(CacheStatus::Miss);
                            State::CheckConcurrency {
                                request: request.take(),
                                ctx: ctx.take(),
                            }
                        }
                    }
                }
                StateProj::PollUpstream {
                    upstream_future,
                    permit,
                    ctx,
                } => {
                    let res = ready!(upstream_future.as_mut().poll(cx));
                    State::UpstreamPolled {
                        upstream_result: Some(res),
                        permit: permit.take(),
                        ctx: ctx.take(),
                    }
                }
                StateProj::UpstreamPolled {
                    upstream_result,
                    permit,
                    ctx,
                } => {
                    let upstream_result = upstream_result.take().expect(POLL_AFTER_READY_ERROR);
                    let permit = permit.take();
                    let predicates = this.response_predicates.clone();
                    let ctx = ctx.take().expect(CONTEXT_TAKEN_ERROR);
                    match this.cache_key {
                        Some(_cache_key) => {
                            let entity_config = match this.policy.as_ref() {
                                PolicyConfig::Enabled(config) => EntityPolicyConfig {
                                    ttl: config.ttl.map(|s| Duration::from_secs(s as u64)),
                                    stale_ttl: config.stale.map(|s| Duration::from_secs(s as u64)),
                                },
                                PolicyConfig::Disabled => EntityPolicyConfig::default(),
                            };
                            State::CheckResponseCachePolicy {
                                cache_policy: Box::pin(async move {
                                    upstream_result
                                        .cache_policy(predicates, &entity_config)
                                        .await
                                }),
                                permit,
                                ctx: Some(ctx),
                            }
                        }
                        None => State::Response {
                            response: Some(upstream_result),
                            ctx: Some(ctx),
                        },
                    }
                }
                StateProj::CheckResponseCachePolicy {
                    cache_policy,
                    permit,
                    ctx,
                } => {
                    // Permit drops here naturally
                    let policy = ready!(cache_policy.poll(cx));
                    let backend = this.backend.clone();
                    let cache_key = this.cache_key.take().expect("CacheKey not found");
                    let permit = permit.take();
                    let mut ctx = ctx.take().expect(CONTEXT_TAKEN_ERROR);
                    match policy {
                        CachePolicy::Cacheable(cache_value) => {
                            // Only resolve if we have a permit (we're the winner of the race)
                            if permit.is_some() {
                                this.concurrency_manager.resolve(&cache_key, &cache_value);
                            }
                            let update_cache_future = Box::pin(async move {
                                let update_cache_result = backend
                                    .set::<Res>(&cache_key, &cache_value, None, &mut ctx)
                                    .await;
                                let upstream_result =
                                    Res::from_cached(cache_value.into_inner()).await;
                                (update_cache_result, upstream_result, ctx)
                            });
                            State::UpdateCache {
                                update_cache_future,
                            }
                        }
                        CachePolicy::NonCacheable(response) => {
                            // Close the channel if we have a permit (permit holder cleans up)
                            // This allows waiters to fall back to independent upstream calls
                            if permit.is_some() {
                                this.concurrency_manager.cleanup(&cache_key);
                            }
                            State::Response {
                                response: Some(response),
                                ctx: Some(ctx),
                            }
                        }
                    }
                }
                StateProj::UpdateCache {
                    update_cache_future,
                } => {
                    // TODO: check backend result
                    let (_backend_result, upstream_result, ctx) =
                        ready!(update_cache_future.poll(cx));
                    State::Response {
                        response: Some(upstream_result),
                        ctx: Some(ctx),
                    }
                }
                StateProj::Response { response, ctx } => {
                    let upstream_response = response.take().expect(POLL_AFTER_READY_ERROR);
                    let ctx_ref = ctx.as_mut().expect(CONTEXT_TAKEN_ERROR);
                    let source = match ctx_ref.status() {
                        CacheStatus::Hit | CacheStatus::Stale => {
                            // TODO: get backend name from backend instance
                            ResponseSource::Backend("unknown".into())
                        }
                        CacheStatus::Miss => ResponseSource::Upstream,
                    };
                    ctx_ref.set_source(source);
                    let ctx = ctx.take().expect(CONTEXT_TAKEN_ERROR);
                    let ctx = ctx.into_cache_context();
                    let (operation, revalidate) = if *this.is_revalidation {
                        ("revalidate", true)
                    } else {
                        ("request", false)
                    };
                    crate::metrics::record_context_metrics(&ctx, operation, revalidate);
                    return Poll::Ready((upstream_response, ctx));
                }
            };
            debug!("{:?}", &state);
            this.state.set(state);
        }
    }
}
