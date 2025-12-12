//! Upstream service abstraction.
//!
//! This module provides the [`Upstream`] trait for calling backend services
//! when cache misses occur.
//!
//! ## Overview
//!
//! The `Upstream` trait abstracts over any async service that can handle
//! requests and return responses. This allows the caching layer to be
//! agnostic to the actual service implementation.
//!
//! ## Framework Integration
//!
//! Protocol-specific crates provide implementations for common frameworks:
//!
//! - `hitbox-reqwest` - Reqwest HTTP client integration
//! - `hitbox-tower` - Tower service integration

use std::future::Future;

/// Trait for calling upstream services with cacheable requests.
///
/// This trait is framework-agnostic and can be implemented for any async service.
///
/// # Examples
///
/// ```rust,ignore
/// use hitbox_core::Upstream;
/// use std::future::Ready;
///
/// struct MockUpstream {
///     response: MyResponse,
/// }
///
/// impl Upstream<MyRequest> for MockUpstream {
///     type Response = MyResponse;
///     type Future = Ready<Self::Response>;
///
///     fn call(&mut self, _req: MyRequest) -> Self::Future {
///         std::future::ready(self.response.clone())
///     }
/// }
/// ```
pub trait Upstream<Req> {
    /// The response type returned by the upstream service
    type Response;

    /// The future that resolves to the response
    type Future: Future<Output = Self::Response> + Send;

    /// Call the upstream service with the given request
    fn call(&mut self, req: Req) -> Self::Future;
}
