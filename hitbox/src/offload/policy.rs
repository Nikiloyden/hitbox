//! Offload task policies and configuration.

use std::time::Duration;

/// Policy for handling task timeouts.
#[derive(Debug, Clone, Default)]
pub enum TimeoutPolicy {
    /// No timeout - task runs until completion.
    #[default]
    None,
    /// Cancel task after specified duration.
    Cancel(Duration),
    /// Log warning after duration but let task continue.
    Warn(Duration),
}

/// Configuration for the OffloadManager.
#[derive(Debug, Clone)]
pub struct OffloadConfig {
    /// Maximum number of concurrent offloaded tasks.
    /// None means unlimited.
    pub max_concurrent_tasks: Option<usize>,
    /// Timeout policy for spawned tasks.
    pub timeout_policy: TimeoutPolicy,
    /// Enable task deduplication by key.
    pub deduplicate: bool,
}

impl Default for OffloadConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: None,
            timeout_policy: TimeoutPolicy::None,
            deduplicate: true,
        }
    }
}

impl OffloadConfig {
    /// Create a new builder for OffloadConfig.
    pub fn builder() -> OffloadConfigBuilder {
        OffloadConfigBuilder::default()
    }
}

/// Builder for OffloadConfig.
#[derive(Debug, Clone, Default)]
pub struct OffloadConfigBuilder {
    max_concurrent_tasks: Option<usize>,
    timeout_policy: TimeoutPolicy,
    deduplicate: bool,
}

impl OffloadConfigBuilder {
    /// Create a new builder with default values.
    pub fn new() -> Self {
        Self {
            max_concurrent_tasks: None,
            timeout_policy: TimeoutPolicy::None,
            deduplicate: true,
        }
    }

    /// Set maximum concurrent tasks.
    pub fn max_concurrent_tasks(self, max: usize) -> Self {
        Self {
            max_concurrent_tasks: Some(max),
            ..self
        }
    }

    /// Set timeout policy.
    pub fn timeout_policy(self, policy: TimeoutPolicy) -> Self {
        Self {
            timeout_policy: policy,
            ..self
        }
    }

    /// Set timeout with cancel policy.
    pub fn timeout(self, duration: Duration) -> Self {
        self.timeout_policy(TimeoutPolicy::Cancel(duration))
    }

    /// Enable or disable task deduplication.
    pub fn deduplicate(self, enabled: bool) -> Self {
        Self {
            deduplicate: enabled,
            ..self
        }
    }

    /// Build the OffloadConfig.
    pub fn build(self) -> OffloadConfig {
        OffloadConfig {
            max_concurrent_tasks: self.max_concurrent_tasks,
            timeout_policy: self.timeout_policy,
            deduplicate: self.deduplicate,
        }
    }
}
