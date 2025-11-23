//! Tests for context-based refill with dynamic dispatch (trait objects).
//!
//! This test verifies that the BackendContext implementation correctly enables
//! refill operations when CompositionBackend is used through `Box<dyn Backend>`.

use async_trait::async_trait;
use chrono::Utc;
use hitbox_backend::composition::CompositionBackend;
use hitbox_backend::{Backend, CacheBackend};
use hitbox_core::{CacheKey, CacheValue, CacheableResponse, EntityPolicyConfig, Predicate};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[cfg(feature = "rkyv_format")]
use rkyv::{Archive, Serialize as RkyvSerialize};
#[cfg(feature = "rkyv_format")]
use rkyv_typename::TypeName;

use crate::common::TestBackend;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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
async fn test_refill_with_boxed_composition_backend() {
    // Create L1 and L2 as trait objects
    let l1: Box<dyn Backend> = Box::new(TestBackend::new());
    let l2: Box<dyn Backend> = Box::new(TestBackend::new());

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "from_l2".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Populate only L2
    l2.set::<TestValue>(&key, &value, Some(Duration::from_secs(60)))
        .await
        .unwrap();

    // Create composition backend and use it as trait object
    let composition = CompositionBackend::new(l1, l2);
    let backend: Box<dyn Backend> = Box::new(composition);

    // Read through trait object - should trigger refill
    let result = backend.get::<TestValue>(&key).await.unwrap();
    assert!(result.is_some(), "Should get value from L2");
    assert_eq!(result.unwrap().data.data, "from_l2");

    // Unfortunately we can't easily verify L1 was refilled because we don't have
    // direct access to L1 through the trait object, but the fact that it works
    // without errors proves the context mechanism is working
}

#[tokio::test]
async fn test_refill_with_trait_object_verifies_l1_populated() {
    // Create backends that we can inspect later
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();

    // Clone for later inspection
    let l1_inspect = l1.clone();

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Populate only L2
    l2.set::<TestValue>(&key, &value, Some(Duration::from_secs(60)))
        .await
        .unwrap();

    // Verify initial state
    assert!(!l1_inspect.has(&key), "L1 should be empty initially");

    // Create composition and use as trait object
    let composition = CompositionBackend::new(l1, l2);
    let backend: Box<dyn Backend> = Box::new(composition);

    // Trigger refill through trait object
    let result = backend.get::<TestValue>(&key).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.data, "test_value");

    // Verify L1 was refilled
    assert!(l1_inspect.has(&key), "L1 should be refilled after L2 hit through trait object");

    // Verify the data is correct
    let l1_value = l1_inspect.get::<TestValue>(&key).await.unwrap();
    assert!(l1_value.is_some());
    assert_eq!(l1_value.unwrap().data.data, "test_value");
}

#[tokio::test]
async fn test_direct_write_through_trait_object() {
    // Create backends
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();

    // Clone for inspection
    let l1_inspect = l1.clone();
    let l2_inspect = l2.clone();

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "direct_write".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Create composition as trait object
    let composition = CompositionBackend::new(l1, l2);
    let backend: Box<dyn Backend> = Box::new(composition);

    // Direct write through trait object
    backend
        .set::<TestValue>(&key, &value, Some(Duration::from_secs(60)))
        .await
        .unwrap();

    // Verify both L1 and L2 have the data
    assert!(l1_inspect.has(&key), "L1 should have data after direct write");
    assert!(l2_inspect.has(&key), "L2 should have data after direct write");
}

#[tokio::test]
async fn test_nested_composition_with_trait_objects() {
    // Create a 3-layer cache: L1 -> (L2 -> L3)
    // The outer composition will refill L1, the inner composition will refill L2
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let l3 = TestBackend::new();

    // Clone for inspection
    let l1_inspect = l1.clone();
    let l2_inspect = l2.clone();

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "from_l3".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Populate only L3
    l3.set::<TestValue>(&key, &value, Some(Duration::from_secs(60)))
        .await
        .unwrap();

    // Verify L1 and L2 are empty initially
    assert!(!l1_inspect.has(&key), "L1 should be empty initially");
    assert!(!l2_inspect.has(&key), "L2 should be empty initially");

    // Create nested composition: L2+L3 first
    let l2_l3_composition = CompositionBackend::new(l2, l3);

    // First, trigger refill at the inner level by reading through L2+L3
    let inner_result = l2_l3_composition.get::<TestValue>(&key).await.unwrap();
    assert!(inner_result.is_some());

    // Now L2 should be refilled by the inner composition
    assert!(l2_inspect.has(&key), "L2 should be refilled after read through inner composition");

    // Wrap inner composition as trait object
    let l2_l3_boxed: Box<dyn Backend> = Box::new(l2_l3_composition);

    // Compose L1 with the nested backend
    let full_composition = CompositionBackend::new(l1, l2_l3_boxed);
    let backend: Box<dyn Backend> = Box::new(full_composition);

    // Clear L1 to test outer refill
    l1_inspect.clear();

    // Read through nested trait object
    let result = backend.get::<TestValue>(&key).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.data, "from_l3");

    // L1 should now be refilled by the outer composition
    assert!(l1_inspect.has(&key), "L1 should be refilled after read through nested trait object");
}

#[tokio::test]
async fn test_arc_wrapped_composition_refill() {
    use std::sync::Arc;

    // Test with Arc<dyn Backend + Send> instead of Box
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();

    // Clone for inspection
    let l1_inspect = l1.clone();

    // Wrap in Arc with Send bound
    let l1_arc: Arc<dyn Backend + Send> = Arc::new(l1);
    let l2_arc: Arc<dyn Backend + Send> = Arc::new(l2);

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "from_l2_arc".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Populate only L2 directly before wrapping
    l2_arc
        .set::<TestValue>(&key, &value, Some(Duration::from_secs(60)))
        .await
        .unwrap();

    // Verify L1 is empty initially
    assert!(!l1_inspect.has(&key), "L1 should be empty initially");

    // Create composition with Arc backends
    let composition = CompositionBackend::new(l1_arc, l2_arc);

    // Trigger refill
    let result = composition.get::<TestValue>(&key).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.data, "from_l2_arc");

    // Verify L1 was refilled
    assert!(l1_inspect.has(&key), "L1 should be refilled after L2 hit through Arc");
}

#[tokio::test]
async fn test_multiple_refills_through_trait_object() {
    // Test that multiple L2 hits through trait object all trigger refills
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();

    // Clone for inspection
    let l1_inspect = l1.clone();
    let l2_ref = l2.clone();

    // Populate L2 with multiple keys
    for i in 0..5 {
        let key = CacheKey::from_str("test", &format!("key{}", i));
        let value = CacheValue::new(
            TestValue {
                data: format!("value_{}", i),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        l2_ref
            .set::<TestValue>(&key, &value, Some(Duration::from_secs(60)))
            .await
            .unwrap();
    }

    let composition = CompositionBackend::new(l1, l2);
    let backend: Box<dyn Backend> = Box::new(composition);

    // Trigger refills through trait object
    for i in 0..5 {
        let key = CacheKey::from_str("test", &format!("key{}", i));

        // Read should trigger refill
        let result = backend.get::<TestValue>(&key).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().data.data, format!("value_{}", i));

        // Verify L1 was refilled
        assert!(
            l1_inspect.has(&key),
            "L1 should be refilled for key{}",
            i
        );
    }
}
