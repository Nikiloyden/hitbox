use hitbox_backend::Backend as BackendTrait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::ConfigError;

use super::composition::CompositionConfig;
use super::feoxdb::FeOxDb;
use super::moka::Moka;
use super::redis::Redis;
use super::serialization::BackendConfig;

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
            Backend::Moka(config) => config.into_backend(),
            Backend::FeOxDb(config) => config.into_backend(),
            Backend::Redis(config) => config.into_backend(),
            Backend::Composition(config) => config.into_backend(),
        }
    }
}
