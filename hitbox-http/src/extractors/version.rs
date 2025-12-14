use async_trait::async_trait;
use hitbox::{Extractor, KeyPart, KeyParts};

use super::NeutralExtractor;
use crate::CacheableHttpRequest;

/// Extractor for HTTP version
#[derive(Debug)]
pub struct Version<E> {
    inner: E,
}

impl<S> Version<NeutralExtractor<S>> {
    pub fn new() -> Self {
        Self {
            inner: NeutralExtractor::new(),
        }
    }
}

/// Extension trait for adding version extraction
pub trait VersionExtractor: Sized {
    fn version(self) -> Version<Self>;
}

impl<E> VersionExtractor for E
where
    E: Extractor,
{
    fn version(self) -> Version<Self> {
        Version { inner: self }
    }
}

#[async_trait]
impl<ReqBody, E> Extractor for Version<E>
where
    ReqBody: hyper::body::Body + Send + 'static,
    ReqBody::Error: Send,
    E: Extractor<Subject = CacheableHttpRequest<ReqBody>> + Send + Sync,
{
    type Subject = E::Subject;

    async fn get(&self, subject: Self::Subject) -> KeyParts<Self::Subject> {
        let version = format!("{:?}", subject.parts().version);
        let mut parts = self.inner.get(subject).await;
        parts.push(KeyPart::new("version", Some(version)));
        parts
    }
}
