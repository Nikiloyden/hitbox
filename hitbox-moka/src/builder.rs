use crate::backend::{Expiration, MokaBackend};
use hitbox::{BackendLabel, CacheKey, CacheValue, Raw};
use hitbox_backend::format::{Format, JsonFormat};
use hitbox_backend::{CacheKeyFormat, Compressor, PassthroughCompressor};
use moka::future::{Cache, CacheBuilder};

pub struct MokaBackendBuilder<S = JsonFormat, C = PassthroughCompressor>
where
    S: Format,
    C: Compressor,
{
    builder: CacheBuilder<CacheKey, CacheValue<Raw>, Cache<CacheKey, CacheValue<Raw>>>,
    key_format: CacheKeyFormat,
    serializer: S,
    compressor: C,
    name: BackendLabel,
}

impl MokaBackendBuilder<JsonFormat, PassthroughCompressor> {
    pub fn new(max_capacity: u64) -> Self {
        let builder = CacheBuilder::new(max_capacity);
        Self {
            builder,
            key_format: CacheKeyFormat::Bitcode,
            serializer: JsonFormat,
            compressor: PassthroughCompressor,
            name: BackendLabel::new_static("moka"),
        }
    }
}

impl<S, C> MokaBackendBuilder<S, C>
where
    S: Format,
    C: Compressor,
{
    /// Set a custom name for this backend.
    ///
    /// The name is used for source path composition in multi-layer caches.
    /// For example, with name "sessions", the source path might be "composition.L1.sessions".
    pub fn name(mut self, name: impl Into<BackendLabel>) -> Self {
        self.name = name.into();
        self
    }

    pub fn key_format(mut self, format: CacheKeyFormat) -> Self {
        self.key_format = format;
        self
    }

    pub fn value_format<NewS>(self, serializer: NewS) -> MokaBackendBuilder<NewS, C>
    where
        NewS: Format,
    {
        MokaBackendBuilder {
            builder: self.builder,
            key_format: self.key_format,
            serializer,
            compressor: self.compressor,
            name: self.name,
        }
    }

    pub fn compressor<NewC>(self, compressor: NewC) -> MokaBackendBuilder<S, NewC>
    where
        NewC: Compressor,
    {
        MokaBackendBuilder {
            builder: self.builder,
            key_format: self.key_format,
            serializer: self.serializer,
            compressor,
            name: self.name,
        }
    }

    pub fn build(self) -> MokaBackend<S, C> {
        let expiry = Expiration;
        let cache = self.builder.expire_after(expiry).build();
        MokaBackend {
            cache,
            key_format: self.key_format,
            serializer: self.serializer,
            compressor: self.compressor,
            name: self.name,
        }
    }
}
