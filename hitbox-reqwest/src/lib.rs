//! Hitbox cache integration for reqwest HTTP client via reqwest-middleware.
//!
//! This crate provides a middleware for `reqwest-middleware` that adds caching
//! capabilities using the hitbox caching framework. It reuses `hitbox-http` types
//! for request/response handling and predicates.
//!
//! # Example (Builder Pattern)
//!
//! ```ignore
//! use reqwest::Client;
//! use reqwest_middleware::ClientBuilder;
//! use hitbox_reqwest::CacheMiddleware;
//! use hitbox_configuration::ConfigEndpoint;
//! use hitbox_moka::MokaBackend;
//!
//! let config = ConfigEndpoint::default().into_endpoint().unwrap();
//!
//! // Using the builder pattern (defaults to NoopConcurrencyManager)
//! let middleware = CacheMiddleware::builder()
//!     .backend(MokaBackend::builder(1000).build())
//!     .config(config)
//!     .build();
//!
//! let client = ClientBuilder::new(Client::new())
//!     .with(middleware)
//!     .build();
//!
//! // All GET requests will be cached automatically
//! let response = client.get("https://api.example.com/data").send().await?;
//! ```
//!
//! # With Concurrency Control (Dogpile Prevention)
//!
//! ```ignore
//! use reqwest::Client;
//! use reqwest_middleware::ClientBuilder;
//! use hitbox_reqwest::{CacheMiddleware, BroadcastConcurrencyManager};
//! use hitbox_configuration::ConfigEndpoint;
//! use hitbox_moka::MokaBackend;
//!
//! let config = ConfigEndpoint::default().into_endpoint().unwrap();
//!
//! // With broadcast concurrency manager to prevent dogpile effect
//! let middleware = CacheMiddleware::builder()
//!     .backend(MokaBackend::builder(1000).build())
//!     .config(config)
//!     .concurrency_manager(BroadcastConcurrencyManager::new())
//!     .build();
//!
//! let client = ClientBuilder::new(Client::new())
//!     .with(middleware)
//!     .build();
//! ```

mod middleware;
mod upstream;

pub use middleware::{CacheMiddleware, CacheMiddlewareBuilder};
pub use upstream::ReqwestUpstream;

// Re-export hitbox-http types for convenience
pub use hitbox_http::{
    BufferedBody, CacheableHttpRequest, CacheableHttpResponse, HttpEndpoint,
    SerializableHttpResponse, extractors, predicates,
};

/// Re-export reqwest body type for convenience in type annotations
pub use reqwest::Body as ReqwestBody;

// Re-export common types
pub use hitbox::config::CacheConfig;
pub use hitbox::policy::PolicyConfig;
pub use hitbox_core::DisabledOffload;

// Re-export concurrency types
pub use hitbox::concurrency::{
    BroadcastConcurrencyManager, ConcurrencyManager, NoopConcurrencyManager,
};
