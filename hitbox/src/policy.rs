use serde::{Deserialize, Serialize};

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
    /// Time-to-live in seconds before cache entry becomes stale.
    pub ttl: Option<u32>,
    /// Duration in seconds during which stale data can still be served.
    pub stale: Option<u32>,
    /// Cache behavior policy.
    #[serde(default)]
    pub policy: CacheBehaviorPolicy,
    /// Concurrency limit for dogpile prevention.
    pub concurrency: Option<u8>,
}

impl Default for EnabledCacheConfig {
    fn default() -> Self {
        Self {
            ttl: Some(5),
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
