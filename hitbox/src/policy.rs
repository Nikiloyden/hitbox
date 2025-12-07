use std::time::Duration;

use bounded_integer::bounded_integer;
use serde::{Deserialize, Serialize};

bounded_integer! {
    /// Concurrency limit for dogpile prevention (1-255).
    /// A value of 1 means only one request can fetch from upstream at a time.
    #[repr(u8)]
    pub struct ConcurrencyLimit { 1..=255 }
}

/// Policy for handling stale cache entries.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Default)]
pub enum StalePolicy {
    /// Return stale data without any revalidation.
    #[default]
    Return,
    /// Treat stale as expired — block and wait for fresh data (synchronous revalidation).
    Revalidate,
    /// Return stale data immediately and revalidate in background (Stale-While-Revalidate).
    OffloadRevalidate,
}

/// Cache behavior policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub struct CacheBehaviorPolicy {
    /// How to handle stale cache entries.
    #[serde(default)]
    pub stale: StalePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct EnabledCacheConfig {
    /// Time-to-live before cache entry becomes stale (e.g., "5s", "500ms", "1m").
    #[serde(default, with = "humantime_serde")]
    pub ttl: Option<Duration>,
    /// Duration during which stale data can still be served (e.g., "5s", "500ms", "1m").
    #[serde(default, with = "humantime_serde")]
    pub stale: Option<Duration>,
    /// Cache behavior policy.
    #[serde(default)]
    pub policy: CacheBehaviorPolicy,
    /// Concurrency limit for dogpile prevention.
    pub concurrency: Option<ConcurrencyLimit>,
}

impl Default for EnabledCacheConfig {
    fn default() -> Self {
        Self {
            ttl: Some(Duration::from_secs(5)),
            stale: None,
            policy: CacheBehaviorPolicy::default(),
            concurrency: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum PolicyConfig {
    Enabled(EnabledCacheConfig),
    Disabled,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self::Enabled(EnabledCacheConfig::default())
    }
}
