//! Tests for the Compose trait API.
//!
//! These tests demonstrate the fluent API for building CompositionBackend hierarchies.

use chrono::Utc;
use hitbox_backend::composition::CompositionPolicy;
use hitbox_backend::composition::policy::{NeverRefill, RaceReadPolicy};
use hitbox_backend::{CacheBackend, Compose};
use hitbox_core::{BoxContext, CacheContext, CacheKey, CacheValue};
use std::time::Duration;

use crate::common::TestBackend;

// Reuse TestValue from nested tests
use super::nested::TestValue;

#[tokio::test]
async fn test_compose_trait_basic_usage() {
    // Basic usage: l1.compose(l2)
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();

    let cache = l1.clone().compose(l2.clone());

    let key = CacheKey::from_str("test", "key1");
    let value = CacheValue::new(
        TestValue {
            data: "compose_api".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Write through composition
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    cache
        .set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
        .await
        .unwrap();

    // Read through composition
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(
        result.unwrap().data,
        TestValue {
            data: "compose_api".to_string()
        }
    );

    // Both layers should have the data
    assert!(l1.has(&key));
    assert!(l2.has(&key));
}

#[tokio::test]
async fn test_compose_with_custom_policy() {
    // Using compose_with to specify custom policies
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();

    let policy = CompositionPolicy::new()
        .read(RaceReadPolicy)
        .refill(NeverRefill);

    let cache = l1.clone().compose_with(l2.clone(), policy);

    let key = CacheKey::from_str("test", "custom_policy");
    let value = CacheValue::new(
        TestValue {
            data: "custom_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Populate only L2
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    l2.set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
        .await
        .unwrap();

    // Read through composition (uses RaceReadPolicy)
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(
        result.unwrap().data,
        TestValue {
            data: "custom_value".to_string()
        }
    );

    // With NeverRefill, L1 should NOT be populated
    assert!(!l1.has(&key));
}

#[tokio::test]
async fn test_compose_nested_3_levels() {
    // Create 3-level hierarchy using compose trait
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let l3 = TestBackend::new();

    // Build: L1 + (L2 + L3)
    let cache = l1.clone().compose(l2.clone().compose(l3.clone()));

    let key = CacheKey::from_str("test", "nested_compose");
    let value = CacheValue::new(
        TestValue {
            data: "nested_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Write cascades to all 3 levels
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    cache
        .set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
        .await
        .unwrap();

    // All levels should have the data
    assert!(l1.has(&key), "L1 should have the value");
    assert!(l2.has(&key), "L2 should have the value");
    assert!(l3.has(&key), "L3 should have the value");

    // Read returns the value
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert_eq!(
        result.unwrap().data,
        TestValue {
            data: "nested_value".to_string()
        }
    );
}

#[tokio::test]
async fn test_compose_with_builder_chaining() {
    // Combine compose with builder methods
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();

    let cache = l1
        .clone()
        .compose(l2.clone())
        .read(RaceReadPolicy::new())
        .refill(NeverRefill::new());

    let key = CacheKey::from_str("test", "chained");
    let value = CacheValue::new(
        TestValue {
            data: "chained_value".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Populate only L2
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    l2.set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
        .await
        .unwrap();

    // Read through composition
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert_eq!(
        result.unwrap().data,
        TestValue {
            data: "chained_value".to_string()
        }
    );

    // With NeverRefill, L1 should not be populated
    assert!(!l1.has(&key));
}

#[tokio::test]
async fn test_compose_4_levels_cascade() {
    // Deep hierarchy: L1 + (L2 + (L3 + L4))
    let l1 = TestBackend::new();
    let l2 = TestBackend::new();
    let l3 = TestBackend::new();
    let l4 = TestBackend::new();

    let cache = l1
        .clone()
        .compose(l2.clone().compose(l3.clone().compose(l4.clone())));

    let key = CacheKey::from_str("test", "deep");
    let value = CacheValue::new(
        TestValue {
            data: "from_l4".to_string(),
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );

    // Populate only L4 (deepest level)
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    l4.set::<TestValue>(&key, &value, Some(Duration::from_secs(60)), &mut ctx)
        .await
        .unwrap();

    // Read cascades through all 4 levels
    let mut ctx: BoxContext = Box::new(CacheContext::default());
    let result = cache.get::<TestValue>(&key, &mut ctx).await.unwrap();
    assert_eq!(
        result.unwrap().data,
        TestValue {
            data: "from_l4".to_string()
        }
    );

    // All levels should now be populated (refill cascade)
    assert!(l1.has(&key), "L1 should be refilled");
    assert!(l2.has(&key), "L2 should be refilled");
    assert!(l3.has(&key), "L3 should be refilled");
    assert!(l4.has(&key), "L4 should have original data");
}
