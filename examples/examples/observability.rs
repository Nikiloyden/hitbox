//! Example demonstrating Hitbox with full observability stack:
//! - OpenTelemetry tracing to Jaeger
//! - Prometheus metrics endpoint
//! - Tower Cache layer integration
//! - Different cache configurations per route
//!
//! Prerequisites:
//!   docker compose -f docker-compose.observability.yml up -d
//!
//! Run:
//!   cargo run --example observability --features observability
//!
//! Endpoints:
//!   - http://localhost:3002/             - Root (long TTL: 60s)
//!   - http://localhost:3002/greet/{name} - Greeting with path-based cache key (short TTL: 10s)
//!   - http://localhost:3002/health       - Health check (caching disabled)
//!   - http://localhost:3002/metrics      - Prometheus metrics
//!
//! Try it:
//!   curl http://localhost:3002/              # Cache miss, then hit (60s TTL)
//!   curl http://localhost:3002/greet/world   # Cache miss (different key per name)
//!   curl http://localhost:3002/greet/world   # Cache hit!
//!   curl http://localhost:3002/greet/claude  # Cache miss (different name = different key)
//!   curl http://localhost:3002/health        # Always fresh (caching disabled)
//!
//! View traces:
//!   - Jaeger UI: http://localhost:16686
//!   - Prometheus: http://localhost:9090
//!   - Grafana: http://localhost:3000 (admin/admin)

use std::time::Duration;

use axum::{Router, routing::get};
use hitbox::concurrency::NoopConcurrencyManager;
use hitbox_configuration::ConfigEndpoint;
use hitbox_moka::MokaBackend;
use hitbox_tower::Cache;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::{
    Resource, runtime,
    trace::{RandomIdGenerator, Sampler, TracerProvider},
};
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{Level, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize OpenTelemetry tracing with OTLP exporter to Jaeger
fn init_tracing() -> Result<TracerProvider, Box<dyn std::error::Error>> {
    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()?;

    let tracer_provider = TracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_sampler(Sampler::AlwaysOn)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(Resource::new(vec![opentelemetry::KeyValue::new(
            SERVICE_NAME,
            "hitbox-observability",
        )]))
        .build();

    let tracer = tracer_provider.tracer("hitbox-observability");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_level(true),
        )
        .with(otel_layer)
        .with(
            tracing_subscriber::filter::Targets::new()
                .with_target("hitbox", Level::TRACE)
                .with_target("hitbox_tower", Level::TRACE)
                .with_target("hitbox_backend", Level::TRACE)
                .with_target("hitbox_moka", Level::TRACE)
                .with_target("tower_http", Level::DEBUG)
                .with_target("observability", Level::DEBUG)
                .with_default(Level::INFO),
        )
        .init();

    Ok(tracer_provider)
}

/// Initialize Prometheus metrics recorder
fn init_metrics() -> PrometheusHandle {
    const EXPONENTIAL_SECONDS: &[f64] = &[
        0.000_001, 0.000_01, 0.000_1, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5,
        5.0, 10.0,
    ];

    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("hitbox_request_duration_seconds".to_string()),
            EXPONENTIAL_SECONDS,
        )
        .expect("Failed to set buckets")
        .set_buckets_for_metric(
            Matcher::Full("hitbox_upstream_duration_seconds".to_string()),
            EXPONENTIAL_SECONDS,
        )
        .expect("Failed to set buckets")
        .set_buckets_for_metric(
            Matcher::Full("hitbox_backend_read_duration_seconds".to_string()),
            EXPONENTIAL_SECONDS,
        )
        .expect("Failed to set buckets")
        .set_buckets_for_metric(
            Matcher::Full("hitbox_backend_write_duration_seconds".to_string()),
            EXPONENTIAL_SECONDS,
        )
        .expect("Failed to set buckets")
        .install_recorder()
        .expect("Failed to install Prometheus recorder")
}

// Handler that simulates some work
#[tracing::instrument]
async fn root_handler() -> &'static str {
    tokio::time::sleep(Duration::from_millis(50)).await;
    "Hello from cached root! (TTL: 60s)"
}

#[tracing::instrument]
async fn greet_handler(axum::extract::Path(name): axum::extract::Path<String>) -> String {
    tokio::time::sleep(Duration::from_millis(30)).await;
    info!("greet handler: {name}");
    format!("Hello, {name}! (TTL: 10s)")
}

#[tracing::instrument]
async fn health_handler() -> &'static str {
    "OK"
}

// Metrics endpoint handler
async fn metrics_handler(
    axum::extract::State(handle): axum::extract::State<PrometheusHandle>,
) -> String {
    handle.render()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize observability
    let tracer_provider = init_tracing()?;
    let metrics_handle = init_metrics();

    info!("Starting Hitbox observability example");
    info!("Jaeger UI: http://localhost:16686");
    info!("Prometheus: http://localhost:9090");
    info!("Grafana: http://localhost:3000 (admin/admin)");
    info!("Metrics endpoint: http://localhost:3002/metrics");
    info!("App endpoint: http://localhost:3002/");

    // Create Moka in-memory cache backend
    let backend = MokaBackend::builder(1024 * 1024).build();

    // Root cache - long TTL (60s), simple cache key by method only
    let root_config = serde_saphyr::from_str::<ConfigEndpoint>(
        r"
        request:
          - Method: GET
          - Path: /
        extractors:
          - Method: {}
          - Path: /
        policy:
          Enabled:
            ttl: 60
            stale: 30
        ",
    )?
    .into_endpoint()?;

    // Greet cache - short TTL (10s), cache key includes path parameter
    let greet_config = serde_saphyr::from_str::<ConfigEndpoint>(
        r"
        request:
          - Method: GET
          - Path: /greet/{name}
        extractors:
          - Method: {}
          - Path: /greet/{name}
        policy:
          Enabled:
            ttl: 10
            stale: 5
        ",
    )?
    .into_endpoint()?;

    // Health check - caching disabled
    let health_config = serde_saphyr::from_str::<ConfigEndpoint>(
        r"
        request:
          - Method: GET
          - Path: /health
        policy: Disabled
        ",
    )?
    .into_endpoint()?;

    // Build cache layers with different configurations (concurrency manager disabled)
    let root_cache = Cache::builder()
        .backend(backend.clone())
        .config(root_config)
        .concurrency_manager(NoopConcurrencyManager)
        .build();

    let greet_cache = Cache::builder()
        .backend(backend.clone())
        .config(greet_config)
        .concurrency_manager(NoopConcurrencyManager)
        .build();

    let health_cache = Cache::builder()
        .backend(backend)
        .config(health_config)
        .concurrency_manager(NoopConcurrencyManager)
        .build();

    // Build router with different cache layers per route
    let app = Router::new()
        .route("/", get(root_handler).layer(root_cache))
        .route("/greet/{name}", get(greet_handler).layer(greet_cache))
        .route("/health", get(health_handler).layer(health_cache))
        .route("/metrics", get(metrics_handler).with_state(metrics_handle))
        .layer(TraceLayer::new_for_http());

    let listener = TcpListener::bind("0.0.0.0:3002").await?;
    info!("Listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    // Shutdown OpenTelemetry
    tracer_provider.shutdown()?;

    Ok(())
}
