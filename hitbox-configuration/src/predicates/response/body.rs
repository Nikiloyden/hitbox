use crate::error::ConfigError;
use bytes::Bytes;
use hitbox_core::Predicate;
use hitbox_http::CacheableHttpResponse;
use hitbox_http::predicates::response::BodyPredicate;
use hitbox_http::predicates::response::body::{
    JqExpression, JqOperation, Operation as BodyOperation, PlainOperation,
};
use hyper::body::Body as HttpBody;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

type CorePredicate<ReqBody> =
    Box<dyn Predicate<Subject = CacheableHttpResponse<ReqBody>> + Send + Sync>;

/// Jq operation configuration
/// Supports both explicit and implicit syntax:
/// - Explicit: `{ expression: ".field", eq: "value" }` - extract field and compare
/// - Implicit: `"length == 3"` - shorthand for `{ expression: "length == 3", eq: true }`
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum JqConfig {
    /// Implicit syntax: plain string becomes expression with eq: true
    Implicit(String),
    /// Explicit syntax: expression with operation
    Explicit {
        expression: String,
        #[serde(flatten)]
        operation: JqOperationConfig,
    },
}

/// Jq operation types (eq, exist, in)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JqOperationConfig {
    Eq(JsonValue),
    #[serde(deserialize_with = "deserialize_exist")]
    Exist,
    In(Vec<JsonValue>),
}

fn deserialize_exist<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::IgnoredAny;
    IgnoredAny::deserialize(deserializer)?;
    Ok(())
}

/// Body predicate operation - supports both plain (byte-based) and jq (JSON-based) operations
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    // Plain operations (byte-based)
    // Note: Stored as String in config, converted to Bytes when creating predicates
    // !!binary tags with non-UTF-8 data currently unsupported due to serde-saphyr limitation
    // See BINARY_DATA_LIMITATION.md for details
    Contains(String),
    Starts(String),
    Ends(String),
    Eq(String),
    Regex(String),
    Limit(usize),

    // Jq operations (JSON-based)
    Jq(JqConfig),
    // TODO: Add ProtoBuf support
    // ProtoBuf {
    //     proto: String,
    //     message: String,
    //     expression: String,
    // },
}

impl Operation {
    pub fn into_predicates<ReqBody>(
        self,
        inner: CorePredicate<ReqBody>,
    ) -> Result<CorePredicate<ReqBody>, ConfigError>
    where
        ReqBody: HttpBody + Send + Unpin + 'static,
        ReqBody::Error: std::fmt::Debug + Send,
        ReqBody::Data: Send,
    {
        match self {
            // Plain operations - convert String to Bytes
            Operation::Contains(s) => Ok(Box::new(inner.body(BodyOperation::Plain(
                PlainOperation::Contains(Bytes::from(s)),
            )))),
            Operation::Starts(s) => Ok(Box::new(inner.body(BodyOperation::Plain(
                PlainOperation::Starts(Bytes::from(s)),
            )))),
            Operation::Ends(s) => Ok(Box::new(inner.body(BodyOperation::Plain(
                PlainOperation::Ends(Bytes::from(s)),
            )))),
            Operation::Eq(s) => Ok(Box::new(inner.body(BodyOperation::Plain(
                PlainOperation::Eq(Bytes::from(s)),
            )))),
            Operation::Regex(pattern) => {
                let regex =
                    regex::bytes::Regex::new(&pattern).map_err(|e| ConfigError::InvalidRegex {
                        pattern,
                        error: e,
                    })?;
                Ok(Box::new(
                    inner.body(BodyOperation::Plain(PlainOperation::RegExp(regex))),
                ))
            }
            Operation::Limit(bytes) => {
                Ok(Box::new(inner.body(BodyOperation::Limit { bytes })))
            }

            // Jq operations
            Operation::Jq(jq_config) => {
                let (expression_str, operation) = match jq_config {
                    JqConfig::Implicit(expr) => {
                        // Implicit syntax: "expression" -> { expression: "...", eq: true }
                        (expr, JqOperation::Eq(serde_json::json!(true)))
                    }
                    JqConfig::Explicit { expression, operation } => {
                        // Explicit syntax: convert operation config to JqOperation
                        let op = match operation {
                            JqOperationConfig::Eq(value) => JqOperation::Eq(value),
                            JqOperationConfig::Exist => JqOperation::Exist,
                            JqOperationConfig::In(values) => JqOperation::In(values),
                        };
                        (expression, op)
                    }
                };

                // Compile the jq expression
                let expression = JqExpression::compile(&expression_str)
                    .map_err(|e| ConfigError::InvalidPredicate(e))?;

                Ok(Box::new(
                    inner.body(BodyOperation::Jq {
                        filter: expression,
                        operation,
                    }),
                ))
            }
        }
    }
}
