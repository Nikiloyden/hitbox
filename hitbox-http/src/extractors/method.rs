use async_trait::async_trait;
use hitbox::{Extractor, KeyPart, KeyParts};

use super::NeutralExtractor;
use crate::CacheableHttpRequest;

#[derive(Debug)]
pub struct Method<E> {
    inner: E,
}

impl<S> Method<NeutralExtractor<S>> {
    pub fn new() -> Self {
        Self {
            inner: NeutralExtractor::new(),
        }
    }
}

impl<S> Default for Method<NeutralExtractor<S>> {
    fn default() -> Self {
        Self::new()
    }
}

pub trait MethodExtractor: Sized {
    fn method(self) -> Method<Self>;
}

impl<E> MethodExtractor for E
where
    E: Extractor,
{
    fn method(self) -> Method<Self> {
        Method { inner: self }
    }
}

#[async_trait]
impl<ReqBody, E> Extractor for Method<E>
where
    ReqBody: hyper::body::Body + Send + 'static,
    ReqBody::Error: Send,
    E: Extractor<Subject = CacheableHttpRequest<ReqBody>> + Send + Sync,
{
    type Subject = E::Subject;

    async fn get(&self, subject: Self::Subject) -> KeyParts<Self::Subject> {
        let method = subject.parts().method.to_string();
        let mut parts = self.inner.get(subject).await;
        parts.push(KeyPart::new("method", Some(method)));
        parts
    }
}
