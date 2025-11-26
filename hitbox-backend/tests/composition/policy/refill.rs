//! Tests for composition refill policies (AlwaysRefill, NeverRefill).

use async_trait::async_trait;
use chrono::Utc;
use hitbox_backend::CacheBackend;
use hitbox_backend::composition::policy::{AlwaysRefill, CompositionRefillPolicy, NeverRefill};
use hitbox_core::{
    BoxContext, CacheContext, CacheKey, CacheValue, CacheableResponse, EntityPolicyConfig,
    Predicate,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[cfg(feature = "rkyv_format")]
use rkyv::{Archive, Serialize as RkyvSerialize};
#[cfg(feature = "rkyv_format")]
use rkyv_typename::TypeName;

use crate::common::TestBackend;

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

// =============================================================================
// CompositionRefillPolicy Trait Tests
// =============================================================================

#[tokio::test]
async fn test_always_refill_policy() {
    let policy = AlwaysRefill::new();
    let l1 = TestBackend::new();
    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Execute refill - should call the closure
    let mut ctx: BoxContext = CacheContext::default().boxed();
    policy
        .execute(&value, || async {
            l1.set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
                .await
        })
        .await;

    assert!(l1.has(&key));
}

#[tokio::test]
async fn test_never_refill_policy() {
    let policy = NeverRefill::new();
    let l1 = TestBackend::new();
    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Execute refill - should NOT call the closure
    let mut ctx: BoxContext = CacheContext::default().boxed();
    policy
        .execute(&value, || async {
            l1.set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
                .await
        })
        .await;

    assert!(!l1.has(&key));
}

// =============================================================================
// Integration Tests (using policy implementations directly)
// =============================================================================

#[tokio::test]
async fn test_manual_refill_with_always_policy() {
    let policy = AlwaysRefill::new();
    let l1 = TestBackend::new();
    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Simulate L2 hit - policy executes refill
    let mut ctx: BoxContext = CacheContext::default().boxed();
    policy
        .execute(&value, || async {
            l1.set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
                .await
        })
        .await;

    assert!(l1.has(&key));
}

#[tokio::test]
async fn test_manual_refill_with_never_policy() {
    let policy = NeverRefill::new();
    let l1 = TestBackend::new();
    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Simulate L2 hit - policy skips refill
    let mut ctx: BoxContext = CacheContext::default().boxed();
    policy
        .execute(&value, || async {
            l1.set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
                .await
        })
        .await;

    assert!(!l1.has(&key));
}

#[tokio::test]
async fn test_default_always_refill() {
    let policy = AlwaysRefill;
    let l1 = TestBackend::new();
    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    let mut ctx: BoxContext = CacheContext::default().boxed();
    policy
        .execute(&value, || async {
            l1.set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
                .await
        })
        .await;

    assert!(l1.has(&key));
}

#[tokio::test]
async fn test_default_never_refill() {
    let policy = NeverRefill;
    let l1 = TestBackend::new();
    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    let mut ctx: BoxContext = CacheContext::default().boxed();
    policy
        .execute(&value, || async {
            l1.set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
                .await
        })
        .await;

    assert!(!l1.has(&key));
}

// =============================================================================
// Metrics Tests
// =============================================================================
// Note: CompositionRefillPolicy metrics collection is tested via CompositionBackend
// integration tests (test_metrics_recorded_on_l2_hit_with_refill) because
// async closures need to own the context. Standalone CompositionRefillPolicy tests
// verify only that the closure is executed or skipped.
