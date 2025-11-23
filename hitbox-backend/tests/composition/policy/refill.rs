//! Tests for composition refill policies (AlwaysRefill, NeverRefill).

use async_trait::async_trait;
use chrono::Utc;
use hitbox_backend::composition::policy::{AlwaysRefill, NeverRefill, RefillPolicy};
use hitbox_backend::CacheBackend;
use hitbox_core::{CacheKey, CacheValue, CacheableResponse, EntityPolicyConfig, Predicate};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[cfg(feature = "rkyv_format")]
use rkyv::{Archive, Serialize as RkyvSerialize};
use rkyv_typename::TypeName;

use crate::common::TestBackend;

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

// =============================================================================
// RefillPolicy Trait Tests
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
    policy.execute(
        &value,
        || async {
            l1.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await
        }
    ).await;

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
    policy.execute(
        &value,
        || async {
            l1.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await
        }
    ).await;

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
    policy.execute(
        &value,
        || async {
            l1.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await
        }
    ).await;

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
    policy.execute(
        &value,
        || async {
            l1.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await
        }
    ).await;

    assert!(!l1.has(&key));
}

#[tokio::test]
async fn test_default_always_refill() {
    let policy = AlwaysRefill::default();
    let l1 = TestBackend::new();
    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    policy.execute(
        &value,
        || async {
            l1.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await
        }
    ).await;

    assert!(l1.has(&key));
}

#[tokio::test]
async fn test_default_never_refill() {
    let policy = NeverRefill::default();
    let l1 = TestBackend::new();
    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "test".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    policy.execute(
        &value,
        || async {
            l1.set::<TestValue>(&key, &value, Some(Duration::from_secs(60))).await
        }
    ).await;

    assert!(!l1.has(&key));
}
