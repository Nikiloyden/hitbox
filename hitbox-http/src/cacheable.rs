use hyper::body::Body as HttpBody;

use crate::BufferedBody;

/// Trait for cacheable subjects that can be decomposed into parts and a BufferedBody,
/// and reconstructed from those parts.
///
/// This enables generic body predicate implementations that work with both
/// requests and responses.
pub trait CacheableSubject {
    /// The HTTP body type
    type Body: HttpBody;

    /// The type representing the non-body parts (e.g., request::Parts or response::Parts)
    type Parts;

    /// Decompose into parts and body
    fn into_parts(self) -> (Self::Parts, BufferedBody<Self::Body>);

    /// Reconstruct from parts and body
    fn from_parts(parts: Self::Parts, body: BufferedBody<Self::Body>) -> Self;
}
