use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use hitbox_backend::format::{Format, JsonFormat};
use hitbox_backend::{
    Backend, BackendResult, CacheBackend, CacheKeyFormat, CompositionBackend, Compressor,
    DeleteStatus, PassthroughCompressor,
};
use hitbox_core::{BoxContext, CacheContext, CacheKey, CacheValue, CacheableResponse, EntityPolicyConfig, Predicate, Raw};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

#[cfg(feature = "rkyv_format")]
use rkyv::{Archive, Serialize as RkyvSerialize};
#[cfg(feature = "rkyv_format")]
use rkyv_typename::TypeName;

// Simple in-memory backend for testing
#[derive(Clone, Debug)]
struct TestBackend {
    store: Arc<Mutex<HashMap<CacheKey, CacheValue<Raw>>>>,
}

impl TestBackend {
    fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl Backend for TestBackend {
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        Ok(self.store.lock().unwrap().get(key).cloned())
    }

    async fn write(
        &self,
        key: &CacheKey,
        value: CacheValue<Raw>,
        _ttl: Option<Duration>,
    ) -> BackendResult<()> {
        self.store.lock().unwrap().insert(key.clone(), value);
        Ok(())
    }

    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        match self.store.lock().unwrap().remove(key) {
            Some(_) => Ok(DeleteStatus::Deleted(1)),
            None => Ok(DeleteStatus::Missing),
        }
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

impl CacheBackend for TestBackend {}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(
    feature = "rkyv_format",
    derive(Archive, RkyvSerialize, rkyv::Deserialize, TypeName)
)]
#[cfg_attr(feature = "rkyv_format", archive(check_bytes))]
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
async fn test_boxed_composition_backend() {
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let composition = CompositionBackend::new(l1, l2);

    // Box the CompositionBackend itself
    let boxed: Box<CompositionBackend<_, _>> = Box::new(composition);

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Should work through Box
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    boxed
        .set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
        .await
        .unwrap();

    let result = boxed.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.data, "test_value");
}

#[tokio::test]
async fn test_arc_composition_backend() {
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let composition = CompositionBackend::new(l1, l2);

    // Arc the CompositionBackend itself
    let arc: Arc<CompositionBackend<_, _>> = Arc::new(composition);

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Should work through Arc
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    arc.set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
        .await
        .unwrap();

    let result = arc.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.data, "test_value");

    // Arc should be cloneable
    let arc2 = arc.clone();
    let result2 = arc2.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result2.is_some());
}

#[tokio::test]
async fn test_ref_composition_backend() {
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let composition = CompositionBackend::new(l1, l2);

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Should work through reference
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    composition
        .set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
        .await
        .unwrap();

    let result = composition.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.data, "test_value");
}

#[tokio::test]
async fn test_composition_as_dyn_backend() {
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let composition = CompositionBackend::new(l1, l2);

    // Use CompositionBackend as trait object
    let backend: &dyn Backend = &composition;

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Should work through trait object
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    backend
        .set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
        .await
        .unwrap();

    let result = backend.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.data, "test_value");
}

#[tokio::test]
async fn test_boxed_composition_as_dyn_backend() {
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let composition = CompositionBackend::new(l1, l2);

    // Box CompositionBackend and use as trait object
    let backend: Box<dyn Backend> = Box::new(composition);

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Should work through boxed trait object
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    backend
        .set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
        .await
        .unwrap();

    let result = backend.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.data, "test_value");
}

#[tokio::test]
async fn test_arc_composition_as_dyn_backend() {
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let composition = CompositionBackend::new(l1, l2);

    // Arc CompositionBackend and use as trait object
    let backend: Arc<dyn Backend + Send + 'static> = Arc::new(composition);

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Should work through Arc'd trait object
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    backend
        .set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
        .await
        .unwrap();

    let result = backend.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.data, "test_value");

    // Arc trait object should be cloneable
    let backend2 = backend.clone();
    let result2 = backend2.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result2.is_some());
}
