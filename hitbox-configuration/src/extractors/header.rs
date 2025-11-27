//! Header extractor configuration.
//!
//! Supports various ways to select headers and extract values:
//!
//! ```yaml
//! extractors:
//!   # Simple (backwards compatible)
//!   - Header: "Authorization"
//!
//!   # With value regex extraction
//!   - Header:
//!       name: "Authorization"
//!       value: "Bearer (.+)"
//!
//!   # With hash transform
//!   - Header:
//!       name: "Authorization"
//!       transform: hash
//!
//!   # Prefix match + regex + hash
//!   - Header:
//!       name:
//!         starts: "X-API-"
//!       value:
//!         regex: "key=(.+)"
//!       transform: hash
//! ```

use hitbox_http::extractors::header::{
    Header as HttpHeader, NameSelector as HttpNameSelector, Transform as HttpTransform,
    ValueExtractor as HttpValueExtractor,
};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::ConfigError;
use crate::RequestExtractor;

/// Header name selector configuration.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[serde(untagged)]
pub enum NameSelector {
    /// Exact name match (implicit): `name: "Authorization"`
    Exact(String),
    /// Explicit operation
    Operation(NameOperation),
}

/// Explicit name selection operations.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum NameOperation {
    /// Exact match: `name: { eq: "Authorization" }`
    Eq(String),
    /// Prefix match: `name: { starts: "X-Custom-" }`
    Starts(String),
}

/// Value extractor configuration.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[serde(untagged)]
pub enum ValueExtractor {
    /// Implicit regex: `value: "Bearer (.+)"`
    Regex(String),
    /// Explicit operation
    Operation(ValueOperation),
}

/// Explicit value extraction operations.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ValueOperation {
    /// Regex extraction: `value: { regex: "Bearer (.+)" }`
    Regex(String),
}

/// Value transformation.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Default)]
#[serde(rename_all = "lowercase")]
pub enum Transform {
    /// No transformation (default)
    #[default]
    None,
    /// SHA256 hash (truncated)
    Hash,
}

/// Extended header configuration.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct HeaderConfig {
    /// Header name selector
    pub name: NameSelector,
    /// Value extractor (optional, defaults to full value)
    #[serde(default)]
    pub value: Option<ValueExtractor>,
    /// Value transformation (optional, defaults to none)
    #[serde(default)]
    pub transform: Transform,
}

/// Header extractor operation.
///
/// Supports both simple string (backwards compatible) and extended configuration.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[serde(untagged)]
pub enum HeaderOperation {
    /// Simple header name: `Header: "Authorization"`
    Simple(String),
    /// Extended configuration
    Extended(HeaderConfig),
}

impl HeaderOperation {
    pub fn into_extractors<ReqBody>(
        self,
        inner: RequestExtractor<ReqBody>,
    ) -> Result<RequestExtractor<ReqBody>, ConfigError>
    where
        ReqBody: hyper::body::Body + Send + 'static,
        ReqBody::Error: Send,
        ReqBody::Data: Send,
    {
        let (name_selector, value_extractor, transform) = match self {
            HeaderOperation::Simple(name) => (
                HttpNameSelector::Exact(name),
                HttpValueExtractor::Full,
                HttpTransform::None,
            ),
            HeaderOperation::Extended(config) => {
                let name_selector = match config.name {
                    NameSelector::Exact(name) => HttpNameSelector::Exact(name),
                    NameSelector::Operation(op) => match op {
                        NameOperation::Eq(name) => HttpNameSelector::Exact(name),
                        NameOperation::Starts(prefix) => HttpNameSelector::Starts(prefix),
                    },
                };

                let value_extractor = match config.value {
                    None => HttpValueExtractor::Full,
                    Some(ValueExtractor::Regex(pattern)) => {
                        let regex = compile_regex(&pattern)?;
                        HttpValueExtractor::Regex(regex)
                    }
                    Some(ValueExtractor::Operation(ValueOperation::Regex(pattern))) => {
                        let regex = compile_regex(&pattern)?;
                        HttpValueExtractor::Regex(regex)
                    }
                };

                let transform = match config.transform {
                    Transform::None => HttpTransform::None,
                    Transform::Hash => HttpTransform::Hash,
                };

                (name_selector, value_extractor, transform)
            }
        };

        Ok(Box::new(HttpHeader::new(
            inner,
            name_selector,
            value_extractor,
            transform,
        )))
    }
}

fn compile_regex(pattern: &str) -> Result<Regex, ConfigError> {
    Regex::new(pattern).map_err(|e| ConfigError::InvalidRegex {
        pattern: pattern.to_string(),
        error: e,
    })
}
