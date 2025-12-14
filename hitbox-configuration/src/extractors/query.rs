use hitbox_http::extractors;
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::transform::{Transform, into_http_transforms};
use crate::{ConfigError, RequestExtractor};

/// Name selector for query parameter matching.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[serde(untagged)]
pub enum NameSelector {
    /// Simple exact name match
    Exact(String),
    /// Operation-based selection
    Operation(NameOperation),
}

impl NameSelector {
    fn into_extractor_selector(self) -> extractors::query::NameSelector {
        match self {
            NameSelector::Exact(name) => extractors::query::NameSelector::Exact(name),
            NameSelector::Operation(op) => op.into_extractor_selector(),
        }
    }
}

/// Name matching operations.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum NameOperation {
    /// Exact match
    Eq(String),
    /// Prefix match
    Starts(String),
}

impl NameOperation {
    fn into_extractor_selector(self) -> extractors::query::NameSelector {
        match self {
            NameOperation::Eq(name) => extractors::query::NameSelector::Exact(name),
            NameOperation::Starts(prefix) => extractors::query::NameSelector::Starts(prefix),
        }
    }
}

/// Value extractor for query parameters.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[serde(untagged)]
pub enum ValueExtractor {
    /// Regex pattern (string form)
    Regex(String),
}

impl ValueExtractor {
    fn try_into_extractor(self) -> Result<extractors::query::ValueExtractor, ConfigError> {
        match self {
            ValueExtractor::Regex(pattern) => Regex::new(&pattern)
                .map(extractors::query::ValueExtractor::Regex)
                .map_err(|error| ConfigError::InvalidRegex { pattern, error }),
        }
    }
}

/// Extended query configuration.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct QueryConfig {
    /// Name selector
    pub name: NameSelector,
    /// Optional value extractor (regex pattern)
    #[serde(default)]
    pub value: Option<ValueExtractor>,
    /// Value transformations (optional)
    #[serde(default)]
    pub transforms: Vec<Transform>,
}

impl QueryConfig {
    fn into_extractors<ReqBody>(
        self,
        inner: RequestExtractor<ReqBody>,
    ) -> Result<RequestExtractor<ReqBody>, ConfigError>
    where
        ReqBody: hyper::body::Body + Send + 'static,
        ReqBody::Error: Send,
        ReqBody::Data: Send,
    {
        let name_selector = self.name.into_extractor_selector();
        let value_extractor = self
            .value
            .map(ValueExtractor::try_into_extractor)
            .transpose()?
            .unwrap_or(extractors::query::ValueExtractor::Full);
        let transforms = into_http_transforms(self.transforms);

        Ok(Box::new(extractors::query::Query::new_with(
            inner,
            name_selector,
            value_extractor,
            transforms,
        )))
    }
}

/// Query parameter extractor configuration.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[serde(untagged)]
pub enum QueryOperation {
    /// Simple query parameter name
    Simple(String),
    /// Extended configuration
    Extended(QueryConfig),
}

impl QueryOperation {
    pub fn into_extractors<ReqBody>(
        self,
        inner: RequestExtractor<ReqBody>,
    ) -> Result<RequestExtractor<ReqBody>, ConfigError>
    where
        ReqBody: hyper::body::Body + Send + 'static,
        ReqBody::Error: Send,
        ReqBody::Data: Send,
    {
        match self {
            QueryOperation::Simple(name) => Ok(Box::new(extractors::query::Query::new_with(
                inner,
                extractors::query::NameSelector::Exact(name),
                extractors::query::ValueExtractor::Full,
                Vec::new(),
            ))),
            QueryOperation::Extended(config) => config.into_extractors(inner),
        }
    }
}
