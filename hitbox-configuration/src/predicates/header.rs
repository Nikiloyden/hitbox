//! Shared header predicate configuration types.
//!
//! This module contains common types used by both request and response
//! header predicates to avoid code duplication.

use http::header::{HeaderName, HeaderValue as HttpHeaderValue};
use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

/// Header value configuration supporting multiple formats.
///
/// Supports both shorthand and explicit operation syntax:
/// ```yaml
/// # Shorthand forms
/// Content-Type: "application/json"           # Implicit Eq
/// Accept: ["application/json", "text/html"]  # Implicit In
///
/// # Explicit operation form
/// X-Custom:
///   contains: "value"
///   regex: "pattern.*"
///   exist: true
/// ```
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(untagged)]
pub enum HeaderValue {
    /// Shorthand for equality check: `Header: "value"`
    Eq(String),
    /// Shorthand for in-list check: `Header: ["val1", "val2"]`
    In(Vec<String>),
    /// Explicit operation: `Header: { contains: "value" }`
    Operation(HeaderValueOperation),
}

/// Explicit header value operations.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum HeaderValueOperation {
    /// Exact value match
    Eq(String),
    /// Value is one of the specified values
    In(Vec<String>),
    /// Value contains substring
    Contains(String),
    /// Value matches regex pattern
    Regex(String),
    /// Header exists (value is ignored)
    #[serde(deserialize_with = "deserialize_exist")]
    Exist,
}

/// Custom deserializer for `Exist` variant that accepts any value.
fn deserialize_exist<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::IgnoredAny;
    IgnoredAny::deserialize(deserializer)?;
    Ok(())
}

/// Map of header names to their value operations.
pub type HeaderOperation = IndexMap<String, HeaderValue>;

/// Parse a header name string into `HeaderName`.
pub fn parse_header_name(name: &str) -> Result<HeaderName, ConfigError> {
    name.parse()
        .map_err(|e| ConfigError::InvalidHeaderName(name.to_string(), e))
}

/// Parse a header value string into `HttpHeaderValue`.
pub fn parse_header_value(value: &str) -> Result<HttpHeaderValue, ConfigError> {
    value
        .parse()
        .map_err(|e| ConfigError::InvalidHeaderValue(value.to_string(), e))
}

/// Parse multiple header value strings into `Vec<HttpHeaderValue>`.
pub fn parse_header_values(values: &[String]) -> Result<Vec<HttpHeaderValue>, ConfigError> {
    values.iter().map(|v| parse_header_value(v)).collect()
}

/// Convert a `HeaderValue` configuration into a predicate `Operation`.
pub fn header_value_to_operation(
    name: HeaderName,
    header_value: HeaderValue,
) -> Result<hitbox_http::predicates::header::Operation, ConfigError> {
    use hitbox_http::predicates::header::Operation;

    match header_value {
        HeaderValue::Eq(value) => {
            let val = parse_header_value(&value)?;
            Ok(Operation::Eq(name, val))
        }
        HeaderValue::In(values) => {
            let vals = parse_header_values(&values)?;
            Ok(Operation::In(name, vals))
        }
        HeaderValue::Operation(op) => match op {
            HeaderValueOperation::Eq(value) => {
                let val = parse_header_value(&value)?;
                Ok(Operation::Eq(name, val))
            }
            HeaderValueOperation::In(values) => {
                let vals = parse_header_values(&values)?;
                Ok(Operation::In(name, vals))
            }
            HeaderValueOperation::Contains(substring) => Ok(Operation::Contains(name, substring)),
            HeaderValueOperation::Regex(pattern) => {
                let compiled_regex =
                    Regex::new(&pattern).map_err(|e| ConfigError::InvalidRegex {
                        pattern: pattern.clone(),
                        error: e,
                    })?;
                Ok(Operation::Regex(name, compiled_regex))
            }
            HeaderValueOperation::Exist => Ok(Operation::Exist(name)),
        },
    }
}
