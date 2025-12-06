use hitbox_backend::Backend as BackendTrait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::ConfigError;

use super::serialization::BackendConfig;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct FeOxDb {
    pub path: Option<String>,
}

impl BackendConfig<FeOxDb> {
    #[cfg(feature = "feoxdb")]
    pub fn into_backend(self) -> Result<Arc<dyn BackendTrait + Send + 'static>, ConfigError> {
        use hitbox_feoxdb::FeOxDbBackend;

        let key_format = self.key.format.to_cache_key_format();
        let serializer = self.value.format.to_serializer();
        let compressor = self.value.compression.to_compressor()?;

        let mut builder = FeOxDbBackend::builder()
            .key_format(key_format)
            .value_format(serializer)
            .compressor(compressor);

        if let Some(path) = self.backend.path {
            builder = builder.path(path);
        }

        let backend = builder
            .build()
            .map_err(|e| ConfigError::BackendNotAvailable(format!("FeOxDb: {}", e)))?;

        Ok(Arc::new(backend))
    }

    #[cfg(not(feature = "feoxdb"))]
    pub fn into_backend(self) -> Result<Arc<dyn BackendTrait + Send + 'static>, ConfigError> {
        Err(ConfigError::BackendNotAvailable("FeOxDb".to_string()))
    }
}
