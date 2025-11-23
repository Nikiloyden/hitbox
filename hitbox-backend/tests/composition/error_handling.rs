use async_trait::async_trait;
use chrono::Utc;
use hitbox_backend::{
    Backend, BackendError, BackendResult, CacheBackend, CacheKeyFormat, Compressor,
    CompositionBackend, DeleteStatus, PassthroughCompressor,
};
use hitbox_backend::format::{Format, JsonFormat};
use hitbox_core::{CacheKey, CacheValue, CacheableResponse, EntityPolicyConfig, Predicate, Raw};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[cfg(feature = "rkyv_format")]
use rkyv::{Archive, Serialize as RkyvSerialize};
#[cfg(feature = "rkyv_format")]
use rkyv_typename::TypeName;

// Failing backend for testing error propagation
#[derive(Clone, Debug)]
struct FailingBackend {
    error_message: String,
}

impl FailingBackend {
    fn new(error_message: &str) -> Self {
        Self {
            error_message: error_message.to_string(),
        }
    }
}

#[async_trait]
impl Backend for FailingBackend {
    async fn read(&self, _key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        Err(BackendError::InternalError(Box::new(
            std::io::Error::other(self.error_message.clone()),
        )))
    }

    async fn write(
        &self,
        _key: &CacheKey,
        _value: CacheValue<Raw>,
        _ttl: Option<Duration>,
    ) -> BackendResult<()> {
        Err(BackendError::InternalError(Box::new(
            std::io::Error::other(self.error_message.clone()),
        )))
    }

    async fn remove(&self, _key: &CacheKey) -> BackendResult<DeleteStatus> {
        Err(BackendError::InternalError(Box::new(
            std::io::Error::other(self.error_message.clone()),
        )))
    }

    fn value_format(&self) -> &dyn Format {
        &JsonFormat
    }

    fn key_format(&self) -> &CacheKeyFormat {
        &CacheKeyFormat::Bitcode
    }

    fn compressor(&self) -> &dyn Compressor {
        &PassthroughCompressor
    }
}

impl CacheBackend for FailingBackend {}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "rkyv_format", derive(Archive, RkyvSerialize, rkyv::Deserialize, TypeName))]
#[cfg_attr(feature = "rkyv_format", archive_attr(derive(TypeName)))]
struct TestValue {
    data: String,
}

#[async_trait]
impl CacheableResponse for TestValue {
    type Cached = Self;
    type Subject = Self;

    async fn cache_policy<P: Predicate<Subject = Self::Subject> + Send + Sync>(
        self,
        _predicate: P,
        _config: &EntityPolicyConfig,
    ) -> hitbox_core::ResponseCachePolicy<Self> {
        unimplemented!()
    }

    async fn into_cached(self) -> hitbox_core::CachePolicy<Self::Cached, Self> {
        unimplemented!()
    }

    async fn from_cached(_cached: Self::Cached) -> Self {
        unimplemented!()
    }
}

#[tokio::test]
async fn test_both_layers_fail_set() {
    let l1 = FailingBackend::new("L1 connection timeout");
    let l2 = FailingBackend::new("L2 authentication failed");
    let composition = CompositionBackend::new(l1, l2);

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // When both layers fail, should return CompositionError with both errors
    let result = composition
        .set::<TestValue>(&key, &value, Some(Duration::from_secs(60)))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();

    // Verify it's an InternalError containing CompositionError
    match err {
        BackendError::InternalError(boxed_err) => {
            let err_str = boxed_err.to_string();
            // Should contain both error messages
            assert!(
                err_str.contains("L1 connection timeout"),
                "Error should contain L1 error: {}",
                err_str
            );
            assert!(
                err_str.contains("L2 authentication failed"),
                "Error should contain L2 error: {}",
                err_str
            );
            assert!(
                err_str.contains("Both cache layers failed"),
                "Error should mention both layers failed: {}",
                err_str
            );
        }
        _ => panic!("Expected InternalError, got: {:?}", err),
    }
}

#[tokio::test]
async fn test_both_layers_fail_delete() {
    let l1 = FailingBackend::new("L1 disk full");
    let l2 = FailingBackend::new("L2 network unreachable");
    let composition = CompositionBackend::new(l1, l2);

    let key = CacheKey::from_str("test", "key1");

    // When both layers fail, should return CompositionError with both errors
    let result = composition.delete(&key).await;

    assert!(result.is_err());
    let err = result.unwrap_err();

    // Verify it contains both errors
    match err {
        BackendError::InternalError(boxed_err) => {
            let err_str = boxed_err.to_string();
            assert!(
                err_str.contains("L1 disk full"),
                "Error should contain L1 error: {}",
                err_str
            );
            assert!(
                err_str.contains("L2 network unreachable"),
                "Error should contain L2 error: {}",
                err_str
            );
            assert!(
                err_str.contains("Both cache layers failed"),
                "Error should mention both layers failed: {}",
                err_str
            );
        }
        _ => panic!("Expected InternalError, got: {:?}", err),
    }
}

#[tokio::test]
async fn test_both_layers_fail_backend_write() {
    let l1 = FailingBackend::new("L1 quota exceeded");
    let l2 = FailingBackend::new("L2 permission denied");
    let composition = CompositionBackend::new(l1, l2);

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test data".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Test via CacheBackend::set (which uses Backend::write internally)
    let result = composition
        .set::<TestValue>(&key, &value, Some(Duration::from_secs(60)))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();

    // Verify it contains both errors
    match err {
        BackendError::InternalError(boxed_err) => {
            let err_str = boxed_err.to_string();
            assert!(
                err_str.contains("L1 quota exceeded"),
                "Error should contain L1 error: {}",
                err_str
            );
            assert!(
                err_str.contains("L2 permission denied"),
                "Error should contain L2 error: {}",
                err_str
            );
        }
        _ => panic!("Expected InternalError, got: {:?}", err),
    }
}

#[tokio::test]
async fn test_both_layers_fail_backend_remove() {
    let l1 = FailingBackend::new("L1 service unavailable");
    let l2 = FailingBackend::new("L2 read-only mode");
    let composition = CompositionBackend::new(l1, l2);

    let key = CacheKey::from_str("test", "key1");

    // Test via Backend trait (lower level)
    let result = composition.remove(&key).await;

    assert!(result.is_err());
    let err = result.unwrap_err();

    // Verify it contains both errors
    match err {
        BackendError::InternalError(boxed_err) => {
            let err_str = boxed_err.to_string();
            assert!(
                err_str.contains("L1 service unavailable"),
                "Error should contain L1 error: {}",
                err_str
            );
            assert!(
                err_str.contains("L2 read-only mode"),
                "Error should contain L2 error: {}",
                err_str
            );
        }
        _ => panic!("Expected InternalError, got: {:?}", err),
    }
}
