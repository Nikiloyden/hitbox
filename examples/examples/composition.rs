//! Composition Backend Example
//!
//! Demonstrates a 3-tier cache hierarchy using composable backends.
//!
//! Cache layers:
//! - L1: Moka (in-memory) - fastest, limited capacity
//! - L2: FeOxDB (file-based) - persistent, local storage
//! - L3: Redis (distributed) - shared across instances
//!
//! Features shown:
//! - Composing multiple backends with fluent API
//! - Refill policies (Always) to populate upper tiers on cache miss
//! - OffloadManager for background operations
//! - Labels for observability and debugging
//!
//! Prerequisites:
//!   Redis server running on localhost:6379
//!
//! Run:
//!   cargo run -p hitbox-examples --example composition
//!
//! Endpoints:
//!   - http://localhost:3000/ - Hello World (cached, TTL: 60s)
//!
//! Try it:
//!   curl -v http://localhost:3000/   # First: miss on all tiers, populates cache
//!   curl -v http://localhost:3000/   # Second: hit on L1 (Moka)

use std::time::Duration;

use axum::{Router, routing::get};
use hitbox::offload::OffloadManager;
use hitbox::policy::PolicyConfig;
use hitbox_backend::composition::{Compose, policy::RefillPolicy};
use hitbox_configuration::Endpoint;
use hitbox_http::extractors::{Method as MethodExtractor, path::PathExtractor};
use hitbox_tower::Cache;
use tempfile::TempDir;

async fn hello() -> &'static str {
    tracing::info!("Handler called - fetching from upstream");
    "Hello, World!"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info,hitbox=debug")
        .init();

    // L1: Moka (in-memory)
    let moka = hitbox_moka::MokaBackend::builder(1024 * 1024)
        .label("moka")
        .build();

    // L2: FeOxDB (file-based)
    let temp_dir = TempDir::new().unwrap();
    let feoxdb = hitbox_feoxdb::FeOxDbBackend::open(temp_dir.path()).unwrap();

    // L3: Redis (distributed)
    let redis = hitbox_redis::RedisBackend::builder()
        .connection(hitbox_redis::ConnectionMode::single("redis://127.0.0.1/"))
        .label("redis")
        .build()
        .unwrap();

    // Compose: Moka → FeOxDB → Redis
    let offload = OffloadManager::with_defaults();

    let local = moka
        .compose(feoxdb, offload.clone())
        .label("local")
        .refill(RefillPolicy::Always);

    let composed = local
        .compose(redis, offload)
        .label("cache")
        .refill(RefillPolicy::Always);

    let config = Endpoint::builder()
        .extractor(MethodExtractor::new().path("/"))
        .policy(PolicyConfig::builder().ttl(Duration::from_secs(60)).build())
        .build();

    let cache = Cache::builder().backend(composed).config(config).build();

    let app = Router::new().route("/", get(hello).layer(cache));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("Listening on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}
