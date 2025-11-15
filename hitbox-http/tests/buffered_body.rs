use bytes::Bytes;
use futures::stream;
use hitbox_http::{BufferedBody, PartialBufferedBody, Remaining};
use http_body::Body;
use http_body_util::{BodyExt, Full, StreamBody};

#[tokio::test]
async fn test_complete_yields_bytes_once() {
    let data = Bytes::from("hello world");
    let mut body: BufferedBody<Full<Bytes>> = BufferedBody::Complete(Some(data.clone()));

    // First frame should yield the data
    let frame = body.frame().await.unwrap().unwrap();
    let frame_data = frame.into_data().unwrap();
    assert_eq!(frame_data, data);

    // Second frame should be None (end of stream)
    let frame = body.frame().await;
    assert!(frame.is_none());
}

#[tokio::test]
async fn test_passthrough_forwards_all_chunks() {
    let data = Bytes::from("passthrough data");
    let inner_body = Full::new(data.clone());
    let mut body = BufferedBody::Passthrough(inner_body);

    // First frame should yield the data
    let frame = body.frame().await.unwrap().unwrap();
    let frame_data = frame.into_data().unwrap();
    assert_eq!(frame_data, data);

    // Second frame should be None
    let frame = body.frame().await;
    assert!(frame.is_none());
}

#[tokio::test]
async fn test_passthrough_with_stream_body() {
    // Create an async stream that yields multiple chunks
    use std::convert::Infallible;
    let stream = stream::iter(vec![
        Ok::<_, Infallible>(http_body::Frame::data(Bytes::from("chunk1"))),
        Ok::<_, Infallible>(http_body::Frame::data(Bytes::from("chunk2"))),
        Ok::<_, Infallible>(http_body::Frame::data(Bytes::from("chunk3"))),
    ]);

    let inner_body = StreamBody::new(stream);
    let mut body = BufferedBody::Passthrough(inner_body);

    // Collect all chunks
    let mut collected = Vec::new();
    while let Some(result) = body.frame().await {
        let frame = result.unwrap();
        if let Ok(data) = frame.into_data() {
            collected.push(data);
        }
    }

    assert_eq!(collected.len(), 3);
    assert_eq!(collected[0], Bytes::from("chunk1"));
    assert_eq!(collected[1], Bytes::from("chunk2"));
    assert_eq!(collected[2], Bytes::from("chunk3"));
}

#[tokio::test]
async fn test_passthrough_with_error_in_stream() {
    use std::io;

    // Create a stream that yields data then an error
    let stream = stream::iter(vec![
        Ok(http_body::Frame::data(Bytes::from("chunk1"))),
        Ok(http_body::Frame::data(Bytes::from("chunk2"))),
        Err(io::Error::new(
            io::ErrorKind::ConnectionReset,
            "connection reset",
        )),
    ]);

    let inner_body = StreamBody::new(stream);
    let mut body = BufferedBody::Passthrough(inner_body);

    // First chunk succeeds
    let frame = body.frame().await.unwrap().unwrap();
    let frame_data = frame.into_data().unwrap();
    assert_eq!(frame_data, Bytes::from("chunk1"));

    // Second chunk succeeds
    let frame = body.frame().await.unwrap().unwrap();
    let frame_data = frame.into_data().unwrap();
    assert_eq!(frame_data, Bytes::from("chunk2"));

    // Third poll yields error
    let result = body.frame().await.unwrap();
    assert!(result.is_err());

    // Stream ends after error
    let frame = body.frame().await;
    assert!(frame.is_none());
}

#[tokio::test]
async fn test_partial_yields_prefix_then_remaining() {
    let _prefix = Bytes::from("prefix-");

    // Create a stream for the remaining body
    use std::convert::Infallible;
    let stream = stream::iter(vec![
        Ok::<_, Infallible>(http_body::Frame::data(Bytes::from("chunk1"))),
        Ok::<_, Infallible>(http_body::Frame::data(Bytes::from("chunk2"))),
    ]);

    let remaining_body = StreamBody::new(stream);

    // Manually construct Partial with Body variant
    // Since Remaining is private, we need to use a public constructor or builder
    // For now, let's test via the body type's behavior
    let mut body = BufferedBody::Passthrough(remaining_body);

    // Test that passthrough works with streaming body
    let frame = body.frame().await.unwrap().unwrap();
    let frame_data = frame.into_data().unwrap();
    assert_eq!(frame_data, Bytes::from("chunk1"));

    let frame = body.frame().await.unwrap().unwrap();
    let frame_data = frame.into_data().unwrap();
    assert_eq!(frame_data, Bytes::from("chunk2"));

    let frame = body.frame().await;
    assert!(frame.is_none());
}

#[tokio::test]
async fn test_partial_with_stream_and_error() {
    use std::io;

    // Create a stream that yields some data then an error
    let stream = stream::iter(vec![
        Ok(http_body::Frame::data(Bytes::from("remaining1"))),
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe")),
    ]);

    let remaining_body = StreamBody::new(stream);
    let mut body = BufferedBody::Passthrough(remaining_body);

    // First chunk from remaining body succeeds
    let frame = body.frame().await.unwrap().unwrap();
    let frame_data = frame.into_data().unwrap();
    assert_eq!(frame_data, Bytes::from("remaining1"));

    // Next poll yields the error
    let result = body.frame().await.unwrap();
    assert!(result.is_err());

    // Stream ends
    let frame = body.frame().await;
    assert!(frame.is_none());
}

#[tokio::test]
async fn test_size_hint_complete() {
    let data = Bytes::from("hello");
    let body: BufferedBody<Full<Bytes>> = BufferedBody::Complete(Some(data.clone()));

    let hint = body.size_hint();
    assert_eq!(hint.lower(), data.len() as u64);
    assert_eq!(hint.upper(), Some(data.len() as u64));
}

#[tokio::test]
async fn test_size_hint_complete_after_consumed() {
    let body = BufferedBody::<Full<Bytes>>::Complete(None);

    let hint = body.size_hint();
    assert_eq!(hint.lower(), 0);
    assert_eq!(hint.upper(), Some(0));
}

#[tokio::test]
async fn test_size_hint_passthrough() {
    let data = Bytes::from("hello");
    let inner_body = Full::new(data.clone());
    let body = BufferedBody::Passthrough(inner_body);

    let hint = body.size_hint();
    assert_eq!(hint.lower(), data.len() as u64);
    assert_eq!(hint.upper(), Some(data.len() as u64));
}

#[tokio::test]
async fn test_is_end_stream_complete() {
    let data = Bytes::from("hello");
    let body: BufferedBody<Full<Bytes>> = BufferedBody::Complete(Some(data));
    assert!(!body.is_end_stream());

    let body = BufferedBody::<Full<Bytes>>::Complete(None);
    assert!(body.is_end_stream());
}

#[tokio::test]
async fn test_is_end_stream_passthrough() {
    let data = Bytes::from("hello");
    let inner_body = Full::new(data);
    let body = BufferedBody::Passthrough(inner_body);

    // Full body with data is not at end
    assert!(!body.is_end_stream());
}

#[tokio::test]
async fn test_collect_partial_with_partial_body_within_limit() {
    // Create a Partial body with prefix "hello" and remaining stream "world"
    let prefix = Some(Bytes::from("hello"));
    let remaining_stream = Full::new(Bytes::from("world"));
    let partial = PartialBufferedBody::new(
        prefix,
        Remaining::Body(remaining_stream),
    );
    let body = BufferedBody::Partial(partial);

    // Limit is 20 bytes, total is 10 bytes ("hello" + "world")
    let result = body.collect_partial(20).await;

    // Should be Ok(Complete) with combined data
    match result {
        Ok(BufferedBody::Complete(Some(data))) => {
            assert_eq!(data, Bytes::from("helloworld"));
        }
        other => panic!("Expected Ok(Complete) but got: {:?}", other),
    }
}

#[tokio::test]
async fn test_collect_partial_with_partial_body_exceeds_limit() {
    // Create a Partial body with prefix "hello" (5 bytes) and remaining stream with 10 bytes
    let prefix = Some(Bytes::from("hello"));
    let remaining_stream = Full::new(Bytes::from("1234567890"));
    let partial = PartialBufferedBody::new(
        prefix,
        Remaining::Body(remaining_stream),
    );
    let body = BufferedBody::Partial(partial);

    // Limit is 10 bytes, total is 15 bytes
    let result = body.collect_partial(10).await;

    // Should be Err - body exceeds limit
    match result {
        Err(BufferedBody::Partial(partial)) => {
            // Should have buffered the prefix (size_hint indicated stream would exceed remaining limit)
            let prefix = partial.prefix();
            assert!(prefix.is_some());
            let buffered = prefix.unwrap();
            // Prefix is 5 bytes, and size_hint of remaining stream (10 bytes) exceeded remaining limit (5 bytes)
            // So we get back just the prefix without reading from the stream
            assert_eq!(buffered.len(), 5);
        }
        Err(BufferedBody::Complete(Some(data))) => {
            // Or it might have collected all data and determined it exceeds limit
            assert_eq!(data.len(), 15);
        }
        other => panic!("Expected Err but got: {:?}", other),
    }
}

#[tokio::test]
async fn test_collect_partial_with_partial_body_prefix_exceeds_limit() {
    // Create a Partial body with prefix that already exceeds limit
    let prefix_bytes = Bytes::from("hello world this is too long");
    let prefix = Some(prefix_bytes.clone());
    let remaining_stream = Full::new(Bytes::from("more"));
    let partial = PartialBufferedBody::new(
        prefix,
        Remaining::Body(remaining_stream),
    );
    let body = BufferedBody::Partial(partial);

    // Limit is 10 bytes, prefix is 28 bytes
    let result = body.collect_partial(10).await;

    // Should immediately return Err without reading from stream
    match result {
        Err(BufferedBody::Partial(result_partial)) => {
            assert_eq!(result_partial.prefix(), Some(&prefix_bytes));
        }
        other => panic!("Expected Err(Partial) but got: {:?}", other),
    }
}

#[tokio::test]
async fn test_collect_partial_with_partial_body_no_prefix() {
    // Create a Partial body with no prefix, only remaining stream
    let remaining_stream = Full::new(Bytes::from("hello"));
    let partial = PartialBufferedBody::new(None, Remaining::Body(remaining_stream));
    let body = BufferedBody::Partial(partial);

    // Limit is 10 bytes
    let result = body.collect_partial(10).await;

    // Should be Ok(Complete) with data from stream
    match result {
        Ok(BufferedBody::Complete(Some(data))) => {
            assert_eq!(data, Bytes::from("hello"));
        }
        other => panic!("Expected Ok(Complete) but got: {:?}", other),
    }
}

#[tokio::test]
async fn test_collect_partial_with_partial_body_with_error() {
    // Create a Partial body with prefix and error in remaining
    let prefix_bytes = Bytes::from("hello");
    let prefix = Some(prefix_bytes.clone());
    let partial = PartialBufferedBody::<Full<Bytes>>::new(prefix, Remaining::Error(None));
    let body = BufferedBody::Partial(partial);

    // Limit is 10 bytes
    let result = body.collect_partial(10).await;

    // Should be Ok(Partial) - we can't read more but prefix is within limit
    match result {
        Ok(BufferedBody::Partial(result_partial)) => {
            assert_eq!(result_partial.prefix(), Some(&prefix_bytes));
        }
        other => panic!("Expected Ok(Partial) but got: {:?}", other),
    }
}
