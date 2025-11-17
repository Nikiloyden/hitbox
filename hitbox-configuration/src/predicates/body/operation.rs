use crate::error::ConfigError;
use bytes::Bytes;
use hitbox_http::predicates::body::{
    JqExpression, JqOperation, Operation as BodyOperation, PlainOperation,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

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
    Exist(bool),
    In(Vec<JsonValue>),
}

/// Body predicate operation - supports both plain (byte-based) and jq (JSON-based) operations
/// This is shared between request and response body predicates
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BodyPredicate {
    // Plain operations (byte-based)
    // Note: Stored as String in config, converted to Bytes when creating predicates
    // !!binary tags with non-UTF-8 data currently unsupported due to serde-saphyr limitation
    Contains(String),
    Starts(String),
    Ends(String),
    Eq(String),
    Regex(String),
    Limit(usize),

    // Jq operations (JSON-based)
    Jq(JqConfig),
}

impl BodyPredicate {
    /// Convert configuration into predicates using the BodyPredicate trait
    /// Generic over any type P that implements hitbox_http's BodyPredicate trait
    pub fn into_predicates<P>(
        self,
        inner: P,
    ) -> Result<hitbox_http::predicates::body::Body<P>, ConfigError>
    where
        P: hitbox_http::predicates::body::BodyPredicate,
    {
        let operation = self.into_body_operation()?;
        Ok(inner.body(operation))
    }

    /// Convert configuration to hitbox_http body operation
    fn into_body_operation(self) -> Result<BodyOperation, ConfigError> {
        match self {
            // Plain operations - convert String to Bytes
            BodyPredicate::Contains(s) => Ok(BodyOperation::Plain(PlainOperation::Contains(
                Bytes::from(s),
            ))),
            BodyPredicate::Starts(s) => {
                Ok(BodyOperation::Plain(PlainOperation::Starts(Bytes::from(s))))
            }
            BodyPredicate::Ends(s) => {
                Ok(BodyOperation::Plain(PlainOperation::Ends(Bytes::from(s))))
            }
            BodyPredicate::Eq(s) => Ok(BodyOperation::Plain(PlainOperation::Eq(Bytes::from(s)))),
            BodyPredicate::Regex(pattern) => {
                let regex = regex::bytes::Regex::new(&pattern)
                    .map_err(|e| ConfigError::InvalidRegex { pattern, error: e })?;
                Ok(BodyOperation::Plain(PlainOperation::RegExp(regex)))
            }
            BodyPredicate::Limit(bytes) => Ok(BodyOperation::Limit { bytes }),

            // Jq operations
            BodyPredicate::Jq(jq_config) => {
                let (expression_str, operation) = match jq_config {
                    JqConfig::Implicit(expr) => {
                        // Implicit syntax: "expression" -> { expression: "...", eq: true }
                        (expr, JqOperation::Eq(serde_json::json!(true)))
                    }
                    JqConfig::Explicit {
                        expression,
                        operation,
                    } => {
                        // Explicit syntax: convert operation config to JqOperation
                        let op = match operation {
                            JqOperationConfig::Eq(value) => JqOperation::Eq(value),
                            JqOperationConfig::Exist(_) => JqOperation::Exist,
                            JqOperationConfig::In(values) => JqOperation::In(values),
                        };
                        (expression, op)
                    }
                };

                // Compile the jq expression
                let expression = JqExpression::compile(&expression_str)
                    .map_err(ConfigError::InvalidPredicate)?;

                Ok(BodyOperation::Jq {
                    filter: expression,
                    operation,
                })
            }
        }
    }
}
