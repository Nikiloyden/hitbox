//! Reference benchmark for CacheFuture with realistic predicates, extractors, and payloads.
//!
//! This benchmark represents a typical API caching scenario:
//! - Request with path parameters, query params, headers, and JSON body (~2.5KB)
//! - Response with paginated JSON data (~5KB)
//! - Multiple predicates (method, path, headers)
//! - Multiple extractors (method, path, query, header with hash)
//! - Moka backend with Bincode format
//! - Real CacheFuture state machine
//!
//! Run with: cargo bench -p hitbox-test --bench cache_future_reference

use std::future::Ready;
use std::sync::Arc;

use bytes::Bytes;
use criterion::{Criterion, criterion_group, criterion_main};
use hitbox::CacheableResponse;
use hitbox::fsm::CacheFuture;
use hitbox::policy::{EnabledCacheConfig, PolicyConfig};
use hitbox::predicate::Predicate;
use hitbox_backend::format::BincodeFormat;
use hitbox_backend::{CacheBackend, PassthroughCompressor};
use hitbox_core::Upstream;
use hitbox_http::extractors::NeutralExtractor;
use hitbox_http::extractors::body::{Body as BodyExtractor, BodyExtraction, JqExtraction};
use hitbox_http::extractors::header::{
    Header, NameSelector as HeaderNameSelector, ValueExtractor as HeaderValueExtractor,
};
use hitbox_http::extractors::method::MethodExtractor;
use hitbox_http::extractors::path::PathExtractor;
use hitbox_http::extractors::query::{
    NameSelector as QueryNameSelector, Query, ValueExtractor as QueryValueExtractor,
};
use hitbox_http::extractors::transform::Transform;
use hitbox_http::predicates::body::{
    BodyPredicate, JqExpression, JqOperation, Operation as BodyOperation,
};
use hitbox_http::predicates::header::HeaderPredicate;
use hitbox_http::predicates::request::header::Operation as HeaderOperation;
use hitbox_http::predicates::request::method::MethodPredicate;
use hitbox_http::predicates::request::path::PathPredicate;
use hitbox_http::predicates::response::header::Operation as ResponseHeaderOperation;
use hitbox_http::predicates::response::status::{StatusClass, StatusCodePredicate};
use hitbox_http::predicates::{NeutralRequestPredicate, NeutralResponsePredicate};
use hitbox_http::{BufferedBody, CacheableHttpRequest, CacheableHttpResponse};
use hitbox_moka::MokaBackend;
use http::{Method, Request, Response};
use http_body_util::Empty;
use serde_json::json;

// Inner body type (what's inside BufferedBody)
type InnerBody = Empty<Bytes>;
// Request body type (BufferedBody wrapping the inner)
type ReqBody = BufferedBody<InnerBody>;
// Cacheable request/response types
type BenchRequest = CacheableHttpRequest<InnerBody>;
type BenchResponse = CacheableHttpResponse<InnerBody>;

// ============================================================================
// Fixture loading
// ============================================================================

const REQUEST_BODY: &str = include_str!("fixtures/reference_request.json");
const RESPONSE_BODY: &str = include_str!("fixtures/reference_response.json");

// ============================================================================
// Mock Upstream
// ============================================================================

/// Mock upstream that returns a fresh response each time
struct MockUpstream;

impl Upstream<BenchRequest> for MockUpstream {
    type Response = BenchResponse;
    type Future = Ready<Self::Response>;

    fn call(&mut self, _req: BenchRequest) -> Self::Future {
        // Create a fresh response each time (no Clone needed)
        std::future::ready(CacheableHttpResponse::from_response(
            create_reference_response(),
        ))
    }
}

// ============================================================================
// Test data creation
// ============================================================================

/// Create a realistic API request with path params, query params, headers, and body
fn create_reference_request() -> Request<ReqBody> {
    Request::builder()
        .method(Method::POST)
        .uri("http://api.example.com/v1/users/12345/orders?include=items,customer&fields=id,status,total&expand=shipping")
        .header("content-type", "application/json")
        .header("authorization", "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U")
        .header("x-tenant-id", "tenant-abc-123")
        .header("x-request-id", "req-550e8400-e29b-41d4-a716-446655440000")
        .header("x-idempotency-key", "idem-key-unique-12345")
        .header("accept", "application/json")
        .header("accept-language", "en-US,en;q=0.9")
        .header("user-agent", "ApiClient/2.0 (Production)")
        .body(BufferedBody::Complete(Some(Bytes::from(REQUEST_BODY))))
        .unwrap()
}

/// Create a realistic API JSON response
fn create_reference_response() -> Response<ReqBody> {
    Response::builder()
        .status(200)
        .header("content-type", "application/json; charset=utf-8")
        .header("cache-control", "public, max-age=300")
        .header("x-request-id", "req-550e8400-e29b-41d4-a716-446655440000")
        .header("x-response-time", "42ms")
        .header("x-ratelimit-limit", "1000")
        .header("x-ratelimit-remaining", "999")
        .body(BufferedBody::Complete(Some(Bytes::from(RESPONSE_BODY))))
        .unwrap()
}

// ============================================================================
// Predicate and Extractor setup
// ============================================================================

/// Create reference request predicates (method + path pattern + required headers) - static dispatch
fn create_request_predicates() -> impl Predicate<Subject = BenchRequest> + Send + Sync {
    NeutralRequestPredicate::new()
        .method(Method::POST)
        .path("/v1/users/{user_id}/orders".to_string())
        .header(HeaderOperation::Exist("authorization".parse().unwrap()))
        .header(HeaderOperation::Exist("x-tenant-id".parse().unwrap()))
        .header(HeaderOperation::Exist("x-idempotency-key".parse().unwrap()))
}

/// Create reference response predicates (cache successful 2xx responses) - static dispatch
fn create_response_predicates()
-> impl Predicate<Subject = <BenchResponse as CacheableResponse>::Subject> + Send + Sync {
    NeutralResponsePredicate::<InnerBody>::new()
        // Only cache 2xx successful responses
        .status_code_class(StatusClass::Success)
        // Require content-type header
        .header(ResponseHeaderOperation::Exist(
            "content-type".parse().unwrap(),
        ))
        // Require cache-control header (indicates response is cacheable)
        .header(ResponseHeaderOperation::Exist(
            "cache-control".parse().unwrap(),
        ))
}

/// Create reference extractors (method + path + query + headers with transforms) - static dispatch
fn create_extractors() -> impl hitbox::Extractor<Subject = BenchRequest> + Send + Sync {
    let base = NeutralExtractor::<InnerBody>::new();

    // Method extractor
    let with_method = base.method();

    // Path extractor (extracts user_id)
    let with_path = with_method.path("/v1/users/{user_id}/orders");

    // Query extractors
    let with_query1 = Query::new(
        with_path,
        QueryNameSelector::Exact("include".to_string()),
        QueryValueExtractor::Full,
        Vec::new(),
    );
    let with_query2 = Query::new(
        with_query1,
        QueryNameSelector::Exact("fields".to_string()),
        QueryValueExtractor::Full,
        Vec::new(),
    );

    // Header: tenant-id (plain)
    let with_tenant = Header::new(
        with_query2,
        HeaderNameSelector::Exact("x-tenant-id".to_string()),
        HeaderValueExtractor::Full,
        Vec::new(),
    );

    // Header: authorization with hash transform (for privacy in cache key)
    let with_auth = Header::new(
        with_tenant,
        HeaderNameSelector::Exact("authorization".to_string()),
        HeaderValueExtractor::Full,
        vec![Transform::Hash],
    );

    // Header: idempotency key
    Header::new(
        with_auth,
        HeaderNameSelector::Exact("x-idempotency-key".to_string()),
        HeaderValueExtractor::Full,
        Vec::new(),
    )
}

/// Create cache policy (enabled with 5 min TTL)
fn create_policy() -> Arc<PolicyConfig> {
    Arc::new(PolicyConfig::Enabled(EnabledCacheConfig {
        ttl: Some(300),
        stale: None,
        policy: Default::default(),
    }))
}

/// Extend request predicates with body jq predicate
fn create_request_predicates_with_body() -> impl Predicate<Subject = BenchRequest> + Send + Sync {
    let jq_filter = JqExpression::compile(".order.customer_id").unwrap();
    create_request_predicates().body(BodyOperation::Jq {
        filter: jq_filter,
        operation: JqOperation::Exist,
    })
}

/// Extend response predicates with body jq predicate
fn create_response_predicates_with_body()
-> impl Predicate<Subject = <BenchResponse as CacheableResponse>::Subject> + Send + Sync {
    let jq_filter = JqExpression::compile(".data | type").unwrap();
    create_response_predicates().body(BodyOperation::Jq {
        filter: jq_filter,
        operation: JqOperation::Eq(json!("array")),
    })
}

/// Extend extractors with body jq extractor
fn create_extractors_with_body() -> impl hitbox::Extractor<Subject = BenchRequest> + Send + Sync {
    let jq_extraction = JqExtraction::compile(
        "{customer_id: .order.customer_id, shipping: .order.shipping_method}",
    )
    .unwrap();
    BodyExtractor::new(create_extractors(), BodyExtraction::Jq(jq_extraction))
}

// ============================================================================
// Benchmarks
// ============================================================================

/// Benchmark request predicates evaluation only
fn bench_request_predicates(c: &mut Criterion) {
    let mut group = c.benchmark_group("reference");
    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("request_predicates", |b| {
        let predicates = create_request_predicates();
        b.to_async(&rt).iter(|| async {
            let request = CacheableHttpRequest::from_request(create_reference_request());
            std::hint::black_box(predicates.check(request).await)
        });
    });

    group.finish();
}

/// Benchmark response predicates evaluation only
fn bench_response_predicates(c: &mut Criterion) {
    let mut group = c.benchmark_group("reference");
    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("response_predicates", |b| {
        let predicates = create_response_predicates();
        b.to_async(&rt).iter(|| async {
            let response = CacheableHttpResponse::from_response(create_reference_response());
            std::hint::black_box(predicates.check(response).await)
        });
    });

    group.finish();
}

/// Benchmark extractors only
fn bench_extractors(c: &mut Criterion) {
    use hitbox::Extractor;

    let mut group = c.benchmark_group("reference");
    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("extractors", |b| {
        let extractors = create_extractors();
        b.to_async(&rt).iter(|| async {
            let request = CacheableHttpRequest::from_request(create_reference_request());
            std::hint::black_box(extractors.get(request).await)
        });
    });

    group.finish();
}

/// Benchmark cache operations with reference key and response
fn bench_cache_operations(c: &mut Criterion) {
    use hitbox::Extractor;
    use hitbox_core::{CacheContext, CacheValue};
    use std::time::Duration;

    let mut group = c.benchmark_group("reference");
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Setup backend
    let backend = MokaBackend::builder(10000)
        .value_format(BincodeFormat)
        .compressor(PassthroughCompressor)
        .build();

    // Generate cache key using extractors
    let extractors = create_extractors();
    let request = CacheableHttpRequest::from_request(create_reference_request());
    let (_, cache_key) = rt.block_on(async { extractors.get(request).await.into_cache_key() });

    // Generate cacheable response
    let response = CacheableHttpResponse::from_response(create_reference_response());
    let cached_response = rt.block_on(async {
        match response.into_cached().await {
            hitbox::CachePolicy::Cacheable(cached) => cached,
            hitbox::CachePolicy::NonCacheable(_) => panic!("Response should be cacheable"),
        }
    });
    let cache_value = CacheValue::new(cached_response, None, None);

    // Pre-populate cache for read benchmark
    rt.block_on(async {
        let mut ctx = CacheContext::default().boxed();
        backend
            .set::<BenchResponse>(
                &cache_key,
                &cache_value,
                Some(Duration::from_secs(300)),
                &mut ctx,
            )
            .await
            .unwrap();
    });

    // Cache write benchmark
    let backend_write = backend.clone();
    let key_write = cache_key.clone();
    let value_write = cache_value.clone();
    group.bench_function("cache_write", |b| {
        b.to_async(&rt).iter(|| async {
            let mut ctx = CacheContext::default().boxed();
            backend_write
                .set::<BenchResponse>(
                    &key_write,
                    &value_write,
                    Some(Duration::from_secs(300)),
                    &mut ctx,
                )
                .await
                .unwrap();
        });
    });

    // Cache read benchmark
    let backend_read = backend.clone();
    let key_read = cache_key.clone();
    group.bench_function("cache_read", |b| {
        b.to_async(&rt).iter(|| async {
            let mut ctx = CacheContext::default().boxed();
            std::hint::black_box(
                backend_read
                    .get::<BenchResponse>(&key_read, &mut ctx)
                    .await
                    .unwrap(),
            );
        });
    });

    group.finish();
}

/// Benchmark full CacheFuture flow (real state machine)
/// Note: CacheFuture requires Arc<dyn ...> for predicates/extractors
fn bench_cache_future(c: &mut Criterion) {
    use hitbox::Extractor;
    use hitbox_core::{CacheContext, CacheValue};
    use std::time::Duration;

    let mut group = c.benchmark_group("reference");
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Setup backend
    let backend = Arc::new(
        MokaBackend::builder(10000)
            .value_format(BincodeFormat)
            .compressor(PassthroughCompressor)
            .build(),
    );

    // CacheFuture requires Arc<dyn ...> - this is the production API
    let request_predicates: Arc<dyn Predicate<Subject = BenchRequest> + Send + Sync> =
        Arc::new(create_request_predicates());
    let response_predicates: Arc<
        dyn Predicate<Subject = <BenchResponse as CacheableResponse>::Subject> + Send + Sync,
    > = Arc::new(create_response_predicates());
    let extractors: Arc<dyn hitbox::Extractor<Subject = BenchRequest> + Send + Sync> =
        Arc::new(create_extractors());
    let policy = create_policy();

    // Pre-populate cache for HIT scenario
    let request = CacheableHttpRequest::from_request(create_reference_request());
    let (_, cache_key) = rt.block_on(async { extractors.get(request).await.into_cache_key() });

    let response = CacheableHttpResponse::from_response(create_reference_response());
    let cached_response = rt.block_on(async {
        match response.into_cached().await {
            hitbox::CachePolicy::Cacheable(cached) => cached,
            hitbox::CachePolicy::NonCacheable(_) => panic!("Response should be cacheable"),
        }
    });
    let cache_value = CacheValue::new(cached_response, None, None);

    rt.block_on(async {
        let mut ctx = CacheContext::default().boxed();
        backend
            .set::<BenchResponse>(
                &cache_key,
                &cache_value,
                Some(Duration::from_secs(300)),
                &mut ctx,
            )
            .await
            .unwrap();
    });

    // Cache HIT: Full CacheFuture flow with cache hit
    let backend_hit = backend.clone();
    let req_pred_hit = request_predicates.clone();
    let res_pred_hit = response_predicates.clone();
    let ext_hit = extractors.clone();
    let policy_hit = policy.clone();

    group.bench_function("cache_future_hit", |b| {
        b.to_async(&rt).iter(|| {
            let backend = backend_hit.clone();
            let req_pred = req_pred_hit.clone();
            let res_pred = res_pred_hit.clone();
            let ext = ext_hit.clone();
            let policy = policy_hit.clone();

            async move {
                let request = CacheableHttpRequest::from_request(create_reference_request());
                let upstream = MockUpstream;

                let cache_future = CacheFuture::new(
                    backend, request, upstream, req_pred, res_pred, ext, policy,
                    None, // no offload manager
                );

                std::hint::black_box(cache_future.await)
            }
        });
    });

    // Cache MISS: Full CacheFuture flow with cache miss (unique keys)
    let backend_miss = backend.clone();
    let req_pred_miss = request_predicates.clone();
    let res_pred_miss = response_predicates.clone();
    let ext_miss = extractors.clone();
    let policy_miss = policy.clone();

    group.bench_function("cache_future_miss", |b| {
        let mut counter = 0u64;
        b.to_async(&rt).iter(|| {
            let backend = backend_miss.clone();
            let req_pred = req_pred_miss.clone();
            let res_pred = res_pred_miss.clone();
            let ext = ext_miss.clone();
            let policy = policy_miss.clone();
            let unique_id = counter;
            counter = counter.wrapping_add(1);

            async move {
                // Create request with unique user_id to cause cache miss
                let request: Request<ReqBody> = Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "http://api.example.com/v1/users/{}/orders?include=items,customer&fields=id,status,total&expand=shipping",
                        unique_id
                    ))
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer token")
                    .header("x-tenant-id", "tenant-abc-123")
                    .header("x-idempotency-key", format!("idem-{}", unique_id))
                    .body(BufferedBody::Complete(Some(Bytes::from(REQUEST_BODY))))
                    .unwrap();
                let request = CacheableHttpRequest::from_request(request);
                let upstream = MockUpstream;

                let cache_future = CacheFuture::new(
                    backend,
                    request,
                    upstream,
                    req_pred,
                    res_pred,
                    ext,
                    policy,
                    None,
                );

                std::hint::black_box(cache_future.await)
            }
        });
    });

    group.finish();
}

// ============================================================================
// Body Benchmarks (with jq predicates/extractors)
// ============================================================================

/// Benchmark request predicates with body jq evaluation
fn bench_body_request_predicates(c: &mut Criterion) {
    let mut group = c.benchmark_group("body");
    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("request_predicates", |b| {
        let predicates = create_request_predicates_with_body();
        b.to_async(&rt).iter(|| async {
            let request = CacheableHttpRequest::from_request(create_reference_request());
            std::hint::black_box(predicates.check(request).await)
        });
    });

    group.finish();
}

/// Benchmark response predicates with body jq evaluation
fn bench_body_response_predicates(c: &mut Criterion) {
    let mut group = c.benchmark_group("body");
    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("response_predicates", |b| {
        let predicates = create_response_predicates_with_body();
        b.to_async(&rt).iter(|| async {
            let response = CacheableHttpResponse::from_response(create_reference_response());
            std::hint::black_box(predicates.check(response).await)
        });
    });

    group.finish();
}

/// Benchmark extractors with body jq extraction
fn bench_body_extractors(c: &mut Criterion) {
    use hitbox::Extractor;

    let mut group = c.benchmark_group("body");
    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("extractors", |b| {
        let extractors = create_extractors_with_body();
        b.to_async(&rt).iter(|| async {
            let request = CacheableHttpRequest::from_request(create_reference_request());
            std::hint::black_box(extractors.get(request).await)
        });
    });

    group.finish();
}

/// Benchmark full CacheFuture flow with body predicates/extractors
fn bench_body_cache_future(c: &mut Criterion) {
    use hitbox::Extractor;
    use hitbox_core::{CacheContext, CacheValue};
    use std::time::Duration;

    let mut group = c.benchmark_group("body");
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Setup backend
    let backend = Arc::new(
        MokaBackend::builder(10000)
            .value_format(BincodeFormat)
            .compressor(PassthroughCompressor)
            .build(),
    );

    // Use body predicates/extractors
    let request_predicates: Arc<dyn Predicate<Subject = BenchRequest> + Send + Sync> =
        Arc::new(create_request_predicates_with_body());
    let response_predicates: Arc<
        dyn Predicate<Subject = <BenchResponse as CacheableResponse>::Subject> + Send + Sync,
    > = Arc::new(create_response_predicates_with_body());
    let extractors: Arc<dyn hitbox::Extractor<Subject = BenchRequest> + Send + Sync> =
        Arc::new(create_extractors_with_body());
    let policy = create_policy();

    // Pre-populate cache for HIT scenario
    let request = CacheableHttpRequest::from_request(create_reference_request());
    let (_, cache_key) = rt.block_on(async { extractors.get(request).await.into_cache_key() });

    let response = CacheableHttpResponse::from_response(create_reference_response());
    let cached_response = rt.block_on(async {
        match response.into_cached().await {
            hitbox::CachePolicy::Cacheable(cached) => cached,
            hitbox::CachePolicy::NonCacheable(_) => panic!("Response should be cacheable"),
        }
    });
    let cache_value = CacheValue::new(cached_response, None, None);

    rt.block_on(async {
        let mut ctx = CacheContext::default().boxed();
        backend
            .set::<BenchResponse>(
                &cache_key,
                &cache_value,
                Some(Duration::from_secs(300)),
                &mut ctx,
            )
            .await
            .unwrap();
    });

    // Cache HIT with body predicates/extractors
    let backend_hit = backend.clone();
    let req_pred_hit = request_predicates.clone();
    let res_pred_hit = response_predicates.clone();
    let ext_hit = extractors.clone();
    let policy_hit = policy.clone();

    group.bench_function("cache_future_hit", |b| {
        b.to_async(&rt).iter(|| {
            let backend = backend_hit.clone();
            let req_pred = req_pred_hit.clone();
            let res_pred = res_pred_hit.clone();
            let ext = ext_hit.clone();
            let policy = policy_hit.clone();

            async move {
                let request = CacheableHttpRequest::from_request(create_reference_request());
                let upstream = MockUpstream;

                let cache_future = CacheFuture::new(
                    backend, request, upstream, req_pred, res_pred, ext, policy, None,
                );

                std::hint::black_box(cache_future.await)
            }
        });
    });

    // Cache MISS with body predicates/extractors
    let backend_miss = backend.clone();
    let req_pred_miss = request_predicates.clone();
    let res_pred_miss = response_predicates.clone();
    let ext_miss = extractors.clone();
    let policy_miss = policy.clone();

    group.bench_function("cache_future_miss", |b| {
        let mut counter = 0u64;
        b.to_async(&rt).iter(|| {
            let backend = backend_miss.clone();
            let req_pred = req_pred_miss.clone();
            let res_pred = res_pred_miss.clone();
            let ext = ext_miss.clone();
            let policy = policy_miss.clone();
            let unique_id = counter;
            counter = counter.wrapping_add(1);

            async move {
                let request: Request<ReqBody> = Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "http://api.example.com/v1/users/{}/orders?include=items,customer&fields=id,status,total&expand=shipping",
                        unique_id
                    ))
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer token")
                    .header("x-tenant-id", "tenant-abc-123")
                    .header("x-idempotency-key", format!("idem-{}", unique_id))
                    .body(BufferedBody::Complete(Some(Bytes::from(REQUEST_BODY))))
                    .unwrap();
                let request = CacheableHttpRequest::from_request(request);
                let upstream = MockUpstream;

                let cache_future = CacheFuture::new(
                    backend, request, upstream, req_pred, res_pred, ext, policy,
                    None,
                );

                std::hint::black_box(cache_future.await)
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_request_predicates,
    bench_response_predicates,
    bench_extractors,
    bench_cache_operations,
    bench_cache_future,
    // Body benchmarks
    bench_body_request_predicates,
    bench_body_response_predicates,
    bench_body_extractors,
    bench_body_cache_future
);
criterion_main!(benches);
