use hitbox_backend::Backend as BackendTrait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::ConfigError;

use super::core::Backend;

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

/// Configuration for composing two backends into a layered cache.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CompositionConfig {
    /// First-layer cache (typically fast, local)
    pub l1: Box<Backend>,
    /// Second-layer cache (typically distributed, persistent)
    pub l2: Box<Backend>,
    /// Composition policies
    #[serde(default)]
    pub policy: CompositionPolicyConfig,
}

impl CompositionConfig {
    pub fn into_backend(self) -> Result<Arc<dyn BackendTrait + Send + 'static>, ConfigError> {
        use hitbox::offload::OffloadManager;
        use hitbox_backend::composition::policy::{
            OptimisticParallelWritePolicy, ParallelReadPolicy, RaceReadPolicy, RaceWritePolicy,
            RefillPolicy, SequentialReadPolicy, SequentialWritePolicy,
        };
        use hitbox_backend::composition::{Compose, CompositionPolicy};

        let l1 = self.l1.into_backend()?;
        let l2 = self.l2.into_backend()?;
        let offload = OffloadManager::default();

        let refill = match self.policy.refill {
            RefillPolicyConfig::Always => RefillPolicy::Always,
            RefillPolicyConfig::Never => RefillPolicy::Never,
        };

        match (self.policy.read, self.policy.write) {
            (ReadPolicy::Sequential, WritePolicy::Sequential) => {
                let policy = CompositionPolicy::new()
                    .read(SequentialReadPolicy::new())
                    .write(SequentialWritePolicy::new())
                    .refill(refill);
                Ok(Arc::new(l1.compose_with(l2, offload, policy)))
            }
            (ReadPolicy::Sequential, WritePolicy::OptimisticParallel) => {
                let policy = CompositionPolicy::new()
                    .read(SequentialReadPolicy::new())
                    .write(OptimisticParallelWritePolicy::new())
                    .refill(refill);
                Ok(Arc::new(l1.compose_with(l2, offload, policy)))
            }
            (ReadPolicy::Sequential, WritePolicy::Race) => {
                let policy = CompositionPolicy::new()
                    .read(SequentialReadPolicy::new())
                    .write(RaceWritePolicy::new())
                    .refill(refill);
                Ok(Arc::new(l1.compose_with(l2, offload, policy)))
            }
            (ReadPolicy::Race, WritePolicy::Sequential) => {
                let policy = CompositionPolicy::new()
                    .read(RaceReadPolicy::new())
                    .write(SequentialWritePolicy::new())
                    .refill(refill);
                Ok(Arc::new(l1.compose_with(l2, offload, policy)))
            }
            (ReadPolicy::Race, WritePolicy::OptimisticParallel) => {
                let policy = CompositionPolicy::new()
                    .read(RaceReadPolicy::new())
                    .write(OptimisticParallelWritePolicy::new())
                    .refill(refill);
                Ok(Arc::new(l1.compose_with(l2, offload, policy)))
            }
            (ReadPolicy::Race, WritePolicy::Race) => {
                let policy = CompositionPolicy::new()
                    .read(RaceReadPolicy::new())
                    .write(RaceWritePolicy::new())
                    .refill(refill);
                Ok(Arc::new(l1.compose_with(l2, offload, policy)))
            }
            (ReadPolicy::Parallel, WritePolicy::Sequential) => {
                let policy = CompositionPolicy::new()
                    .read(ParallelReadPolicy::new())
                    .write(SequentialWritePolicy::new())
                    .refill(refill);
                Ok(Arc::new(l1.compose_with(l2, offload, policy)))
            }
            (ReadPolicy::Parallel, WritePolicy::OptimisticParallel) => {
                let policy = CompositionPolicy::new()
                    .read(ParallelReadPolicy::new())
                    .write(OptimisticParallelWritePolicy::new())
                    .refill(refill);
                Ok(Arc::new(l1.compose_with(l2, offload, policy)))
            }
            (ReadPolicy::Parallel, WritePolicy::Race) => {
                let policy = CompositionPolicy::new()
                    .read(ParallelReadPolicy::new())
                    .write(RaceWritePolicy::new())
                    .refill(refill);
                Ok(Arc::new(l1.compose_with(l2, offload, policy)))
            }
        }
    }
}
