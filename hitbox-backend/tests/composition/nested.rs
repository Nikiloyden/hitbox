//! Tests for nested CompositionBackend (composition of compositions).
//!
//! This tests that CompositionBackend can be composed with other CompositionBackends,
//! creating multi-level cache hierarchies.

use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;

use hitbox_backend::{CacheBackend, CompositionBackend, SyncBackend};
use hitbox_core::{
    BoxContext, CacheContext, CacheKey, CacheValue, CacheableResponse, EntityPolicyConfig, Offload,
    Predicate,
};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::future::Future;

#[cfg(feature = "rkyv_format")]
use rkyv::{Archive, Serialize as RkyvSerialize};
#[cfg(feature = "rkyv_format")]
use rkyv_typename::TypeName;

use crate::common::TestBackend;

/// Test offload that spawns tasks with tokio::spawn
#[derive(Clone, Debug)]
struct TestOffload;

impl Offload for TestOffload {
    fn spawn<F>(&self, _kind: impl Into<SmolStr>, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        tokio::spawn(future);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "rkyv_format",
    derive(Archive, RkyvSerialize, rkyv::Deserialize, TypeName)
)]
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
    let l2_l3 = CompositionBackend::new(l2.clone(), l3.clone(), TestOffload);

    // Create outer composition: L1 + (L2 + L3)
    let cache = CompositionBackend::new(l1.clone(), l2_l3, TestOffload);

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Write through nested composition - should populate all 3 levels
    let mut ctx: BoxContext = CacheContext::default().boxed();
    cache
        .set::<TestValue>(&key, &value, &mut ctx)
        .await
        .unwrap();

    // Verify all 3 levels have the data
    assert!(l1.has(&key), "L1 should have the value");
    assert!(l2.has(&key), "L2 should have the value");
    assert!(l3.has(&key), "L3 should have the value");

    // Read should return the value (from L1)
    let mut ctx: BoxContext = CacheContext::default().boxed();
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
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
    let mut ctx: BoxContext = CacheContext::default().boxed();
    l3.set::<TestValue>(&key, &value, &mut ctx).await.unwrap();

    // Create nested composition
    let l2_l3 = CompositionBackend::new(l2.clone(), l3.clone(), TestOffload);
    let cache = CompositionBackend::new(l1.clone(), l2_l3, TestOffload);

    // Read should miss L1, miss L2, hit L3
    let mut ctx: BoxContext = CacheContext::default().boxed();
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(
        result.unwrap().data,
        TestValue {
            data: "from_l3".to_string()
        }
    );

    // L1 should now be populated (refill from nested composition)
    assert!(
        l1.has(&key),
        "L1 should be refilled from nested composition"
    );
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
    let mut ctx: BoxContext = CacheContext::default().boxed();
    l4.set::<TestValue>(&key, &value, &mut ctx).await.unwrap();

    // Build nested hierarchy
    let l3_l4 = CompositionBackend::new(l3.clone(), l4.clone(), TestOffload);
    let l2_l3_l4 = CompositionBackend::new(l2.clone(), l3_l4, TestOffload);
    let cache = CompositionBackend::new(l1.clone(), l2_l3_l4, TestOffload);

    // Read should cascade through all 4 levels
    let mut ctx: BoxContext = CacheContext::default().boxed();
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
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
    // Test nested composition with Arc<SyncBackend>

    let l1: Arc<SyncBackend> = Arc::new(TestBackend::new());
    let l2: Arc<SyncBackend> = Arc::new(TestBackend::new());
    let l3: Arc<SyncBackend> = Arc::new(TestBackend::new());

    // Create inner composition as trait object
    let l2_l3: Arc<SyncBackend> = Arc::new(CompositionBackend::new(l2, l3, TestOffload));

    // Create outer composition with trait object
    let cache = CompositionBackend::new(l1, l2_l3, TestOffload);

    let key = CacheKey::from_str("test", "dyn_key");
    let value = CacheValue::new(
        TestValue {
            data: "dynamic_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Write and read through dynamic dispatch
    let mut ctx: BoxContext = CacheContext::default().boxed();
    cache
        .set::<TestValue>(&key, &value, &mut ctx)
        .await
        .unwrap();

    let mut ctx: BoxContext = CacheContext::default().boxed();
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
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
    let l2_l3 = CompositionBackend::new(l2, l3, TestOffload);
    let nested = CompositionBackend::new(l1, l2_l3, TestOffload);

    // Use the entire nested composition as a trait object
    let backend: Arc<SyncBackend> = Arc::new(nested);

    // Operations through trait object
    let mut ctx: BoxContext = CacheContext::default().boxed();
    backend
        .set::<TestValue>(&key, &value, &mut ctx)
        .await
        .unwrap();

    let mut ctx: BoxContext = CacheContext::default().boxed();
    let result = backend.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(
        result.unwrap().data,
        TestValue {
            data: "trait_object_value".to_string()
        }
    );
}

// =============================================================================
// TTL/Stale Metadata Tests - Shared Test Logic
// =============================================================================

/// Helper struct to hold backends for TTL/stale testing with inspection capability.
struct TtlTestBackends {
    l1: TestBackend,
    l2: TestBackend,
    l3: Option<TestBackend>,
}

impl TtlTestBackends {
    fn two_level() -> Self {
        Self {
            l1: TestBackend::new(),
            l2: TestBackend::new(),
            l3: None,
        }
    }

    fn three_level() -> Self {
        Self {
            l1: TestBackend::new(),
            l2: TestBackend::new(),
            l3: Some(TestBackend::new()),
        }
    }
}

/// Test TTL preservation through write/read cycle.
async fn run_ttl_preserved_test<B: CacheBackend + Send + Sync>(
    cache: B,
    l1: &TestBackend,
    l2: &TestBackend,
) {
    let key = CacheKey::from_str("test", "ttl_key");
    let expire_time = Utc::now() + chrono::Duration::seconds(300);
    let value = CacheValue::new(
        TestValue {
            data: "ttl_test".to_string(),
        },
        Some(expire_time),
        None,
    );

    // Write through composition
    let mut ctx: BoxContext = CacheContext::default().boxed();
    cache
        .set::<TestValue>(&key, &value, &mut ctx)
        .await
        .unwrap();

    // Verify TTL is preserved in L1
    let l1_raw = l1.get_raw(&key).expect("L1 should have the value");
    assert!(l1_raw.expire.is_some(), "L1 should have expire time");
    assert_eq!(
        l1_raw.expire,
        Some(expire_time),
        "L1 expire time should match"
    );

    // Verify TTL is preserved in L2
    let l2_raw = l2.get_raw(&key).expect("L2 should have the value");
    assert!(l2_raw.expire.is_some(), "L2 should have expire time");
    assert_eq!(
        l2_raw.expire,
        Some(expire_time),
        "L2 expire time should match"
    );

    // Read back and verify TTL is preserved
    let mut ctx: BoxContext = CacheContext::default().boxed();
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    let cache_value = result.unwrap();
    assert_eq!(
        cache_value.expire,
        Some(expire_time),
        "Read expire time should match"
    );
}

/// Test stale preservation through write/read cycle.
async fn run_stale_preserved_test<B: CacheBackend + Send + Sync>(
    cache: B,
    l1: &TestBackend,
    l2: &TestBackend,
) {
    let key = CacheKey::from_str("test", "stale_key");
    let expire_time = Utc::now() + chrono::Duration::seconds(300);
    let stale_time = Utc::now() + chrono::Duration::seconds(60);
    let value = CacheValue::new(
        TestValue {
            data: "stale_test".to_string(),
        },
        Some(expire_time),
        Some(stale_time),
    );

    // Write through composition
    let mut ctx: BoxContext = CacheContext::default().boxed();
    cache
        .set::<TestValue>(&key, &value, &mut ctx)
        .await
        .unwrap();

    // Verify stale is preserved in L1
    let l1_raw = l1.get_raw(&key).expect("L1 should have the value");
    assert!(l1_raw.stale.is_some(), "L1 should have stale time");
    assert_eq!(l1_raw.stale, Some(stale_time), "L1 stale time should match");

    // Verify stale is preserved in L2
    let l2_raw = l2.get_raw(&key).expect("L2 should have the value");
    assert!(l2_raw.stale.is_some(), "L2 should have stale time");
    assert_eq!(l2_raw.stale, Some(stale_time), "L2 stale time should match");

    // Read back and verify stale is preserved
    let mut ctx: BoxContext = CacheContext::default().boxed();
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    let cache_value = result.unwrap();
    assert_eq!(
        cache_value.stale,
        Some(stale_time),
        "Read stale time should match"
    );
    assert_eq!(
        cache_value.expire,
        Some(expire_time),
        "Read expire time should match"
    );
}

/// Test TTL/stale preservation during refill (L2 -> L1).
async fn run_refill_ttl_stale_test<B: CacheBackend + Send + Sync>(
    cache: B,
    l1: &TestBackend,
    l2: &TestBackend,
) {
    let key = CacheKey::from_str("test", "refill_ttl_key");
    let expire_time = Utc::now() + chrono::Duration::seconds(300);
    let stale_time = Utc::now() + chrono::Duration::seconds(60);
    let value = CacheValue::new(
        TestValue {
            data: "refill_ttl_test".to_string(),
        },
        Some(expire_time),
        Some(stale_time),
    );

    // Populate only L2 (not L1)
    let mut ctx: BoxContext = CacheContext::default().boxed();
    l2.set::<TestValue>(&key, &value, &mut ctx).await.unwrap();

    // Verify L1 is empty
    assert!(!l1.has(&key), "L1 should be empty initially");

    // Read through composition (should trigger refill)
    let mut ctx: BoxContext = CacheContext::default().boxed();
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());

    // Verify L1 was refilled with correct TTL/stale
    let l1_raw = l1.get_raw(&key).expect("L1 should be refilled");
    assert_eq!(
        l1_raw.expire,
        Some(expire_time),
        "L1 refilled expire time should match"
    );
    assert_eq!(
        l1_raw.stale,
        Some(stale_time),
        "L1 refilled stale time should match"
    );
}

/// Test no TTL/stale preservation.
async fn run_no_ttl_no_stale_test<B: CacheBackend + Send + Sync>(cache: B, l1: &TestBackend) {
    let key = CacheKey::from_str("test", "no_ttl_key");
    let value = CacheValue::new(
        TestValue {
            data: "no_ttl_test".to_string(),
        },
        None,
        None,
    );

    // Write through composition
    let mut ctx: BoxContext = CacheContext::default().boxed();
    cache
        .set::<TestValue>(&key, &value, &mut ctx)
        .await
        .unwrap();

    // Verify no TTL/stale in L1
    let l1_raw = l1.get_raw(&key).expect("L1 should have the value");
    assert!(l1_raw.expire.is_none(), "L1 should have no expire time");
    assert!(l1_raw.stale.is_none(), "L1 should have no stale time");

    // Read back and verify
    let mut ctx: BoxContext = CacheContext::default().boxed();
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    let cache_value = result.unwrap();
    assert!(
        cache_value.expire.is_none(),
        "Read should have no expire time"
    );
    assert!(
        cache_value.stale.is_none(),
        "Read should have no stale time"
    );
}

/// Test TTL/stale preservation in nested refill (L3 -> L2 -> L1).
async fn run_nested_refill_ttl_stale_test<B: CacheBackend + Send + Sync>(
    cache: B,
    l1: &TestBackend,
    l2: &TestBackend,
    l3: &TestBackend,
) {
    let key = CacheKey::from_str("test", "nested_ttl_key");
    let expire_time = Utc::now() + chrono::Duration::seconds(600);
    let stale_time = Utc::now() + chrono::Duration::seconds(120);
    let value = CacheValue::new(
        TestValue {
            data: "nested_ttl_test".to_string(),
        },
        Some(expire_time),
        Some(stale_time),
    );

    // Populate only L3 (deepest level)
    let mut ctx: BoxContext = CacheContext::default().boxed();
    l3.set::<TestValue>(&key, &value, &mut ctx).await.unwrap();

    // Verify L1 and L2 are empty
    assert!(!l1.has(&key), "L1 should be empty initially");
    assert!(!l2.has(&key), "L2 should be empty initially");

    // Read through composition (should trigger cascade refill)
    let mut ctx: BoxContext = CacheContext::default().boxed();
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    let cache_value = result.unwrap();

    // Verify returned value has correct TTL/stale
    assert_eq!(
        cache_value.expire,
        Some(expire_time),
        "Returned expire time should match"
    );
    assert_eq!(
        cache_value.stale,
        Some(stale_time),
        "Returned stale time should match"
    );

    // Verify L1 was refilled with correct TTL/stale
    let l1_raw = l1.get_raw(&key).expect("L1 should be refilled");
    assert_eq!(
        l1_raw.expire,
        Some(expire_time),
        "L1 refilled expire time should match"
    );
    assert_eq!(
        l1_raw.stale,
        Some(stale_time),
        "L1 refilled stale time should match"
    );

    // Verify L2 was refilled with correct TTL/stale
    let l2_raw = l2.get_raw(&key).expect("L2 should be refilled");
    assert_eq!(
        l2_raw.expire,
        Some(expire_time),
        "L2 refilled expire time should match"
    );
    assert_eq!(
        l2_raw.stale,
        Some(stale_time),
        "L2 refilled stale time should match"
    );
}

// =============================================================================
// TTL/Stale Tests - Concrete Types (TestBackend)
// =============================================================================

#[tokio::test]
async fn test_ttl_preserved_concrete() {
    let backends = TtlTestBackends::two_level();
    let cache = CompositionBackend::new(backends.l1.clone(), backends.l2.clone(), TestOffload);
    run_ttl_preserved_test(cache, &backends.l1, &backends.l2).await;
}

#[tokio::test]
async fn test_stale_preserved_concrete() {
    let backends = TtlTestBackends::two_level();
    let cache = CompositionBackend::new(backends.l1.clone(), backends.l2.clone(), TestOffload);
    run_stale_preserved_test(cache, &backends.l1, &backends.l2).await;
}

#[tokio::test]
async fn test_refill_ttl_stale_concrete() {
    let backends = TtlTestBackends::two_level();
    let cache = CompositionBackend::new(backends.l1.clone(), backends.l2.clone(), TestOffload);
    run_refill_ttl_stale_test(cache, &backends.l1, &backends.l2).await;
}

#[tokio::test]
async fn test_no_ttl_no_stale_concrete() {
    let backends = TtlTestBackends::two_level();
    let cache = CompositionBackend::new(backends.l1.clone(), backends.l2.clone(), TestOffload);
    run_no_ttl_no_stale_test(cache, &backends.l1).await;
}

#[tokio::test]
async fn test_nested_refill_ttl_stale_concrete() {
    let backends = TtlTestBackends::three_level();
    let l3 = backends.l3.as_ref().unwrap();
    let l2_l3 = CompositionBackend::new(backends.l2.clone(), l3.clone(), TestOffload);
    let cache = CompositionBackend::new(backends.l1.clone(), l2_l3, TestOffload);
    run_nested_refill_ttl_stale_test(cache, &backends.l1, &backends.l2, l3).await;
}

// =============================================================================
// TTL/Stale Tests - Arc<SyncBackend> (Dynamic Dispatch)
// =============================================================================

#[tokio::test]
async fn test_ttl_preserved_arc_sync() {
    let backends = TtlTestBackends::two_level();
    let l1: Arc<SyncBackend> = Arc::new(backends.l1.clone());
    let l2: Arc<SyncBackend> = Arc::new(backends.l2.clone());
    let cache = CompositionBackend::new(l1, l2, TestOffload);
    run_ttl_preserved_test(cache, &backends.l1, &backends.l2).await;
}

#[tokio::test]
async fn test_stale_preserved_arc_sync() {
    let backends = TtlTestBackends::two_level();
    let l1: Arc<SyncBackend> = Arc::new(backends.l1.clone());
    let l2: Arc<SyncBackend> = Arc::new(backends.l2.clone());
    let cache = CompositionBackend::new(l1, l2, TestOffload);
    run_stale_preserved_test(cache, &backends.l1, &backends.l2).await;
}

#[tokio::test]
async fn test_refill_ttl_stale_arc_sync() {
    let backends = TtlTestBackends::two_level();
    let l1: Arc<SyncBackend> = Arc::new(backends.l1.clone());
    let l2: Arc<SyncBackend> = Arc::new(backends.l2.clone());
    let cache = CompositionBackend::new(l1, l2, TestOffload);
    run_refill_ttl_stale_test(cache, &backends.l1, &backends.l2).await;
}

#[tokio::test]
async fn test_no_ttl_no_stale_arc_sync() {
    let backends = TtlTestBackends::two_level();
    let l1: Arc<SyncBackend> = Arc::new(backends.l1.clone());
    let l2: Arc<SyncBackend> = Arc::new(backends.l2.clone());
    let cache = CompositionBackend::new(l1, l2, TestOffload);
    run_no_ttl_no_stale_test(cache, &backends.l1).await;
}

#[tokio::test]
async fn test_nested_refill_ttl_stale_arc_sync() {
    let backends = TtlTestBackends::three_level();
    let l3 = backends.l3.as_ref().unwrap();
    let l1: Arc<SyncBackend> = Arc::new(backends.l1.clone());
    let l2: Arc<SyncBackend> = Arc::new(backends.l2.clone());
    let l3_arc: Arc<SyncBackend> = Arc::new(l3.clone());
    let l2_l3 = CompositionBackend::new(l2, l3_arc, TestOffload);
    let cache = CompositionBackend::new(l1, l2_l3, TestOffload);
    run_nested_refill_ttl_stale_test(cache, &backends.l1, &backends.l2, l3).await;
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
    let mut ctx: BoxContext = CacheContext::default().boxed();
    l1.set::<TestValue>(&key, &value, &mut ctx).await.unwrap();
    let mut ctx: BoxContext = CacheContext::default().boxed();
    l2.set::<TestValue>(&key, &value, &mut ctx).await.unwrap();
    let mut ctx: BoxContext = CacheContext::default().boxed();
    l3.set::<TestValue>(&key, &value, &mut ctx).await.unwrap();

    // Verify all have the data
    assert!(l1.has(&key));
    assert!(l2.has(&key));
    assert!(l3.has(&key));

    // Create nested composition and delete
    let l2_l3 = CompositionBackend::new(l2.clone(), l3.clone(), TestOffload);
    let cache = CompositionBackend::new(l1.clone(), l2_l3, TestOffload);

    let mut ctx: BoxContext = CacheContext::default().boxed();
    cache.delete(&key, &mut ctx).await.unwrap();

    // Verify all levels no longer have the data
    assert!(!l1.has(&key), "L1 should be deleted");
    assert!(!l2.has(&key), "L2 should be deleted");
    assert!(!l3.has(&key), "L3 should be deleted");
}
