use hitbox_backend::Backend as BackendTrait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::ConfigError;

use super::serialization::BackendConfig;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Moka {
    pub max_capacity: u64,
    /// Optional label for this backend (used in metrics/tracing).
    #[serde(default)]
    pub label: Option<String>,
}

impl BackendConfig<Moka> {
    #[cfg(feature = "moka")]
    pub fn into_backend(self) -> Result<Arc<dyn BackendTrait + Send + 'static>, ConfigError> {
        use hitbox_moka::MokaBackend;

        let key_format = self.key.format.to_cache_key_format();
        let serializer = self.value.format.to_serializer();
        let compressor = self.value.compression.to_compressor()?;

        let mut builder = MokaBackend::builder(self.backend.max_capacity)
            .key_format(key_format)
            .value_format(serializer)
            .compressor(compressor);

        if let Some(label) = self.backend.label {
            builder = builder.label(label);
        }

        Ok(Arc::new(builder.build()))
    }

    #[cfg(not(feature = "moka"))]
    pub fn into_backend(self) -> Result<Arc<dyn BackendTrait + Send + 'static>, ConfigError> {
        Err(ConfigError::BackendNotAvailable("Moka".to_string()))
    }
}
