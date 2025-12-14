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

impl PolicyConfig {
    /// Create a new builder for an enabled cache configuration.
    pub fn builder() -> PolicyConfigBuilder {
        PolicyConfigBuilder::default()
    }

    /// Create a disabled policy configuration.
    pub fn disabled() -> Self {
        Self::Disabled
    }
}

/// Builder for PolicyConfig.
#[derive(Debug, Clone, Default)]
pub struct PolicyConfigBuilder {
    ttl: Option<Duration>,
    stale: Option<Duration>,
    stale_policy: StalePolicy,
    concurrency: Option<ConcurrencyLimit>,
}

impl PolicyConfigBuilder {
    /// Create a new builder with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the time-to-live before cache entry becomes stale.
    pub fn ttl(self, ttl: Duration) -> Self {
        Self {
            ttl: Some(ttl),
            ..self
        }
    }

    /// Set the duration during which stale data can still be served.
    pub fn stale(self, stale: Duration) -> Self {
        Self {
            stale: Some(stale),
            ..self
        }
    }

    /// Set the policy for handling stale cache entries.
    pub fn stale_policy(self, policy: StalePolicy) -> Self {
        Self {
            stale_policy: policy,
            ..self
        }
    }

    /// Set the concurrency limit for dogpile prevention.
    pub fn concurrency(self, limit: ConcurrencyLimit) -> Self {
        Self {
            concurrency: Some(limit),
            ..self
        }
    }

    /// Build the PolicyConfig with enabled caching.
    pub fn build(self) -> PolicyConfig {
        PolicyConfig::Enabled(EnabledCacheConfig {
            ttl: self.ttl,
            stale: self.stale,
            policy: CacheBehaviorPolicy {
                stale: self.stale_policy,
            },
            concurrency: self.concurrency,
        })
    }
}
