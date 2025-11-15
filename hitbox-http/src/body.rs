//! HTTP body buffering and streaming utilities for transparent caching.
//!
//! # Design Rationale: Cache Layer Transparency
//!
//! Hitbox aims to be a transparent caching layer - upstream services and clients
//! should see the same behavior with or without the cache.
//!
//! ## Why `BufferedBody::Partial` exists
//!
//! When predicates inspect request/response bodies, they consume bytes from the stream.
//! To maintain transparency, these bytes must be forwarded to upstream. Since you can't
//! "un-read" from a stream, we must:
//!
//! 1. Buffer the consumed prefix
//! 2. Preserve the unconsumed remaining stream
//! 3. Replay prefix + remaining to upstream
//!
//! This enables:
//! - Resumable uploads/downloads (e.g., large file transfers)
//! - Accurate error reporting (errors occur at the same byte position)
//! - Zero data loss or corruption
//! - Support for partial transfer protocols (HTTP Range requests, etc.)
//!
//! ## Example: Large File Upload
//!
//! ```text
//! Without cache:
//!   Client → Upstream: 500MB uploaded, error at byte 300MB
//!
//! With transparent cache:
//!   Client → Hitbox (reads 10MB for predicate) → Upstream
//!   Upstream receives: 10MB (replayed) + 290MB (streamed) + error
//!   Total: Same 300MB, same error position ✅
//! ```
//!
//! ## Body States
//!
//! - **Complete**: Body was fully read and buffered (within configured size limits)
//! - **Partial**: Body was partially read - contains buffered prefix plus remaining stream or error
//! - **Passthrough**: Body was not inspected at all (zero overhead)
//!
//! The `Partial` state is critical for maintaining transparency when:
//! - Body size exceeds configured limits but must still be forwarded
//! - Network or upstream errors occur mid-stream
//! - Predicates need to inspect body content without blocking large transfers

use bytes::{Buf, Bytes, BytesMut};
use http_body::{Body as HttpBody, Frame};
use pin_project::pin_project;
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Enum to represent the remaining body state after partial consumption.
#[pin_project(project = RemainingProj)]
pub enum Remaining<B>
where
    B: HttpBody,
{
    /// The body stream continues
    Body(#[pin] B),
    /// An error was encountered during consumption - yield once then end stream
    Error(Option<B::Error>),
}

/// Represents a partially consumed body with a buffered prefix and remaining stream.
///
/// This type acts as both a data structure and a streamable body, implementing `HttpBody`
/// to yield the prefix first, then stream from the remaining body.
#[pin_project]
pub struct PartialBufferedBody<B>
where
    B: HttpBody,
{
    prefix: Option<Bytes>,
    #[pin]
    remaining: Remaining<B>,
}

impl<B> PartialBufferedBody<B>
where
    B: HttpBody,
{
    pub fn new(prefix: Option<Bytes>, remaining: Remaining<B>) -> Self {
        Self { prefix, remaining }
    }

    pub fn prefix(&self) -> Option<&Bytes> {
        self.prefix.as_ref()
    }

    pub fn into_parts(self) -> (Option<Bytes>, Remaining<B>) {
        (self.prefix, self.remaining)
    }

    /// Attempts to collect this partial body up to the specified limit.
    ///
    /// Returns:
    /// - `Ok(BufferedBody)` if the body is within limit or an error prevents further reading
    /// - `Err(PartialBody)` if the body exceeds the limit (may have different inner type)
    pub async fn collect_partial(self, limit_bytes: usize) -> Result<BufferedBody<B>, BufferedBody<B>>
    where
        B: Unpin,
    {
        let prefix_len = self.prefix.as_ref().map(|b| b.len()).unwrap_or(0);

        // If prefix already exceeds limit, return immediately
        if prefix_len > limit_bytes {
            return Err(BufferedBody::Partial(self));
        }

        match self.remaining {
            // If there's an error, we can't read more - return as-is wrapped in BufferedBody
            Remaining::Error(_) => Ok(BufferedBody::Partial(self)),

            // Need to read from remaining stream
            Remaining::Body(stream) => {
                // Use collect_stream on the inner stream with the remaining limit
                let remaining_limit = limit_bytes - prefix_len;
                match collect_stream(stream, Some(remaining_limit)).await {
                    CollectResult::Complete(new_data) => {
                        // Combine prefix with newly read data
                        let combined = match self.prefix {
                            Some(prefix_bytes) if !new_data.is_empty() => {
                                let mut buf = BytesMut::from(prefix_bytes.as_ref());
                                buf.extend_from_slice(&new_data);
                                buf.freeze()
                            }
                            Some(prefix_bytes) => prefix_bytes,
                            None => new_data,
                        };

                        if combined.len() <= limit_bytes {
                            Ok(BufferedBody::Complete(
                                if combined.is_empty() { None } else { Some(combined) }
                            ))
                        } else {
                            Err(BufferedBody::Complete(Some(combined)))
                        }
                    }
                    CollectResult::ExceedsLimit { buffered: new_data, remaining: new_stream } => {
                        // Combine prefix with new data
                        let combined = match self.prefix {
                            Some(prefix_bytes) if !new_data.is_empty() => {
                                let mut buf = BytesMut::from(prefix_bytes.as_ref());
                                buf.extend_from_slice(&new_data);
                                buf.freeze()
                            }
                            Some(prefix_bytes) => prefix_bytes,
                            None => new_data,
                        };

                        Err(BufferedBody::Partial(PartialBufferedBody::new(
                            if combined.is_empty() { None } else { Some(combined) },
                            Remaining::Body(new_stream),
                        )))
                    }
                    CollectResult::Error { buffered: new_data, error } => {
                        // Combine prefix with new data
                        let combined = match self.prefix {
                            Some(prefix_bytes) if !new_data.is_empty() => {
                                let mut buf = BytesMut::from(prefix_bytes.as_ref());
                                buf.extend_from_slice(&new_data);
                                buf.freeze()
                            }
                            Some(prefix_bytes) => prefix_bytes,
                            None => new_data,
                        };

                        Ok(BufferedBody::Partial(PartialBufferedBody::new(
                            if combined.is_empty() { None } else { Some(combined) },
                            Remaining::Error(Some(error)),
                        )))
                    }
                }
            }
        }
    }
}

impl<B: HttpBody> HttpBody for PartialBufferedBody<B> {
    type Data = Bytes;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();

        // First, yield the prefix if present
        if let Some(prefix) = this.prefix.take() {
            return Poll::Ready(Some(Ok(Frame::data(prefix))));
        }

        // Then handle the remaining body or error
        match this.remaining.project() {
            RemainingProj::Body(body) => {
                match body.poll_frame(cx) {
                    Poll::Ready(Some(Ok(frame))) => {
                        let frame = frame.map_data(|mut data| data.copy_to_bytes(data.remaining()));
                        Poll::Ready(Some(Ok(frame)))
                    }
                    Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            }
            RemainingProj::Error(error) => {
                if let Some(err) = error.take() {
                    Poll::Ready(Some(Err(err)))
                } else {
                    Poll::Ready(None)
                }
            }
        }
    }

    fn size_hint(&self) -> http_body::SizeHint {
        let prefix_len = self.prefix.as_ref().map(|b| b.len() as u64).unwrap_or(0);

        match &self.remaining {
            Remaining::Body(body) => {
                let mut hint = body.size_hint();
                hint.set_lower(hint.lower().saturating_add(prefix_len));
                if let Some(upper) = hint.upper() {
                    hint.set_upper(upper.saturating_add(prefix_len));
                }
                hint
            }
            Remaining::Error(_) => http_body::SizeHint::with_exact(prefix_len),
        }
    }

    fn is_end_stream(&self) -> bool {
        if self.prefix.is_some() {
            return false;
        }

        match &self.remaining {
            Remaining::Body(body) => body.is_end_stream(),
            Remaining::Error(err) => err.is_none(),
        }
    }
}

/// A body wrapper that represents different consumption states.
///
/// This enum allows predicates to partially consume request or response bodies
/// without losing data. The complete body (including any buffered prefix) is
/// forwarded to upstream services.
///
/// # Variants
///
/// - [`Complete`](BufferedBody::Complete): Body was fully read and buffered (within size limits)
/// - [`Partial`](BufferedBody::Partial): Body was partially read - has buffered prefix plus
///   remaining stream or error
/// - [`Passthrough`](BufferedBody::Passthrough): Body was not read at all (untouched)
#[pin_project(project = BufferedBodyProj)]
pub enum BufferedBody<B>
where
    B: HttpBody,
{
    /// Body was fully read and buffered (within size limits).
    ///
    /// The `Option` is used to yield the data once, then return `None` on subsequent polls.
    Complete(Option<Bytes>),

    /// Body was partially read - contains buffered prefix and remaining stream.
    ///
    /// The `PartialBufferedBody` handles streaming of both the prefix and remaining data.
    Partial(#[pin] PartialBufferedBody<B>),

    /// Body was passed through without reading (untouched).
    ///
    /// The body is forwarded directly to upstream without any buffering.
    Passthrough(#[pin] B),
}

impl<B> HttpBody for BufferedBody<B>
where
    B: HttpBody,
{
    type Data = Bytes;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.project() {
            BufferedBodyProj::Complete(data) => {
                if let Some(bytes) = data.take() {
                    Poll::Ready(Some(Ok(Frame::data(bytes))))
                } else {
                    Poll::Ready(None)
                }
            }

            BufferedBodyProj::Partial(partial) => {
                // Delegate to PartialBody's HttpBody implementation
                partial.poll_frame(cx)
            }

            BufferedBodyProj::Passthrough(body) => {
                // Delegate to the inner body and convert Data type
                match body.poll_frame(cx) {
                    Poll::Ready(Some(Ok(frame))) => {
                        let frame = frame.map_data(|mut data| data.copy_to_bytes(data.remaining()));
                        Poll::Ready(Some(Ok(frame)))
                    }
                    Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }

    fn size_hint(&self) -> http_body::SizeHint {
        match self {
            BufferedBody::Complete(Some(bytes)) => {
                let len = bytes.len() as u64;
                http_body::SizeHint::with_exact(len)
            }
            BufferedBody::Complete(None) => http_body::SizeHint::with_exact(0),

            BufferedBody::Partial(partial) => partial.size_hint(),

            BufferedBody::Passthrough(body) => body.size_hint(),
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            BufferedBody::Complete(None) => true,
            BufferedBody::Complete(Some(_)) => false,

            BufferedBody::Partial(partial) => partial.is_end_stream(),

            BufferedBody::Passthrough(body) => body.is_end_stream(),
        }
    }
}

/// Result of collecting a stream with an optional limit
enum CollectResult<B: HttpBody> {
    /// Body completed within limit (or no limit)
    Complete(Bytes),
    /// Body exceeded limit - contains buffered data and remaining stream
    ExceedsLimit { buffered: Bytes, remaining: B },
    /// Error occurred during reading - contains buffered data and error
    Error { buffered: Bytes, error: B::Error },
}

/// Helper function to read body stream chunks with an optional size limit
async fn collect_stream<B>(mut stream: B, limit: Option<usize>) -> CollectResult<B>
where
    B: HttpBody + Unpin,
{
    use http_body_util::BodyExt;

    // Check size_hint if we have a limit
    if let Some(limit_bytes) = limit
        && let Some(upper) = stream.size_hint().upper()
        && upper > limit_bytes as u64
    {
        // Size hint indicates body exceeds limit
        return CollectResult::ExceedsLimit {
            buffered: Bytes::new(),
            remaining: stream,
        };
    }

    let mut buffer = BytesMut::new();

    loop {
        match stream.frame().await {
            Some(Ok(frame)) => {
                if let Ok(mut data) = frame.into_data() {
                    let data_len = data.remaining();

                    // Check if adding this chunk would exceed limit
                    if let Some(limit_bytes) = limit
                        && buffer.len() + data_len > limit_bytes
                    {
                        // Exceeds limit - buffer this chunk and return
                        while data.has_remaining() {
                            buffer.extend_from_slice(data.chunk());
                            data.advance(data.chunk().len());
                        }
                        return CollectResult::ExceedsLimit {
                            buffered: buffer.freeze(),
                            remaining: stream,
                        };
                    }

                    // Within limit (or no limit) - buffer the chunk
                    while data.has_remaining() {
                        buffer.extend_from_slice(data.chunk());
                        data.advance(data.chunk().len());
                    }
                }
                // Ignore trailers and other frame types
            }
            Some(Err(err)) => {
                // Error reading body
                return CollectResult::Error {
                    buffered: buffer.freeze(),
                    error: err,
                };
            }
            None => {
                // Body complete
                return CollectResult::Complete(buffer.freeze());
            }
        }
    }
}

impl<B> BufferedBody<B>
where
    B: HttpBody,
{
    /// Collects the entire body into bytes, handling errors properly.
    ///
    /// This method consumes the body and returns:
    /// - `Ok(bytes)` if collection succeeds
    /// - `Err(Self)` with `BufferedBody::Partial` containing the error if collection fails
    pub async fn collect(self) -> Result<Bytes, Self>
    where
        B::Data: Send,
    {
        use http_body_util::BodyExt;

        match self {
            // Already complete, extract bytes
            BufferedBody::Complete(Some(bytes)) => Ok(bytes),
            BufferedBody::Complete(None) => Ok(Bytes::new()),

            // Passthrough - need to collect
            BufferedBody::Passthrough(body) => match body.collect().await {
                Ok(collected) => Ok(collected.to_bytes()),
                Err(err) => Err(BufferedBody::Partial(PartialBufferedBody::new(
                    None,
                    Remaining::Error(Some(err)),
                ))),
            },

            // Partial - delegate to PartialBody which implements HttpBody
            BufferedBody::Partial(partial) => {
                let (prefix, remaining) = partial.into_parts();
                match remaining {
                    Remaining::Body(body) => match body.collect().await {
                        Ok(collected) => {
                            if let Some(prefix_bytes) = prefix {
                                let mut combined = BytesMut::from(prefix_bytes.as_ref());
                                combined.extend_from_slice(&collected.to_bytes());
                                Ok(combined.freeze())
                            } else {
                                Ok(collected.to_bytes())
                            }
                        }
                        Err(err) => Err(BufferedBody::Partial(PartialBufferedBody::new(
                            prefix,
                            Remaining::Error(Some(err)),
                        ))),
                    },
                    Remaining::Error(err) => Err(BufferedBody::Partial(PartialBufferedBody::new(
                        prefix,
                        Remaining::Error(err),
                    ))),
                }
            }
        }
    }

    /// Collects body up to a specified limit.
    ///
    /// This method reads the body in chunks until either:
    /// - The body completes within the limit → Ok(Complete)
    /// - The body exactly reaches the limit → Ok(Partial)
    /// - The body exceeds the limit → Err(Partial) with buffered prefix + remaining stream
    ///
    /// Errors during reading are stored in `Remaining::Error` and returned as Ok(Partial).
    ///
    /// # Arguments
    /// * `limit_bytes` - Maximum number of bytes to buffer
    ///
    /// # Returns
    /// * `Ok(BufferedBody::Complete)` - Body completed within limit
    /// * `Ok(BufferedBody::Partial)` - Body reached limit or encountered error
    /// * `Err(BufferedBody::Partial)` - Body exceeded limit (contains buffered data + remaining stream)
    pub async fn collect_partial(self, limit_bytes: usize) -> Result<Self, Self>
    where
        B: Unpin,
    {
        match self {
            // Already complete - just check size
            BufferedBody::Complete(Some(ref data)) => {
                if data.len() <= limit_bytes {
                    Ok(self)
                } else {
                    Err(self)
                }
            }
            BufferedBody::Complete(None) => {
                // Empty body is always within limit
                Ok(self)
            }

            // Delegate to PartialBody's collect_partial method
            BufferedBody::Partial(partial) => partial.collect_partial(limit_bytes).await,

            // Passthrough - need to read and check
            BufferedBody::Passthrough(stream) => {
                match collect_stream(stream, Some(limit_bytes)).await {
                    CollectResult::Complete(buffered) => {
                        // Body completed, check if it's within limit
                        if buffered.len() <= limit_bytes {
                            Ok(BufferedBody::Complete(if buffered.is_empty() {
                                None
                            } else {
                                Some(buffered)
                            }))
                        } else {
                            // Entire body exceeded limit but we buffered it all
                            Err(BufferedBody::Complete(Some(buffered)))
                        }
                    }
                    CollectResult::ExceedsLimit {
                        buffered,
                        remaining,
                    } => {
                        // Body exceeds limit
                        if buffered.is_empty() {
                            Err(BufferedBody::Passthrough(remaining))
                        } else {
                            Err(BufferedBody::Partial(PartialBufferedBody::new(
                                Some(buffered),
                                Remaining::Body(remaining),
                            )))
                        }
                    }
                    CollectResult::Error { buffered, error } => {
                        // Error reading body
                        Ok(BufferedBody::Partial(PartialBufferedBody::new(
                            if buffered.is_empty() {
                                None
                            } else {
                                Some(buffered)
                            },
                            Remaining::Error(Some(error)),
                        )))
                    }
                }
            }
        }
    }
}

impl<B> fmt::Debug for BufferedBody<B>
where
    B: HttpBody,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BufferedBody::Complete(Some(bytes)) => f
                .debug_tuple("Complete")
                .field(&format!("{} bytes", bytes.len()))
                .finish(),
            BufferedBody::Complete(None) => f.debug_tuple("Complete").field(&"consumed").finish(),
            BufferedBody::Partial(partial) => {
                let prefix_len = partial.prefix().map(|b| b.len()).unwrap_or(0);
                f.debug_struct("Partial")
                    .field("prefix_len", &prefix_len)
                    .field("remaining", &"...")
                    .finish()
            }
            BufferedBody::Passthrough(_) => f.debug_tuple("Passthrough").field(&"...").finish(),
        }
    }
}
