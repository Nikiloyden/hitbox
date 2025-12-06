use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(tag = "type")]
pub enum Compression {
    #[default]
    Disabled,
    Gzip {
        #[serde(default = "default_gzip_level")]
        level: u32,
    },
    Zstd {
        #[serde(default = "default_zstd_level")]
        level: i32,
    },
}

fn default_gzip_level() -> u32 {
    6
}

fn default_zstd_level() -> i32 {
    3
}

impl Compression {
    /// Convert configuration compression format to backend compressor
    pub fn to_compressor(
        &self,
    ) -> Result<std::sync::Arc<dyn hitbox_backend::Compressor>, ConfigError> {
        use hitbox_backend::PassthroughCompressor;
        use std::sync::Arc;

        match self {
            Compression::Disabled => Ok(Arc::new(PassthroughCompressor)),
            #[cfg(feature = "gzip")]
            Compression::Gzip { level } => {
                use hitbox_backend::GzipCompressor;
                Ok(Arc::new(GzipCompressor::with_level(*level)))
            }
            #[cfg(not(feature = "gzip"))]
            Compression::Gzip { .. } => Err(ConfigError::BackendNotAvailable(
                "Gzip compression requested but 'gzip' feature is not enabled".to_string(),
            )),
            #[cfg(feature = "zstd")]
            Compression::Zstd { level } => {
                use hitbox_backend::ZstdCompressor;
                Ok(Arc::new(ZstdCompressor::with_level(*level)))
            }
            #[cfg(not(feature = "zstd"))]
            Compression::Zstd { .. } => Err(ConfigError::BackendNotAvailable(
                "Zstd compression requested but 'zstd' feature is not enabled".to_string(),
            )),
        }
    }
}
