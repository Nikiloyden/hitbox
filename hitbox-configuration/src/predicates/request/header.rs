//! Request header predicate configuration.

use hitbox_http::predicates::request::HeaderPredicate;

use crate::predicates::header::{header_value_to_operation, parse_header_name};
use crate::{RequestPredicate, error::ConfigError};

// Re-export shared types for external use
pub use crate::predicates::header::HeaderOperation;

pub fn into_predicates<ReqBody>(
    headers: HeaderOperation,
    inner: RequestPredicate<ReqBody>,
) -> Result<RequestPredicate<ReqBody>, ConfigError>
where
    ReqBody: hyper::body::Body + Send + 'static,
    ReqBody::Error: Send,
    ReqBody::Data: Send,
{
    headers.into_iter().try_rfold(
        inner,
        |inner, (header_name, header_value)| -> Result<RequestPredicate<ReqBody>, ConfigError> {
            let name = parse_header_name(&header_name)?;
            let operation = header_value_to_operation(name, header_value)?;
            Ok(Box::new(inner.header(operation)))
        },
    )
}
