use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::Utc;
use cucumber::World;
use hitbox::CacheContext;
use hitbox::concurrency::BroadcastConcurrencyManager;
use hitbox::fsm::CacheFuture;
use hitbox::policy::{EnabledCacheConfig, PolicyConfig};
use hitbox_backend::CacheBackend;
use hitbox_core::{
    CacheKey, CachePolicy, CacheValue, CacheablePolicyData, CacheableRequest, CacheableResponse,
    EntityPolicyConfig, Extractor, KeyPart, KeyParts, Predicate, PredicateResult,
    RequestCachePolicy, ResponseCachePolicy, Upstream,
};
use hitbox_moka::MokaBackend;

// =============================================================================
// Request / Response types
// =============================================================================

#[derive(Clone, Debug)]
pub struct SimpleRequest(pub u32);

#[async_trait::async_trait]
impl CacheableRequest for SimpleRequest {
    async fn cache_policy<P, E>(self, predicates: P, extractors: E) -> RequestCachePolicy<Self>
    where
        P: Predicate<Subject = Self> + Send + Sync,
        E: Extractor<Subject = Self> + Send + Sync,
    {
        match predicates.check(self).await {
            PredicateResult::Cacheable(request) => {
                let key_parts = extractors.get(request).await;
                let (request, cache_key) = key_parts.into_cache_key();
                RequestCachePolicy::Cacheable(CacheablePolicyData {
                    key: cache_key,
                    request,
                })
            }
            PredicateResult::NonCacheable(request) => RequestCachePolicy::NonCacheable(request),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SimpleResponse(pub u32);

#[async_trait::async_trait]
impl CacheableResponse for SimpleResponse {
    type Cached = u32;
    type Subject = SimpleResponse;

    async fn cache_policy<P>(
        self,
        predicates: P,
        _config: &EntityPolicyConfig,
    ) -> ResponseCachePolicy<Self>
    where
        P: Predicate<Subject = Self::Subject> + Send + Sync,
    {
        match predicates.check(self).await {
            PredicateResult::Cacheable(response) => {
                CachePolicy::Cacheable(CacheValue::new(response.0, None, None))
            }
            PredicateResult::NonCacheable(response) => CachePolicy::NonCacheable(response),
        }
    }

    async fn into_cached(self) -> CachePolicy<Self::Cached, Self> {
        CachePolicy::Cacheable(self.0)
    }

    async fn from_cached(cached: Self::Cached) -> Self {
        SimpleResponse(cached)
    }
}

// =============================================================================
// Configurable Predicates
// =============================================================================

#[derive(Debug, Clone, Copy)]
pub struct ConfigurableRequestPredicate {
    pub cacheable: bool,
}

#[async_trait::async_trait]
impl Predicate for ConfigurableRequestPredicate {
    type Subject = SimpleRequest;

    async fn check(&self, subject: Self::Subject) -> PredicateResult<Self::Subject> {
        if self.cacheable {
            PredicateResult::Cacheable(subject)
        } else {
            PredicateResult::NonCacheable(subject)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConfigurableResponsePredicate {
    pub cacheable: bool,
}

#[async_trait::async_trait]
impl Predicate for ConfigurableResponsePredicate {
    type Subject = SimpleResponse;

    async fn check(&self, subject: Self::Subject) -> PredicateResult<Self::Subject> {
        if self.cacheable {
            PredicateResult::Cacheable(subject)
        } else {
            PredicateResult::NonCacheable(subject)
        }
    }
}

// =============================================================================
// Extractor
// =============================================================================

#[derive(Debug)]
pub struct FixedKeyExtractor;

#[async_trait::async_trait]
impl Extractor for FixedKeyExtractor {
    type Subject = SimpleRequest;

    async fn get(&self, subject: Self::Subject) -> KeyParts<Self::Subject> {
        let mut key_parts = KeyParts::new(subject);
        key_parts.push(KeyPart::new("fixed_key", Some("value")));
        key_parts
    }
}

// =============================================================================
// Configurable Upstream
// =============================================================================

pub struct ConfigurableUpstream {
    pub call_count: Arc<AtomicUsize>,
    pub delay_ms: u64,
}

impl ConfigurableUpstream {
    pub fn new(call_count: Arc<AtomicUsize>, delay_ms: u64) -> Self {
        Self {
            call_count,
            delay_ms,
        }
    }
}

impl Upstream<SimpleRequest> for ConfigurableUpstream {
    type Response = SimpleResponse;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = SimpleResponse> + Send>>;

    fn call(&mut self, request: SimpleRequest) -> Self::Future {
        let call_count = self.call_count.clone();
        let delay_ms = self.delay_ms;
        let response_value = request.0;
        Box::pin(async move {
            call_count.fetch_add(1, Ordering::SeqCst);
            if delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
            SimpleResponse(response_value)
        })
    }
}

// =============================================================================
// FSM Configuration
// =============================================================================

#[derive(Debug, Clone, Default)]
pub struct FsmConfig {
    pub cache_enabled: bool,
    pub request_cacheable: bool,
    pub response_cacheable: bool,
    pub concurrency: Option<u8>,
    pub ttl: Option<u32>,
    pub stale: Option<u32>,
}

// =============================================================================
// Cache Pre-population State
// =============================================================================

#[derive(Debug, Clone, Default)]
pub enum CacheState {
    #[default]
    Empty,
    Fresh(u32),
    Stale(u32),
    Expired(u32),
}

// =============================================================================
// Test Results
// =============================================================================

#[derive(Debug, Default)]
pub struct TestResults {
    pub responses: Vec<(SimpleResponse, CacheContext)>,
    pub upstream_call_count: usize,
}

impl TestResults {
    pub fn upstream_call_count(&self) -> usize {
        self.upstream_call_count
    }

    pub fn all_responses_eq(&self, expected: u32) -> bool {
        self.responses.iter().all(|(r, _)| r.0 == expected)
    }

    /// Get FSM states from the first response context as strings.
    /// Only available when `fsm-trace` feature is enabled.
    #[cfg(feature = "fsm-trace")]
    pub fn fsm_states(&self) -> Option<Vec<String>> {
        self.responses
            .first()
            .map(|(_, ctx)| ctx.states.iter().map(|s| s.to_string()).collect())
    }

    /// Get FSM states for all responses as strings.
    /// Returns a vector of string vectors, one per response.
    /// Only available when `fsm-trace` feature is enabled.
    #[cfg(feature = "fsm-trace")]
    pub fn all_fsm_states(&self) -> Vec<Vec<String>> {
        self.responses
            .iter()
            .map(|(_, ctx)| ctx.states.iter().map(|s| s.to_string()).collect())
            .collect()
    }
}

// =============================================================================
// FsmWorld - Cucumber World for FSM tests
// =============================================================================

#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct FsmWorld {
    pub config: FsmConfig,
    pub cache_state: CacheState,
    pub upstream_delay_ms: u64,
    pub request_delay_ms: u64,
    pub results: TestResults,
    pub backend: MokaBackend,
}

impl FsmWorld {
    pub fn new() -> Self {
        Self {
            config: FsmConfig {
                cache_enabled: true,
                request_cacheable: true,
                response_cacheable: true,
                concurrency: Some(1),
                ttl: Some(60),
                stale: None,
            },
            cache_state: CacheState::Empty,
            upstream_delay_ms: 100,
            request_delay_ms: 10,
            results: TestResults::default(),
            backend: MokaBackend::builder(100).build(),
        }
    }

    pub async fn run_requests(&mut self, num_requests: usize, request_value: u32) {
        let upstream_call_count = Arc::new(AtomicUsize::new(0));

        let backend = Arc::new(self.backend.clone());
        let request_pred = Arc::new(ConfigurableRequestPredicate {
            cacheable: self.config.request_cacheable,
        });
        let response_pred = Arc::new(ConfigurableResponsePredicate {
            cacheable: self.config.response_cacheable,
        });
        let extractor = Arc::new(FixedKeyExtractor);
        let concurrency_manager = Arc::new(BroadcastConcurrencyManager::<SimpleResponse>::new());

        let policy = Arc::new(if self.config.cache_enabled {
            PolicyConfig::Enabled(EnabledCacheConfig {
                ttl: self.config.ttl,
                stale: self.config.stale,
                concurrency: self.config.concurrency,
                policy: Default::default(),
            })
        } else {
            PolicyConfig::Disabled
        });

        // Pre-populate cache if needed
        self.prepopulate_cache(&backend).await;

        let mut handles = Vec::new();

        for i in 0..num_requests {
            let backend = backend.clone();
            let request_pred = request_pred.clone();
            let response_pred = response_pred.clone();
            let extractor = extractor.clone();
            let policy = policy.clone();
            let concurrency_manager = concurrency_manager.clone();
            let upstream_call_count = upstream_call_count.clone();
            let upstream_delay_ms = self.upstream_delay_ms;
            let request_delay_ms = self.request_delay_ms;

            let handle = tokio::spawn(async move {
                if request_delay_ms > 0 && i > 0 {
                    tokio::time::sleep(Duration::from_millis(i as u64 * request_delay_ms)).await;
                }

                let upstream = ConfigurableUpstream::new(upstream_call_count, upstream_delay_ms);

                let cache_future = CacheFuture::new(
                    backend,
                    SimpleRequest(request_value),
                    upstream,
                    request_pred,
                    response_pred,
                    extractor,
                    policy,
                    None, // No offload manager for tests
                    concurrency_manager,
                );

                cache_future.await
            });

            handles.push(handle);
        }

        let responses: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.expect("Task should not panic"))
            .collect();

        self.results = TestResults {
            responses,
            upstream_call_count: upstream_call_count.load(Ordering::SeqCst),
        };
    }

    async fn prepopulate_cache(&self, backend: &Arc<MokaBackend>) {
        let mut ctx = CacheContext::default().boxed();
        match &self.cache_state {
            CacheState::Empty => {}
            CacheState::Fresh(value) => {
                let cache_key = CacheKey::from_str("fixed_key", "value");
                // Fresh: expires in the future
                let expire = Some(Utc::now() + chrono::Duration::hours(1));
                let cache_value = CacheValue::new(*value, expire, None);
                let _ = backend
                    .set::<SimpleResponse>(
                        &cache_key,
                        &cache_value,
                        Some(Duration::from_secs(3600)),
                        &mut ctx,
                    )
                    .await;
            }
            CacheState::Stale(value) => {
                let cache_key = CacheKey::from_str("fixed_key", "value");
                // Stale: expire is in the future, but stale threshold is in the past
                // This means: not expired yet, but past the "fresh" period
                let expire = Some(Utc::now() + chrono::Duration::hours(1));
                let stale = Some(Utc::now() - chrono::Duration::seconds(1));
                let cache_value = CacheValue::new(*value, expire, stale);
                let _ = backend
                    .set::<SimpleResponse>(
                        &cache_key,
                        &cache_value,
                        Some(Duration::from_secs(3600)),
                        &mut ctx,
                    )
                    .await;
            }
            CacheState::Expired(value) => {
                let cache_key = CacheKey::from_str("fixed_key", "value");
                // Expired: both TTL and stale period expired
                let expire = Some(Utc::now() - chrono::Duration::hours(1));
                let stale = Some(Utc::now() - chrono::Duration::minutes(30));
                let cache_value = CacheValue::new(*value, expire, stale);
                let _ = backend
                    .set::<SimpleResponse>(
                        &cache_key,
                        &cache_value,
                        Some(Duration::from_secs(3600)),
                        &mut ctx,
                    )
                    .await;
            }
        }
    }

    pub async fn cache_contains_value(&self, expected: u32) -> bool {
        let cache_key = CacheKey::from_str("fixed_key", "value");
        let mut ctx = CacheContext::default().boxed();
        if let Ok(Some(cached)) = self
            .backend
            .get::<SimpleResponse>(&cache_key, &mut ctx)
            .await
        {
            cached.into_inner() == expected
        } else {
            false
        }
    }

    pub async fn cache_is_empty(&self) -> bool {
        let cache_key = CacheKey::from_str("fixed_key", "value");
        let mut ctx = CacheContext::default().boxed();
        self.backend
            .get::<SimpleResponse>(&cache_key, &mut ctx)
            .await
            .ok()
            .flatten()
            .is_none()
    }
}

impl Default for FsmWorld {
    fn default() -> Self {
        Self::new()
    }
}
