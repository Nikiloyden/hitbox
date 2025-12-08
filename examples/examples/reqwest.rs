//! Example of using hitbox-reqwest with reqwest-middleware.
//!
//! This example demonstrates how to add caching to a reqwest client
//! using the hitbox caching framework with Moka in-memory backend.

use hitbox_configuration::ConfigEndpoint;
use hitbox_moka::MokaBackend;
use hitbox_reqwest::CacheMiddleware;
use reqwest::Client;
use reqwest_middleware::ClientBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for debugging
    tracing_subscriber::fmt()
        .with_env_filter("hitbox=debug")
        .init();

    // Create a Moka in-memory backend
    let backend = MokaBackend::builder(1000).build();

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

    // Create the cache middleware using builder pattern
    let cache_middleware = CacheMiddleware::builder()
        .backend(backend)
        .config(config)
        .build();

    // Build the client with middleware
    let client = ClientBuilder::new(Client::new())
        .with(cache_middleware)
        .build();

    // GitHub API for hitbox repo (requires User-Agent header)
    let url = "https://api.github.com/repos/hit-box/hitbox";

    println!("=== First request (cache miss) ===");
    let response = client
        .get(url)
        .header("User-Agent", "hitbox-example/1.0")
        .send()
        .await?;
    println!("Status: {}", response.status());
    println!(
        "X-Cache-Status: {:?}",
        response.headers().get("X-Cache-Status")
    );
    let body = response.text().await?;
    println!("Body length: {} bytes", body.len());

    println!("\n=== Second request (should be cache hit) ===");
    let response = client
        .get(url)
        .header("User-Agent", "hitbox-example/1.0")
        .send()
        .await?;
    println!("Status: {}", response.status());
    println!(
        "X-Cache-Status: {:?}",
        response.headers().get("X-Cache-Status")
    );
    let body = response.text().await?;
    println!("Body length: {} bytes", body.len());

    Ok(())
}
