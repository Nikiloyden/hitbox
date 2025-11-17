mod jq;
mod operation;
mod plain;
mod predicate;

pub use jq::{JqExpression, JqOperation};
pub use operation::Operation;
pub use plain::PlainOperation;
pub use predicate::{Body, BodyPredicate};
