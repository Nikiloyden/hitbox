use std::fmt::Debug;

use hitbox::predicate::PredicateResult;
use hyper::body::Body as HttpBody;

use super::{
    jq::{JqExpression, JqOperation},
    plain::PlainOperation,
};
use crate::BufferedBody;

#[derive(Debug)]
pub enum Operation {
    Limit {
        bytes: usize,
    },
    Plain(PlainOperation),
    Jq {
        filter: JqExpression,
        operation: JqOperation,
    },
}

impl Operation {
    /// Check if the operation matches the body.
    /// Returns `PredicateResult::Cacheable` if the operation is satisfied,
    /// `PredicateResult::NonCacheable` otherwise.
    pub async fn check<B>(&self, body: BufferedBody<B>) -> PredicateResult<BufferedBody<B>>
    where
        B: HttpBody + Unpin,
        B::Data: Send,
        B::Error: Debug,
    {
        match self {
            Operation::Limit { bytes } => {
                use crate::CollectExactResult;

                // Check size hint first for optimization
                if let Some(upper) = body.size_hint().upper()
                    && upper > *bytes as u64
                {
                    // Size hint indicates body exceeds limit - non-cacheable
                    return PredicateResult::NonCacheable(body);
                }

                // Try to read limit+1 bytes to check if body exceeds limit
                let result = body.collect_exact(*bytes + 1).await;

                match result {
                    CollectExactResult::AtLeast { .. } => {
                        // Got limit+1 bytes, so body exceeds limit
                        PredicateResult::NonCacheable(result.into_buffered_body())
                    }
                    CollectExactResult::Incomplete { ref error, .. } => {
                        let is_error = error.is_some();
                        let body = result.into_buffered_body();
                        if is_error {
                            // Error occurred - non-cacheable
                            PredicateResult::NonCacheable(body)
                        } else {
                            // Within limit, no error - cacheable
                            PredicateResult::Cacheable(body)
                        }
                    }
                }
            }
            Operation::Plain(plain_op) => plain_op.check(body).await,
            Operation::Jq { filter, operation } => operation.check(filter, body).await,
        }
    }
}
