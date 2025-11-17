pub mod body;
pub mod header;
pub mod status;

pub use body::{Body, BodyPredicate, JqFilter};
pub use header::{Header, HeaderPredicate};
pub use status::{StatusClass, StatusCode, StatusCodePredicate};

// Re-export shared body types for convenience
pub use crate::predicates::body::{JqExpression, JqOperation, Operation, PlainOperation};
