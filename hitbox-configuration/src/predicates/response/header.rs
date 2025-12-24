//! Response header predicate configuration.

use hitbox_http::predicates::response::HeaderPredicate;
use hyper::body::Body as HttpBody;

use crate::ResponsePredicate;
use crate::error::ConfigError;
use crate::predicates::header::{header_value_to_operation, parse_header_name};

// Re-export shared types for external use
pub use crate::predicates::header::{HeaderOperation, HeaderValue, HeaderValueOperation};

pub fn into_predicates<ReqBody>(
    headers: HeaderOperation,
    inner: ResponsePredicate<ReqBody>,
) -> Result<ResponsePredicate<ReqBody>, ConfigError>
where
    ReqBody: HttpBody + Send + 'static,
    ReqBody::Error: std::fmt::Debug + Send,
    ReqBody::Data: Send,
{
    headers.into_iter().try_rfold(
        inner,
        |inner, (header_name, header_value)| -> Result<ResponsePredicate<ReqBody>, ConfigError> {
            let name = parse_header_name(&header_name)?;
            let operation = header_value_to_operation(name, header_value)?;
            Ok(Box::new(inner.header(operation)))
        },
    )
}
