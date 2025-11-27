use hyper::body::Body as HttpBody;
use serde::{Deserialize, Serialize};

use super::{HeaderOperation, MethodOperation, PathOperation, QueryOperation, header};
use crate::predicates::body::BodyOperationConfig;
use crate::{RequestPredicate, error::ConfigError};

// Use standard externally-tagged enum (serde default)
// YAML syntax: Method: {...}, Path: "...", Query: {...}, etc.
#[derive(Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum Predicate {
    Method(MethodOperation),
    Path(PathOperation),
    Query(QueryOperation),
    Header(HeaderOperation),
    Body(BodyOperationConfig),
}

impl Predicate {
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
            Predicate::Method(method_operation) => method_operation.into_predicates(inner),
            Predicate::Path(path_operation) => path_operation.into_predicates(inner),
            Predicate::Query(query_operation) => query_operation.into_predicates(inner),
            Predicate::Header(header_operation) => header::into_predicates(header_operation, inner),
            Predicate::Body(body_predicate) => Ok(Box::new(body_predicate.into_predicates(inner)?)),
        }
    }
}
