//! Cache policy types and configuration.
//!
//! This module provides types for representing cache decisions and
//! configuring caching behavior:
//!
//! - [`CachePolicy`] - Result of a cache decision (cacheable or not)
//! - [`EntityPolicyConfig`] - TTL configuration for cached entities
//!
//! ## Cache Policy
//!
//! [`CachePolicy`] represents the outcome of determining whether something
//! should be cached. It's a two-variant enum that preserves type information
//! for both cacheable and non-cacheable cases.
//!
//! ## Configuration
//!
//! [`EntityPolicyConfig`] provides TTL (time-to-live) settings for cached
//! entries, supporting both expiration and staleness timeouts for
//! stale-while-revalidate patterns.

use std::time::Duration;

/// Result of a cache decision.
///
/// Represents whether an entity should be cached or passed through without
/// caching. Both variants preserve the entity, just wrapped differently.
///
/// # Type Parameters
///
/// * `C` - Type of the cacheable entity (usually the cached representation)
/// * `N` - Type of the non-cacheable entity (usually the original response)
///
/// # Example
///
/// ```
/// use hitbox_core::CachePolicy;
///
/// fn decide_caching(status: u16, body: String) -> CachePolicy<String, String> {
///     if status == 200 {
///         CachePolicy::Cacheable(body)
///     } else {
///         CachePolicy::NonCacheable(body)
///     }
/// }
///
/// match decide_caching(200, "OK".to_string()) {
///     CachePolicy::Cacheable(data) => println!("Cache: {}", data),
///     CachePolicy::NonCacheable(data) => println!("Pass through: {}", data),
/// }
/// ```
#[derive(Debug)]
pub enum CachePolicy<C, N> {
    /// Entity should be cached.
    Cacheable(C),
    /// Entity should not be cached; pass through directly.
    NonCacheable(N),
}

/// Configuration for entity caching TTLs.
///
/// Specifies how long cached entries should live and when they become stale.
/// Used by [`CacheableResponse::cache_policy`](crate::response::CacheableResponse::cache_policy)
/// to set timestamps on cached values.
///
/// # Fields
///
/// * `ttl` - Time until the entry expires (becomes invalid)
/// * `stale_ttl` - Time until the entry becomes stale (should refresh in background)
///
/// # Example
///
/// ```
/// use hitbox_core::EntityPolicyConfig;
/// use std::time::Duration;
///
/// // Expire after 1 hour, become stale after 5 minutes
/// let config = EntityPolicyConfig {
///     ttl: Some(Duration::from_secs(3600)),
///     stale_ttl: Some(Duration::from_secs(300)),
/// };
///
/// // No expiration (cached forever until manually invalidated)
/// let forever = EntityPolicyConfig::default();
/// ```
#[derive(Default)]
pub struct EntityPolicyConfig {
    /// Time until cached entries expire and become invalid.
    pub ttl: Option<Duration>,
    /// Time until cached entries become stale (for background refresh).
    pub stale_ttl: Option<Duration>,
}
