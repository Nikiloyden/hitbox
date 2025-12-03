use std::{
    fmt::Debug,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{self, Poll},
    time::Instant,
};

use crate::{
    CacheContext, CacheStatus, CacheableResponse, ResponseSource, offload::OffloadManager,
};
use futures::ready;
use hitbox_core::{Cacheable, DebugState, Upstream};
use pin_project::pin_project;
use tracing::debug;

use crate::{
    CacheKey, CacheableRequest, Extractor, Predicate,
    backend::CacheBackend,
    concurrency::{ConcurrencyManager, NoopConcurrencyManager},
    fsm::states::{self, PollUpstream, State, StateProj},
};

const POLL_AFTER_READY_ERROR: &str = "CacheFuture can't be polled after finishing";

#[pin_project(project = CacheFutureProj)]
pub struct CacheFuture<B, Req, Res, U, ReqP, ResP, E, C>
where
    U: Upstream<Req, Response = Res>,
    B: CacheBackend,
    Res: CacheableResponse,
    Req: CacheableRequest,
    ReqP: Predicate<Subject = Req> + Send + Sync,
    ResP: Predicate<Subject = Res::Subject> + Send + Sync,
    E: Extractor<Subject = Req> + Send + Sync,
    C: ConcurrencyManager<Res>,
{
    backend: Arc<B>,
    cache_key: Option<CacheKey>,
    #[pin]
    state: State<Res, Req, U, ReqP, E>,
    response_predicates: Option<ResP>,
    policy: Arc<crate::policy::PolicyConfig>,
    /// Optional offload manager for background revalidation (SWR).
    offload_manager: Option<OffloadManager>,
    /// Whether this is a background revalidation task.
    is_revalidation: bool,
    concurrency_manager: C,
    /// Start time for latency measurement.
    start_time: Instant,
}

impl<B, Req, Res, U, ReqP, ResP, E, C> CacheFuture<B, Req, Res, U, ReqP, ResP, E, C>
where
    U: Upstream<Req, Response = Res>,
    B: CacheBackend,
    Res: CacheableResponse,
    Req: CacheableRequest,
    ReqP: Predicate<Subject = Req> + Send + Sync,
    ResP: Predicate<Subject = Res::Subject> + Send + Sync,
    E: Extractor<Subject = Req> + Send + Sync,
    C: ConcurrencyManager<Res>,
{
    pub fn new(
        backend: Arc<B>,
        request: Req,
        upstream: U,
        request_predicates: ReqP,
        response_predicates: ResP,
        key_extractors: E,
        policy: Arc<crate::policy::PolicyConfig>,
        offload_manager: Option<OffloadManager>,
        concurrency_manager: C,
    ) -> Self {
        let initial_state = states::Initial {
            request,
            predicates: request_predicates,
            extractors: key_extractors,
            ctx: CacheContext::default().boxed(),
            upstream,
        };
        CacheFuture {
            backend,
            cache_key: None,
            state: State::Initial(Some(initial_state)),
            response_predicates: Some(response_predicates),
            policy,
            offload_manager,
            is_revalidation: false,
            concurrency_manager,
            start_time: Instant::now(),
        }
    }
}

impl<B, Req, Res, U, ReqP, ResP, E>
    CacheFuture<B, Req, Res, U, ReqP, ResP, E, NoopConcurrencyManager>
where
    U: Upstream<Req, Response = Res>,
    U::Future: Send + 'static,
    B: CacheBackend,
    Res: CacheableResponse,
    Req: CacheableRequest,
    ReqP: Predicate<Subject = Req> + Send + Sync,
    ResP: Predicate<Subject = Res::Subject> + Send + Sync,
    E: Extractor<Subject = Req> + Send + Sync,
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
    /// * `response_predicates` - Predicates to check if response should be cached
    /// * `policy` - Cache policy configuration (TTL, stale TTL)
    ///
    /// Note: `request_predicates` and `key_extractors` are not needed for revalidation
    /// since the FSM starts at `PollUpstream` state, skipping the initial request check.
    pub fn revalidate(
        backend: Arc<B>,
        cache_key: CacheKey,
        request: Req,
        mut upstream: U,
        response_predicates: ResP,
        policy: Arc<crate::policy::PolicyConfig>,
    ) -> Self {
        let upstream_future = upstream.call(request);

        CacheFuture {
            backend,
            cache_key: Some(cache_key.clone()),
            state: State::PollUpstream {
                upstream_future,
                state: Some(PollUpstream {
                    permit: None,
                    ctx: CacheContext::default().boxed(),
                    cache_key: Some(cache_key),
                }),
            },
            response_predicates: Some(response_predicates),
            policy,
            // Revalidation tasks don't spawn further revalidation
            offload_manager: None,
            is_revalidation: true,
            // Revalidation tasks don't need concurrency control
            concurrency_manager: NoopConcurrencyManager,
            start_time: Instant::now(),
        }
    }
}

impl<'pin, B, Req, Res, U, ReqP, ResP, E, C> CacheFutureProj<'pin, B, Req, Res, U, ReqP, ResP, E, C>
where
    U: Upstream<Req, Response = Res> + Send + 'static,
    U::Future: Send + 'static,
    B: CacheBackend + Send + Sync + 'static,
    Res: CacheableResponse + Send,
    Res::Cached: Cacheable + Send + Debug,
    Req: CacheableRequest + Send + Debug + 'static,
    ReqP: Predicate<Subject = Req> + Send + Sync + 'static,
    ResP: Predicate<Subject = Res::Subject> + Send + Sync + 'static,
    E: Extractor<Subject = Req> + Send + Sync + 'static,
    C: ConcurrencyManager<Res>,
{
    /// Spawns a background revalidation task if offload_manager is available.
    fn spawn_revalidation(&mut self, offload_data: states::OffloadData<Req, U>) {
        if let Some(offload_manager) = self.offload_manager.as_ref() {
            if let Some(response_predicates) = self.response_predicates.take() {
                let revalidation_future = CacheFuture::<
                    B,
                    Req,
                    Res,
                    U,
                    ReqP,
                    ResP,
                    E,
                    NoopConcurrencyManager,
                >::revalidate(
                    self.backend.clone(),
                    offload_data.cache_key.clone(),
                    offload_data.request,
                    offload_data.upstream,
                    response_predicates,
                    self.policy.clone(),
                );

                offload_manager.spawn_with_key(offload_data.cache_key, async move {
                    let (_response, ctx) = revalidation_future.await;
                    debug!(
                        status = ?ctx.status,
                        source = ?ctx.source,
                        "Revalidation completed"
                    );
                });
            }
        } else {
            tracing::warn!(
                "StalePolicy::OffloadRevalidate is configured but \
                 OffloadManager is not provided. \
                 Falling back to returning stale data without revalidation."
            );
        }
    }
}

impl<B, Req, Res, U, ReqP, ResP, E, C> Future for CacheFuture<B, Req, Res, U, ReqP, ResP, E, C>
where
    U: Upstream<Req, Response = Res> + Send + 'static,
    U::Future: Send + 'static,
    B: CacheBackend + Send + Sync + 'static,
    Res: CacheableResponse + Send,
    Res::Cached: Cacheable + Send,
    Req: CacheableRequest + Send + 'static,
    ReqP: Predicate<Subject = Req> + Send + Sync + 'static,
    ResP: Predicate<Subject = Res::Subject> + Send + Sync + 'static,
    E: Extractor<Subject = Req> + Send + Sync + 'static,
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
                StateProj::Initial(initial_state) => {
                    let initial = initial_state.take().expect(POLL_AFTER_READY_ERROR);
                    initial.transition(this.policy.as_ref()).into_state()
                }
                StateProj::CheckRequestCachePolicy {
                    cache_policy_future,
                    state,
                } => {
                    let policy = ready!(cache_policy_future.poll(cx));
                    let check_state = state.take().expect(POLL_AFTER_READY_ERROR);

                    check_state
                        .transition(policy, this.backend.clone(), this.cache_key)
                        .into_state()
                }
                StateProj::PollCache { poll_cache, state } => {
                    let (cache_result, ctx) = ready!(poll_cache.poll(cx));
                    let poll_cache_state = state.take().expect(POLL_AFTER_READY_ERROR);

                    poll_cache_state
                        .transition(
                            cache_result,
                            ctx,
                            this.backend.clone(),
                            this.policy.as_ref(),
                            &*this.concurrency_manager,
                        )
                        .into_state()
                }
                StateProj::AwaitResponse {
                    await_response_future,
                    state,
                } => {
                    let result = ready!(await_response_future.poll(cx));
                    let await_response_state = state.take().expect(POLL_AFTER_READY_ERROR);

                    await_response_state
                        .transition(result, &*this.concurrency_manager)
                        .into_state()
                }
                StateProj::ConvertResponse {
                    response_future,
                    state,
                } => {
                    let (response, ctx) = ready!(response_future.poll(cx));
                    let convert_response_state = state.take().expect(POLL_AFTER_READY_ERROR);
                    convert_response_state
                        .transition(response, ctx)
                        .into_state()
                }
                StateProj::HandleStale {
                    response_future,
                    state,
                } => {
                    let (response, ctx) = ready!(response_future.poll(cx));
                    let handle_stale_state = state.take().expect(POLL_AFTER_READY_ERROR);

                    let result = handle_stale_state.transition(response, ctx, this.policy.as_ref());

                    // Handle offload revalidation if requested
                    if let Some(offload_data) = result.offload_data {
                        this.spawn_revalidation(offload_data);
                    }

                    result.transition.into_state()
                }
                StateProj::PollUpstream {
                    upstream_future,
                    state,
                } => {
                    let upstream_result = ready!(upstream_future.poll(cx));
                    let poll_upstream = state.take().expect(POLL_AFTER_READY_ERROR);
                    let predicates = this
                        .response_predicates
                        .take()
                        .expect("Response predicates already taken");

                    poll_upstream
                        .transition(upstream_result, predicates, this.policy.as_ref())
                        .into_state()
                }
                StateProj::CheckResponseCachePolicy {
                    cache_policy,

                    state,
                } => {
                    let policy = ready!(cache_policy.poll(cx));
                    let check_state = state.take().expect(POLL_AFTER_READY_ERROR);

                    check_state
                        .transition(policy, this.backend.clone(), &*this.concurrency_manager)
                        .into_state()
                }
                StateProj::UpdateCache {
                    update_cache_future,
                } => {
                    // TODO: check backend result
                    let (_backend_result, response, ctx) = ready!(update_cache_future.poll(cx));
                    states::UpdateCacheState { response, ctx }
                        .transition()
                        .into_state()
                }
                StateProj::Response(response_state) => {
                    let mut state = response_state.take().expect(POLL_AFTER_READY_ERROR);
                    state.ctx.record_state(DebugState::Response);
                    // For cache miss, set source to Upstream.
                    // For hit/stale, the backend has already set the correct source.
                    if state.ctx.status() == CacheStatus::Miss {
                        state.ctx.set_source(ResponseSource::Upstream);
                    }
                    let ctx = hitbox_core::finalize_context(state.ctx);
                    let duration = this.start_time.elapsed();
                    crate::metrics::record_context_metrics(&ctx, duration, *this.is_revalidation);
                    return Poll::Ready((state.response, ctx));
                }
            };
            debug!("{:?}", &state);
            this.state.set(state);
        }
    }
}
