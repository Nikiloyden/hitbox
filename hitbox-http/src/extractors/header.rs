//! Header extractor with support for name selection, value extraction, and transformation.

use async_trait::async_trait;
use hitbox::{Extractor, KeyPart, KeyParts};
use http::HeaderValue;
use regex::Regex;

use super::NeutralExtractor;
pub use super::transform::Transform;
use super::transform::apply_transform_chain;
use crate::CacheableHttpRequest;

/// Header name selector.
#[derive(Debug, Clone)]
pub enum NameSelector {
    /// Exact header name match
    Exact(String),
    /// Headers starting with prefix
    Starts(String),
}

/// Value extractor.
#[derive(Debug, Clone)]
pub enum ValueExtractor {
    /// Extract full value
    Full,
    /// Extract using regex (first capture group)
    Regex(Regex),
}

/// Header extractor.
#[derive(Debug)]
pub struct Header<E> {
    inner: E,
    name_selector: NameSelector,
    value_extractor: ValueExtractor,
    transforms: Vec<Transform>,
}

impl<S> Header<NeutralExtractor<S>> {
    pub fn new(name: String) -> Self {
        Self {
            inner: NeutralExtractor::new(),
            name_selector: NameSelector::Exact(name),
            value_extractor: ValueExtractor::Full,
            transforms: Vec::new(),
        }
    }
}

impl<E> Header<E> {
    pub fn new_with(
        inner: E,
        name_selector: NameSelector,
        value_extractor: ValueExtractor,
        transforms: Vec<Transform>,
    ) -> Self {
        Self {
            inner,
            name_selector,
            value_extractor,
            transforms,
        }
    }
}

pub trait HeaderExtractor: Sized {
    fn header(self, name: String) -> Header<Self>;
}

impl<E> HeaderExtractor for E
where
    E: Extractor,
{
    fn header(self, name: String) -> Header<Self> {
        Header {
            inner: self,
            name_selector: NameSelector::Exact(name),
            value_extractor: ValueExtractor::Full,
            transforms: Vec::new(),
        }
    }
}

/// Extract value from header using the value extractor.
fn extract_value(value: &HeaderValue, extractor: &ValueExtractor) -> Option<String> {
    let value_str = value.to_str().ok()?;

    match extractor {
        ValueExtractor::Full => Some(value_str.to_string()),
        ValueExtractor::Regex(regex) => {
            regex.captures(value_str).and_then(|caps| {
                // Return first capture group if exists, otherwise full match
                caps.get(1)
                    .or_else(|| caps.get(0))
                    .map(|m| m.as_str().to_string())
            })
        }
    }
}

#[async_trait]
impl<ReqBody, E> Extractor for Header<E>
where
    ReqBody: hyper::body::Body + Send + 'static,
    ReqBody::Error: Send,
    E: Extractor<Subject = CacheableHttpRequest<ReqBody>> + Send + Sync,
{
    type Subject = E::Subject;

    async fn get(&self, subject: Self::Subject) -> KeyParts<Self::Subject> {
        let headers = &subject.parts().headers;
        let mut extracted_parts = Vec::new();

        match &self.name_selector {
            NameSelector::Exact(name) => {
                let value = headers
                    .get(name.as_str())
                    .and_then(|v| extract_value(v, &self.value_extractor))
                    .map(|v| apply_transform_chain(v, &self.transforms));

                extracted_parts.push(KeyPart::new(name.clone(), value));
            }
            NameSelector::Starts(prefix) => {
                for (name, value) in headers.iter() {
                    let name_str = name.as_str();
                    if name_str.starts_with(prefix.as_str()) {
                        let extracted = extract_value(value, &self.value_extractor)
                            .map(|v| apply_transform_chain(v, &self.transforms));

                        extracted_parts.push(KeyPart::new(name_str, extracted));
                    }
                }
                // Sort by header name for deterministic cache keys
                extracted_parts.sort_by(|a, b| a.key().cmp(b.key()));
            }
        }

        let mut parts = self.inner.get(subject).await;
        parts.append(&mut extracted_parts);
        parts
    }
}
