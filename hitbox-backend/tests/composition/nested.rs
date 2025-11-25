//! Tests for nested CompositionBackend (composition of compositions).
//!
//! This tests that CompositionBackend can be composed with other CompositionBackends,
//! creating multi-level cache hierarchies.

use async_trait::async_trait;
use chrono::Utc;
use hitbox_backend::{Backend, CacheBackend, CompositionBackend};
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
#[cfg_attr(feature = "rkyv_format", archive(check_bytes))]
#[cfg_attr(feature = "rkyv_format", archive_attr(derive(TypeName)))]
pub(super) struct TestValue {
    pub data: String,
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

// =============================================================================
// Static Dispatch Tests (Concrete Types)
// =============================================================================

#[tokio::test]
async fn test_nested_composition_static_dispatch() {
    // Create a 3-level cache hierarchy:
    // Level 1 (fastest): L1
    // Level 2 (medium):  L2
    // Level 3 (slowest): L3
    //
    // Structure: CompositionBackend(L1, CompositionBackend(L2, L3))

    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let l3 = TestBackend::new();

    // Create inner composition: L2 + L3
    let l2_l3 = CompositionBackend::new(l2.clone(), l3.clone());

    // Create outer composition: L1 + (L2 + L3)
    let cache = CompositionBackend::new(l1.clone(), l2_l3);

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Write through nested composition - should populate all 3 levels
    cache.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await.unwrap();

    // Verify all 3 levels have the data
    assert!(l1.has(&key), "L1 should have the value");
    assert!(l2.has(&key), "L2 should have the value");
    assert!(l3.has(&key), "L3 should have the value");

    // Read should return the value (from L1)
    let result = cache.get::<TestValue>(&key).await.unwrap();
    assert!(result.is_some());
    assert_eq!(
        result.unwrap().data,
        TestValue {
            data: "test_value".to_string()
        }
    );
}

#[tokio::test]
async fn test_nested_composition_static_l1_miss() {
    // Test that on L1 miss, it checks the nested composition (L2+L3)

    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let l3 = TestBackend::new();

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "from_l3".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Populate only L3
    l3.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await.unwrap();

    // Create nested composition
    let l2_l3 = CompositionBackend::new(l2.clone(), l3.clone());
    let cache = CompositionBackend::new(l1.clone(), l2_l3);

    // Read should miss L1, miss L2, hit L3
    let result = cache.get::<TestValue>(&key).await.unwrap();
    assert!(result.is_some());
    assert_eq!(
        result.unwrap().data,
        TestValue {
            data: "from_l3".to_string()
        }
    );

    // L1 should now be populated (refill from nested composition)
    assert!(l1.has(&key), "L1 should be refilled from nested composition");
    // L2 should also be populated (refill from L3)
    assert!(l2.has(&key), "L2 should be refilled from L3");
}

#[tokio::test]
async fn test_nested_composition_static_4_levels() {
    // Test 4-level hierarchy: L1 + (L2 + (L3 + L4))

    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let l3 = TestBackend::new();
    let l4 = TestBackend::new();

    let key = CacheKey::from_str("test", "deep_key");
    let value = CacheValue::new(
        TestValue {
            data: "from_l4".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Populate only L4 (deepest level)
    l4.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await.unwrap();

    // Build nested hierarchy
    let l3_l4 = CompositionBackend::new(l3.clone(), l4.clone());
    let l2_l3_l4 = CompositionBackend::new(l2.clone(), l3_l4);
    let cache = CompositionBackend::new(l1.clone(), l2_l3_l4);

    // Read should cascade through all 4 levels
    let result = cache.get::<TestValue>(&key).await.unwrap();
    assert!(result.is_some());
    assert_eq!(
        result.unwrap().data,
        TestValue {
            data: "from_l4".to_string()
        }
    );

    // All levels should now have the data (refill cascade)
    assert!(l1.has(&key), "L1 should be refilled");
    assert!(l2.has(&key), "L2 should be refilled");
    assert!(l3.has(&key), "L3 should be refilled");
    assert!(l4.has(&key), "L4 should have original data");
}

// =============================================================================
// Dynamic Dispatch Tests (Trait Objects)
// =============================================================================

#[tokio::test]
async fn test_nested_composition_dynamic_dispatch() {
    // Test nested composition with Box<dyn Backend>

    let l1: Box<dyn Backend> = Box::new(TestBackend::new());
    let l2: Box<dyn Backend> = Box::new(TestBackend::new());
    let l3: Box<dyn Backend> = Box::new(TestBackend::new());

    // Create inner composition as trait object
    let l2_l3: Box<dyn Backend> = Box::new(CompositionBackend::new(l2, l3));

    // Create outer composition with trait object
    let cache = CompositionBackend::new(l1, l2_l3);

    let key = CacheKey::from_str("test", "dyn_key");
    let value = CacheValue::new(
        TestValue {
            data: "dynamic_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Write and read through dynamic dispatch
    cache.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await.unwrap();

    let result = cache.get::<TestValue>(&key).await.unwrap();
    assert!(result.is_some());
    assert_eq!(
        result.unwrap().data,
        TestValue {
            data: "dynamic_value".to_string()
        }
    );
}

#[tokio::test]
async fn test_nested_composition_dynamic_as_trait_object() {
    // Test that the nested composition itself can be used as a trait object

    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let l3 = TestBackend::new();

    let key = CacheKey::from_str("test", "nested_trait");
    let value = CacheValue::new(
        TestValue {
            data: "trait_object_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Create nested composition
    let l2_l3 = CompositionBackend::new(l2, l3);
    let nested = CompositionBackend::new(l1, l2_l3);

    // Use the entire nested composition as a trait object
    let backend: Box<dyn Backend> = Box::new(nested);

    // Operations through trait object
    backend.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await.unwrap();

    let result = backend.get::<TestValue>(&key).await.unwrap();
    assert!(result.is_some());
    assert_eq!(
        result.unwrap().data,
        TestValue {
            data: "trait_object_value".to_string()
        }
    );
}

#[tokio::test]
async fn test_nested_composition_delete_cascades() {
    // Test that delete operations cascade through nested compositions

    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let l3 = TestBackend::new();

    let key = CacheKey::from_str("test", "delete_key");
    let value = CacheValue::new(
        TestValue {
            data: "to_delete".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Populate all levels
    l1.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await.unwrap();
    l2.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await.unwrap();
    l3.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await.unwrap();

    // Verify all have the data
    assert!(l1.has(&key));
    assert!(l2.has(&key));
    assert!(l3.has(&key));

    // Create nested composition and delete
    let l2_l3 = CompositionBackend::new(l2.clone(), l3.clone());
    let cache = CompositionBackend::new(l1.clone(), l2_l3);

    cache.delete(&key).await.unwrap();

    // Verify all levels no longer have the data
    assert!(!l1.has(&key), "L1 should be deleted");
    assert!(!l2.has(&key), "L2 should be deleted");
    assert!(!l3.has(&key), "L3 should be deleted");
}
