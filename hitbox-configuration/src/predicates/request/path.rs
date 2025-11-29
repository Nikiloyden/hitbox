//! Path predicate configuration.

use hitbox_http::predicates::NeutralRequestPredicate;
use hitbox_http::predicates::conditions::Or;
use hitbox_http::predicates::request::Path;
use hyper::body::Body as HttpBody;
use serde::{Deserialize, Serialize};

use crate::{RequestPredicate, error::ConfigError};

/// Path predicate operation.
///
/// Supports both single pattern and list of patterns:
/// ```yaml
/// # Single pattern (backwards compatible)
/// - Path: "/api/v1/{resource}/{id}"
///
/// # Multiple patterns
/// - Path:
///     in:
///       - "/api/v1/users"
///       - "/api/v2/users"
/// ```
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(untagged)]
pub enum PathOperation {
    /// Single pattern: `Path: "/api/{id}"`
    Pattern(String),
    /// Multiple patterns: `Path: { in: [...] }`
    In { r#in: Vec<String> },
}

impl PathOperation {
    pub fn into_predicates<ReqBody>(
        self,
        inner: RequestPredicate<ReqBody>,
    ) -> Result<RequestPredicate<ReqBody>, ConfigError>
    where
        ReqBody: HttpBody + Send + Unpin + 'static,
        ReqBody::Error: std::fmt::Debug + Send,
        ReqBody::Data: Send,
    {
        match self {
            PathOperation::Pattern(pattern) => Ok(Box::new(Path::new(inner, pattern.into()))),
            PathOperation::In { r#in: patterns } => patterns
                .into_iter()
                .map(|pattern| -> RequestPredicate<ReqBody> {
                    Box::new(Path::new(
                        Box::new(NeutralRequestPredicate::new()),
                        pattern.into(),
                    ))
                })
                .reduce(|acc, predicate| {
                    Box::new(Or::new(
                        predicate,
                        acc,
                        Box::new(NeutralRequestPredicate::new()),
                    ))
                })
                .ok_or(ConfigError::EmptyPathList),
        }
    }
}
