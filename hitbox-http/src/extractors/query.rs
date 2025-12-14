//! Query parameter extractor with support for name selection, value extraction, and transformation.

use async_trait::async_trait;
use hitbox::{Extractor, KeyPart, KeyParts};
use regex::Regex;

pub use super::transform::Transform;
use super::NeutralExtractor;
use super::transform::apply_transform_chain;
use crate::CacheableHttpRequest;

/// Query parameter name selector.
#[derive(Debug, Clone)]
pub enum NameSelector {
    /// Exact parameter name match
    Exact(String),
    /// Parameters starting with prefix
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

/// Query parameter extractor.
#[derive(Debug)]
pub struct Query<E> {
    inner: E,
    name_selector: NameSelector,
    value_extractor: ValueExtractor,
    transforms: Vec<Transform>,
}

impl<S> Query<NeutralExtractor<S>> {
    pub fn new(name: String) -> Self {
        Self {
            inner: NeutralExtractor::new(),
            name_selector: NameSelector::Exact(name),
            value_extractor: ValueExtractor::Full,
            transforms: Vec::new(),
        }
    }
}

impl<E> Query<E> {
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

pub trait QueryExtractor: Sized {
    fn query(self, name: String) -> Query<Self>;
}

impl<E> QueryExtractor for E
where
    E: Extractor,
{
    fn query(self, name: String) -> Query<Self> {
        Query {
            inner: self,
            name_selector: NameSelector::Exact(name),
            value_extractor: ValueExtractor::Full,
            transforms: Vec::new(),
        }
    }
}

/// Extract value using the value extractor.
fn extract_value(value: &str, extractor: &ValueExtractor) -> Option<String> {
    match extractor {
        ValueExtractor::Full => Some(value.to_string()),
        ValueExtractor::Regex(regex) => regex
            .captures(value)
            .and_then(|caps| caps.get(1).or_else(|| caps.get(0)))
            .map(|m| m.as_str().to_string()),
    }
}

#[async_trait]
impl<ReqBody, E> Extractor for Query<E>
where
    ReqBody: hyper::body::Body + Send + 'static,
    ReqBody::Error: Send,
    E: Extractor<Subject = CacheableHttpRequest<ReqBody>> + Send + Sync,
{
    type Subject = E::Subject;

    async fn get(&self, subject: Self::Subject) -> KeyParts<Self::Subject> {
        let query_map = subject
            .parts()
            .uri
            .query()
            .and_then(crate::query::parse)
            .unwrap_or_default();

        let mut extracted_parts: Vec<KeyPart> = match &self.name_selector {
            NameSelector::Exact(name) => query_map
                .get(name)
                .map(|v| v.inner())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| {
                    extract_value(&value, &self.value_extractor)
                        .map(|v| apply_transform_chain(v, &self.transforms))
                        .map(|v| KeyPart::new(name.clone(), Some(v)))
                })
                .collect(),

            NameSelector::Starts(prefix) => {
                let mut parts: Vec<KeyPart> = query_map
                    .iter()
                    .filter(|(name, _)| name.starts_with(prefix.as_str()))
                    .flat_map(|(name, value)| {
                        value.inner().into_iter().filter_map(|v| {
                            extract_value(&v, &self.value_extractor)
                                .map(|extracted| apply_transform_chain(extracted, &self.transforms))
                                .map(|extracted| KeyPart::new(name.clone(), Some(extracted)))
                        })
                    })
                    .collect();
                // Sort by parameter name for deterministic cache keys
                parts.sort_by(|a, b| a.key().cmp(b.key()));
                parts
            }
        };

        let mut parts = self.inner.get(subject).await;
        parts.append(&mut extracted_parts);
        parts
    }
}
