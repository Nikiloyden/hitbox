//! Test that SerializableHttpResponse implements Cacheable trait properly

use bytes::Bytes;
use hitbox_core::Cacheable;
use hitbox_http::{BufferedBody, CacheableHttpResponse, SerializableHttpResponse};
use http::Response;
use http_body_util::Full;

// This test verifies that SerializableHttpResponse implements Cacheable
// The test compiles only if all required bounds are satisfied
#[test]
fn test_serializable_http_response_is_cacheable() {
    fn assert_cacheable<T: Cacheable>() {}

    // This will only compile if SerializableHttpResponse implements Cacheable
    assert_cacheable::<SerializableHttpResponse>();
}

#[cfg(feature = "rkyv_format")]
#[test]
fn test_rkyv_dynamic_serialization_trait() {
    use rkyv_dyn::SerializeDyn;

    // This test verifies that SerializableHttpResponse can be used as &dyn SerializeDyn
    // which is required by the Cacheable trait when rkyv_format is enabled
    fn assert_serialize_dyn<T>()
    where
        T: SerializeDyn,
    {
    }

    assert_serialize_dyn::<SerializableHttpResponse>();
}

#[tokio::test]
async fn test_cacheable_response_serialization_roundtrip() {
    use hitbox::CacheableResponse;
    use serde_json;

    // Create an HTTP response
    let body = Bytes::from(r#"{"message": "Hello, World!", "status": "success"}"#);
    let response = Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .header("cache-control", "public, max-age=3600")
        .header("x-custom-header", "test-value")
        .body(BufferedBody::<Full<Bytes>>::Complete(Some(body.clone())))
        .unwrap();

    // Create CacheableHttpResponse
    let cacheable = CacheableHttpResponse::from_response(response);

    // Convert to cached representation
    let cached = cacheable.into_cached().await;
    let serializable = match cached {
        hitbox::CachePolicy::Cacheable(data) => data,
        hitbox::CachePolicy::NonCacheable(_) => panic!("Expected cacheable response"),
    };

    // Serialize with serde_json
    let serialized = serde_json::to_vec(&serializable).expect("Failed to serialize");

    // Deserialize
    let deserialized: SerializableHttpResponse =
        serde_json::from_slice(&serialized).expect("Failed to deserialize");

    // Verify the data matches
    assert_eq!(
        serde_json::to_value(&serializable).unwrap(),
        serde_json::to_value(&deserialized).unwrap()
    );
}

#[cfg(feature = "rkyv_format")]
#[tokio::test]
async fn test_cacheable_response_rkyv_roundtrip() {
    use hitbox::CacheableResponse;
    use hitbox::RkyvDeserializer;
    use rkyv::Deserialize as RkyvDeserialize;

    // Create an HTTP response
    let body = Bytes::from(r#"{"message": "Hello, World!", "status": "success"}"#);
    let response = Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .header("cache-control", "public, max-age=3600")
        .header("x-custom-header", "test-value")
        .header("x-request-id", "12345")
        .body(BufferedBody::<Full<Bytes>>::Complete(Some(body.clone())))
        .unwrap();

    // Create CacheableHttpResponse
    let cacheable = CacheableHttpResponse::from_response(response);

    // Convert to cached representation
    let cached = cacheable.into_cached().await;
    let serializable = match cached {
        hitbox::CachePolicy::Cacheable(data) => data,
        hitbox::CachePolicy::NonCacheable(_) => panic!("Expected cacheable response"),
    };

    // Serialize with rkyv
    let serialized =
        rkyv::to_bytes::<_, 256>(&serializable).expect("Failed to serialize with rkyv");

    // Deserialize with validation
    let archived = rkyv::check_archived_root::<SerializableHttpResponse>(&serialized)
        .expect("Failed to validate archived data");

    let deserialized: SerializableHttpResponse =
        RkyvDeserialize::deserialize(archived, &mut RkyvDeserializer)
            .expect("Failed to deserialize with rkyv");

    // Verify the data matches using serde_json for comparison
    assert_eq!(
        serde_json::to_value(&serializable).unwrap(),
        serde_json::to_value(&deserialized).unwrap()
    );
}
