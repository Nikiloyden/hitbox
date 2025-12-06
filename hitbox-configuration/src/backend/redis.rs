use hitbox_backend::Backend as BackendTrait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::ConfigError;

use super::serialization::BackendConfig;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Redis {
    pub connection_string: String,
    /// Optional label for this backend (used in metrics/tracing).
    #[serde(default)]
    pub label: Option<String>,
}

impl BackendConfig<Redis> {
    #[cfg(feature = "redis")]
    pub fn into_backend(self) -> Result<Arc<dyn BackendTrait + Send + 'static>, ConfigError> {
        use hitbox_redis::RedisBackend;

        let key_format = self.key.format.to_cache_key_format();
        let serializer = self.value.format.to_serializer();
        let compressor = self.value.compression.to_compressor()?;

        let mut builder = RedisBackend::builder()
            .server(self.backend.connection_string)
            .key_format(key_format)
            .value_format(serializer)
            .compressor(compressor);

        if let Some(label) = self.backend.label {
            builder = builder.label(label);
        }

        let backend = builder
            .build()
            .map_err(|e| ConfigError::BackendNotAvailable(format!("Redis: {}", e)))?;

        Ok(Arc::new(backend))
    }

    #[cfg(not(feature = "redis"))]
    pub fn into_backend(self) -> Result<Arc<dyn BackendTrait + Send + 'static>, ConfigError> {
        Err(ConfigError::BackendNotAvailable("Redis".to_string()))
    }
}
