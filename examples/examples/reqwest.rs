//! Example of using hitbox-reqwest with reqwest-middleware.
//!
//! This example demonstrates how to add caching to a reqwest client
//! using the hitbox caching framework with Redis backend and RON serialization.

use std::sync::Arc;

use hitbox_backend::CacheKeyFormat;
use hitbox_backend::format::RonFormat;
use hitbox_configuration::ConfigEndpoint;
use hitbox_redis::RedisBackend;
use hitbox_reqwest::CacheMiddleware;
use reqwest::Client;
use reqwest_middleware::ClientBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create a Redis backend with RON serialization format and URL-encoded keys
    let backend = Arc::new(
        RedisBackend::builder()
            .server("redis://127.0.0.1/")
            .value_format(RonFormat)
            .key_format(CacheKeyFormat::UrlEncoded)
            .build()?,
    );

    // Configure cache endpoint using YAML configuration
    let config_yaml = r#"
    request:
    - Method: GET
    extractors:
    - Method: {}
    - Path: "/{path}*"
    policy:
      Enabled:
        ttl: 60s
    "#;

    let config = serde_saphyr::from_str::<ConfigEndpoint>(config_yaml)
        .expect("Failed to parse config")
        .into_endpoint()
        .expect("Failed to create endpoint");

    // Create the cache middleware
    let cache_middleware = CacheMiddleware::new(backend, config);

    // Build the client with middleware
    let client = ClientBuilder::new(Client::new())
        .with(cache_middleware)
        .build();

    println!("=== First request (cache miss) ===");
    let response = client.get("https://httpbin.org/get").send().await?;
    println!("Status: {}", response.status());
    println!(
        "X-Cache-Status: {:?}",
        response.headers().get("X-Cache-Status")
    );
    println!("Body: {}", response.text().await?);

    println!("\n=== Second request (should be cache hit) ===");
    let response = client.get("https://httpbin.org/get").send().await?;
    println!("Status: {}", response.status());
    println!(
        "X-Cache-Status: {:?}",
        response.headers().get("X-Cache-Status")
    );
    println!("Body: {}", response.text().await?);

    Ok(())
}
