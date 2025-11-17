use std::fmt::Debug;

use async_trait::async_trait;
use bytes::{Buf, Bytes, BytesMut};
use hitbox::predicate::{Predicate, PredicateResult};
use http::Response;
use http_body_util::BodyExt;
use hyper::body::Body as HttpBody;
use jaq_core::{
    self, Ctx, Filter, Native, RcIter,
    load::{Arena, File, Loader},
};
use jaq_json::{self, Val};
// use prost_reflect::MessageDescriptor;
use serde_json::Value;

use crate::{BufferedBody, CacheableHttpResponse, PartialBufferedBody, Remaining};

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
                let values: Vec<Value> = values.into_iter().map(|v| v.into()).collect();
                if values.len() == 1 {
                    Some(values.into_iter().next().unwrap())
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

/// Searches for a pattern in a body stream without collecting the entire body.
/// Returns (found: bool, buffered_body)
///
/// This function optimizes the search by:
/// - Stopping early if pattern is found
/// - Handling pattern spanning chunk boundaries with overlap buffer
/// - Preserving errors in Partial bodies for transparency
async fn streaming_search<B>(body: BufferedBody<B>, pattern: &[u8]) -> (bool, BufferedBody<B>)
where
    B: HttpBody + Unpin,
    B::Error: Debug,
    B::Data: Send,
{
    if pattern.is_empty() {
        return (true, body);
    }

    match body {
        // Already complete - just search
        BufferedBody::Complete(Some(bytes)) => {
            let found = bytes.windows(pattern.len()).any(|w| w == pattern);
            (found, BufferedBody::Complete(Some(bytes)))
        }
        BufferedBody::Complete(None) => (false, BufferedBody::Complete(None)),

        // Partial - extract parts and search through prefix + remaining
        BufferedBody::Partial(partial) => {
            let (prefix, remaining) = partial.into_parts();
            match remaining {
                Remaining::Body(body) => streaming_search_body(prefix, body, pattern).await,
                Remaining::Error(error) => {
                    // Already have an error, just search in the prefix we have
                    let found = prefix
                        .as_ref()
                        .map(|b| b.windows(pattern.len()).any(|w| w == pattern))
                        .unwrap_or(false);
                    (
                        found,
                        BufferedBody::Partial(PartialBufferedBody::new(
                            prefix,
                            Remaining::Error(error),
                        )),
                    )
                }
            }
        }

        // Passthrough - stream through it
        BufferedBody::Passthrough(stream) => streaming_search_body(None, stream, pattern).await,
    }
}

/// Helper function that performs streaming search on any Body implementation.
/// Optionally starts with an initial prefix buffer.
async fn streaming_search_body<B>(
    initial_prefix: Option<Bytes>,
    mut body: B,
    pattern: &[u8],
) -> (bool, BufferedBody<B>)
where
    B: HttpBody + Unpin,
    B::Error: Debug,
    B::Data: Send,
{
    let mut buffer = BytesMut::new();

    // Initialize with prefix if provided
    if let Some(prefix_bytes) = initial_prefix {
        buffer.extend_from_slice(&prefix_bytes);
    }

    // Keep last (pattern.len() - 1) bytes to handle pattern spanning chunks
    let overlap_size = pattern.len().saturating_sub(1);

    loop {
        match body.frame().await {
            Some(Ok(frame)) => {
                if let Ok(mut data) = frame.into_data() {
                    // Search in: [overlap from previous] + [current chunk]
                    let search_start = buffer.len().saturating_sub(overlap_size);
                    buffer.extend_from_slice(&data.copy_to_bytes(data.remaining()));

                    // Search in the new region (overlap + new data)
                    if buffer[search_start..]
                        .windows(pattern.len())
                        .any(|w| w == pattern)
                    {
                        // Found! Return complete body with all buffered data
                        return (true, BufferedBody::Complete(Some(buffer.freeze())));
                    }
                }
            }
            Some(Err(error)) => {
                // Error occurred - save buffered data + error in Partial body
                let buffered = if buffer.is_empty() {
                    None
                } else {
                    Some(buffer.freeze())
                };

                // Check if pattern was in buffered data before error
                let found = buffered
                    .as_ref()
                    .map(|b| b.windows(pattern.len()).any(|w| w == pattern))
                    .unwrap_or(false);

                let result_body = BufferedBody::Partial(PartialBufferedBody::new(
                    buffered,
                    Remaining::Error(Some(error)),
                ));
                return (found, result_body);
            }
            None => {
                // End of stream
                let combined = if buffer.is_empty() {
                    None
                } else {
                    Some(buffer.freeze())
                };

                let found = combined
                    .as_ref()
                    .map(|b| b.windows(pattern.len()).any(|w| w == pattern))
                    .unwrap_or(false);

                return (found, BufferedBody::Complete(combined));
            }
        }
    }
}

#[derive(Debug)]
pub enum PlainOperation {
    Eq(Bytes),
    Contains(Bytes),
    Starts(Bytes),
    Ends(Bytes),
    RegExp(regex::bytes::Regex),
}

impl PlainOperation {
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
            PlainOperation::Starts(prefix) => {
                // Empty prefix always matches
                if prefix.is_empty() {
                    return PredicateResult::Cacheable(body);
                }

                // Use collect_exact to read exactly prefix.len() bytes
                use crate::CollectExactResult;

                let result = body.collect_exact(prefix.len()).await;

                // Check if body starts with prefix
                let matches = match &result {
                    CollectExactResult::AtLeast { buffered, .. } => buffered.starts_with(prefix),
                    CollectExactResult::Incomplete { .. } => false, // Not enough bytes
                };

                // Reconstruct body
                let result_body = result.into_buffered_body();

                if matches {
                    PredicateResult::Cacheable(result_body)
                } else {
                    PredicateResult::NonCacheable(result_body)
                }
            }

            PlainOperation::Eq(expected) => body
                .collect()
                .await
                .map(|body_bytes| {
                    let matches = body_bytes.as_ref() == expected.as_ref();
                    let result_body = BufferedBody::Complete(Some(body_bytes));
                    if matches {
                        PredicateResult::Cacheable(result_body)
                    } else {
                        PredicateResult::NonCacheable(result_body)
                    }
                })
                .unwrap_or_else(PredicateResult::NonCacheable),

            PlainOperation::Contains(sequence) => {
                let (found, result_body) = streaming_search(body, sequence.as_ref()).await;
                if found {
                    PredicateResult::Cacheable(result_body)
                } else {
                    PredicateResult::NonCacheable(result_body)
                }
            }

            PlainOperation::Ends(suffix) => body
                .collect()
                .await
                .map(|body_bytes| {
                    let matches = body_bytes.ends_with(suffix);
                    let result_body = BufferedBody::Complete(Some(body_bytes));
                    if matches {
                        PredicateResult::Cacheable(result_body)
                    } else {
                        PredicateResult::NonCacheable(result_body)
                    }
                })
                .unwrap_or_else(PredicateResult::NonCacheable),

            PlainOperation::RegExp(regex) => body
                .collect()
                .await
                .map(|body_bytes| {
                    let matches = regex.is_match(body_bytes.as_ref());
                    let result_body = BufferedBody::Complete(Some(body_bytes));
                    if matches {
                        PredicateResult::Cacheable(result_body)
                    } else {
                        PredicateResult::NonCacheable(result_body)
                    }
                })
                .unwrap_or_else(PredicateResult::NonCacheable),
        }
    }
}

#[derive(Debug, Clone)]
pub enum JqOperation {
    Eq(Value),
    Exist,
    In(Vec<Value>),
}

#[derive(Debug)]
pub enum Operation {
    Limit { bytes: usize },
    Plain(PlainOperation),
    Jq { filter: JqExpression, operation: JqOperation },
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
                if let Some(upper) = body.size_hint().upper() {
                    if upper > *bytes as u64 {
                        // Size hint indicates body exceeds limit - non-cacheable
                        return PredicateResult::NonCacheable(body);
                    }
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
            Operation::Jq { filter, operation } => {
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
                        return PredicateResult::NonCacheable(BufferedBody::Complete(Some(
                            body_bytes,
                        )));
                    }
                };

                // Apply the jq filter
                let found_value = filter.apply(json_value);

                // Check if the operation matches
                let matches = match operation {
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
    }
}

// TODO: Add ProtoBufOperation
// #[derive(Debug)]
// pub enum ParsingType {
//     ProtoBuf(MessageDescriptor),
// }

#[derive(Debug)]
pub struct Body<P> {
    operation: Operation,
    inner: P,
}

pub trait BodyPredicate: Sized {
    fn body(self, operation: Operation) -> Body<Self>;
}

impl<P> BodyPredicate for P
where
    P: Predicate,
{
    fn body(self, operation: Operation) -> Body<Self> {
        Body {
            operation,
            inner: self,
        }
    }
}

#[async_trait]
impl<P, ResBody> Predicate for Body<P>
where
    ResBody: HttpBody + Send + Unpin + 'static,
    P: Predicate<Subject = CacheableHttpResponse<ResBody>> + Send + Sync,
    ResBody::Error: Debug + Send,
    ResBody::Data: Send,
{
    type Subject = P::Subject;

    async fn check(&self, response: Self::Subject) -> PredicateResult<Self::Subject> {
        self.inner
            .check(response)
            .await
            .map(|response| async move {
                let (parts, body) = response.into_response().into_parts();

                // Delegate to Operation::check
                let result = self.operation.check(body).await;

                // Convert back to CacheableHttpResponse
                match result {
                    PredicateResult::Cacheable(buffered_body) => {
                        let http_response = Response::from_parts(parts, buffered_body);
                        PredicateResult::Cacheable(CacheableHttpResponse::from_response(
                            http_response,
                        ))
                    }
                    PredicateResult::NonCacheable(buffered_body) => {
                        let http_response = Response::from_parts(parts, buffered_body);
                        PredicateResult::NonCacheable(CacheableHttpResponse::from_response(
                            http_response,
                        ))
                    }
                }
            })
            .await
    }
}
