//! Hitbox cache integration for reqwest HTTP client via reqwest-middleware.
//!
//! This crate provides a middleware for `reqwest-middleware` that adds caching
//! capabilities using the hitbox caching framework. It reuses `hitbox-http` types
//! for request/response handling and predicates.
//!
//! # Example
//!
//! ```ignore
//! use reqwest::Client;
//! use reqwest_middleware::ClientBuilder;
//! use hitbox_reqwest::CacheMiddleware;
//! use hitbox_configuration::ConfigEndpoint;
//! use hitbox_moka::MokaBackend;
//! use std::sync::Arc;
//!
//! let backend = Arc::new(MokaBackend::builder(1000).build());
//! let config = ConfigEndpoint::default().into_endpoint().unwrap();
//! let middleware = CacheMiddleware::new(backend, config);
//!
//! let client = ClientBuilder::new(Client::new())
//!     .with(middleware)
//!     .build();
//!
//! // All GET requests will be cached automatically
//! let response = client.get("https://api.example.com/data").send().await?;
//! ```

mod middleware;
mod upstream;

pub use middleware::CacheMiddleware;
pub use upstream::ReqwestUpstream;

// Re-export hitbox-http types for convenience
pub use hitbox_http::{
    BufferedBody, CacheableHttpRequest, CacheableHttpResponse, SerializableHttpResponse,
    extractors, predicates,
};

/// Re-export reqwest body type for convenience in type annotations
pub use reqwest::Body as ReqwestBody;

// Re-export common types
pub use hitbox::config::CacheConfig;
pub use hitbox::policy::PolicyConfig;
pub use hitbox_core::DisabledOffload;
