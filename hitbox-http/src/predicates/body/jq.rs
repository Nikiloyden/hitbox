use std::fmt::Debug;

use hitbox::predicate::PredicateResult;
use hyper::body::Body as HttpBody;
use jaq_core::{
    Ctx, Filter, Native, RcIter,
    load::{Arena, File, Loader},
};
use jaq_json::{self, Val};
use serde_json::Value;

use crate::BufferedBody;

/// Wrapper around a compiled jq expression.
/// This allows us to compile the expression once and reuse it.
#[derive(Clone)]
pub struct JqExpression(Filter<Native<Val>>);

impl JqExpression {
    /// Compile a jq expression into a reusable filter.
    pub fn compile(expression: &str) -> Result<Self, String> {
        let program = File {
            code: expression,
            path: (),
        };
        let loader = Loader::new(jaq_std::defs().chain(jaq_json::defs()));
        let arena = Arena::default();
        let modules = loader
            .load(&arena, program)
            .map_err(|e| format!("Failed to load jq program: {:?}", e))?;
        let filter = jaq_core::Compiler::default()
            .with_funs(jaq_std::funs().chain(jaq_json::funs()))
            .compile(modules)
            .map_err(|e| format!("Failed to compile jq program: {:?}", e))?;
        Ok(Self(filter))
    }

    /// Apply the filter to a JSON value and return the result.
    pub fn apply(&self, input: Value) -> Option<Value> {
        let inputs = RcIter::new(core::iter::empty());
        let out = self.0.run((Ctx::new([], &inputs), Val::from(input)));
        let results: Result<Vec<_>, _> = out.collect();
        match results {
            Ok(values) if values.eq(&vec![Val::Null]) => None,
            Ok(values) if !values.is_empty() => {
                let mut values: Vec<Value> = values.into_iter().map(|v| v.into()).collect();
                if values.len() == 1 {
                    values.pop()
                } else {
                    Some(Value::Array(values))
                }
            }
            _ => None,
        }
    }
}

impl Debug for JqExpression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JqExpression").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub enum JqOperation {
    Eq(Value),
    Exist,
    In(Vec<Value>),
}

impl JqOperation {
    /// Check if the jq operation matches the body.
    /// Returns `PredicateResult::Cacheable` if the operation is satisfied,
    /// `PredicateResult::NonCacheable` otherwise.
    pub async fn check<B>(
        &self,
        filter: &JqExpression,
        body: BufferedBody<B>,
    ) -> PredicateResult<BufferedBody<B>>
    where
        B: HttpBody + Unpin,
        B::Data: Send,
        B::Error: Debug,
    {
        // Collect the full body to parse as JSON
        let body_bytes = match body.collect().await {
            Ok(bytes) => bytes,
            Err(error_body) => return PredicateResult::NonCacheable(error_body),
        };

        // Parse body as JSON
        let json_value: Value = match serde_json::from_slice(&body_bytes) {
            Ok(v) => v,
            Err(_) => {
                // Failed to parse JSON - non-cacheable
                return PredicateResult::NonCacheable(BufferedBody::Complete(Some(body_bytes)));
            }
        };

        // Apply the jq filter
        let found_value = filter.apply(json_value);

        // Check if the operation matches
        let matches = match self {
            JqOperation::Eq(expected) => {
                found_value.as_ref().map(|v| v == expected).unwrap_or(false)
            }
            JqOperation::Exist => found_value.is_some(),
            JqOperation::In(values) => found_value
                .as_ref()
                .map(|v| values.contains(v))
                .unwrap_or(false),
        };

        let result_body = BufferedBody::Complete(Some(body_bytes));
        if matches {
            PredicateResult::Cacheable(result_body)
        } else {
            PredicateResult::NonCacheable(result_body)
        }
    }
}
