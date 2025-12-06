use hitbox_backend::Backend as BackendTrait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::ConfigError;

use super::composition::{CompositionPolicyConfig, ReadPolicy, RefillPolicyConfig, WritePolicy};
use super::serialization::BackendConfig;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Moka {
    pub max_capacity: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct FeOxDb {
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Redis {
    pub connection_string: String,
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Backend {
    Moka(BackendConfig<Moka>),
    FeOxDb(BackendConfig<FeOxDb>),
    Redis(BackendConfig<Redis>),
    Composition(CompositionConfig),
}

impl Backend {
    pub fn into_backend(self) -> Result<Arc<dyn BackendTrait + Send + 'static>, ConfigError> {
        match self {
            #[cfg(feature = "moka")]
            Backend::Moka(config) => {
                use hitbox_moka::MokaBackend;

                let key_format = config.key.format.to_cache_key_format();
                let serializer = config.value.format.to_serializer();
                let compressor = config.value.compression.to_compressor()?;

                let backend = MokaBackend::builder(config.backend.max_capacity)
                    .key_format(key_format)
                    .value_format(serializer)
                    .compressor(compressor)
                    .build();

                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "moka"))]
            Backend::Moka(_) => Err(ConfigError::BackendNotAvailable("Moka".to_string())),
            #[cfg(feature = "feoxdb")]
            Backend::FeOxDb(config) => {
                use hitbox_feoxdb::FeOxDbBackend;

                let key_format = config.key.format.to_cache_key_format();
                let serializer = config.value.format.to_serializer();
                let compressor = config.value.compression.to_compressor()?;

                let mut builder = FeOxDbBackend::builder()
                    .key_format(key_format)
                    .value_format(serializer)
                    .compressor(compressor);

                if let Some(path) = config.backend.path {
                    builder = builder.path(path);
                }

                let backend = builder
                    .build()
                    .map_err(|e| ConfigError::BackendNotAvailable(format!("FeOxDb: {}", e)))?;

                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "feoxdb"))]
            Backend::FeOxDb(_) => Err(ConfigError::BackendNotAvailable("FeOxDb".to_string())),
            #[cfg(feature = "redis")]
            Backend::Redis(config) => {
                use hitbox_redis::RedisBackend;

                let key_format = config.key.format.to_cache_key_format();
                let serializer = config.value.format.to_serializer();
                let compressor = config.value.compression.to_compressor()?;

                let backend = RedisBackend::builder()
                    .server(config.backend.connection_string)
                    .key_format(key_format)
                    .value_format(serializer)
                    .compressor(compressor)
                    .build()
                    .map_err(|e| ConfigError::BackendNotAvailable(format!("Redis: {}", e)))?;

                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "redis"))]
            Backend::Redis(_) => Err(ConfigError::BackendNotAvailable("Redis".to_string())),
            Backend::Composition(config) => {
                use hitbox::offload::OffloadManager;
                use hitbox_backend::composition::policy::{
                    OptimisticParallelWritePolicy, ParallelReadPolicy, RaceReadPolicy,
                    RaceWritePolicy, RefillPolicy, SequentialReadPolicy, SequentialWritePolicy,
                };
                use hitbox_backend::composition::{Compose, CompositionPolicy};

                let l1 = config.l1.into_backend()?;
                let l2 = config.l2.into_backend()?;
                let offload = OffloadManager::default();

                let refill = match config.policy.refill {
                    RefillPolicyConfig::Always => RefillPolicy::Always,
                    RefillPolicyConfig::Never => RefillPolicy::Never,
                };

                // Match all combinations of read and write policies
                match (config.policy.read, config.policy.write) {
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
    }
}
