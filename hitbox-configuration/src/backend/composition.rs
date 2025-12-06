use serde::{Deserialize, Serialize};

/// Read policy for composition backends.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
pub enum ReadPolicy {
    /// Try L1 first, then L2 on miss (default)
    #[default]
    Sequential,
    /// Race L1 and L2, return first hit
    Race,
    /// Query both in parallel, prefer fresher by TTL
    Parallel,
}

/// Write policy for composition backends.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
pub enum WritePolicy {
    /// Write to L1, then L2 (write-through)
    Sequential,
    /// Write to both in parallel, succeed if at least one succeeds (default)
    #[default]
    OptimisticParallel,
    /// Race both writes, return on first success
    Race,
}

/// Refill policy for composition backends.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
pub enum RefillPolicyConfig {
    /// Always populate L1 after L2 hit
    Always,
    /// Never populate L1 after L2 hit (default)
    #[default]
    Never,
}

/// Policy configuration for composition backends.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
pub struct CompositionPolicyConfig {
    #[serde(default)]
    pub read: ReadPolicy,
    #[serde(default)]
    pub write: WritePolicy,
    #[serde(default)]
    pub refill: RefillPolicyConfig,
}
