//! Trait for composing backends into layered cache hierarchies.
//!
//! The `Compose` trait provides a fluent API for building `CompositionBackend` instances,
//! making it easy to create multi-level cache hierarchies with custom policies.
//!
//! # Examples
//!
//! Basic composition with default policies:
//! ```ignore
//! use hitbox_backend::composition::Compose;
//!
//! let cache = mem_backend.compose(redis_backend);
//! ```
//!
//! Composition with custom policies:
//! ```ignore
//! use hitbox_backend::composition::{Compose, CompositionPolicy};
//! use hitbox_backend::composition::policy::{RaceReadPolicy, SequentialWritePolicy};
//!
//! let policy = CompositionPolicy::new()
//!     .read(RaceReadPolicy::new())
//!     .write(SequentialWritePolicy::new());
//!
//! let cache = mem_backend.compose_with(redis_backend, policy);
//! ```

use crate::Backend;
use super::{CompositionBackend, CompositionPolicy};
use super::policy::{ReadPolicy, RefillPolicy, WritePolicy};

/// Trait for composing backends into layered cache hierarchies.
///
/// This trait is automatically implemented for all types that implement `Backend`,
/// providing a fluent API for creating `CompositionBackend` instances.
///
/// # Examples
///
/// ```ignore
/// use hitbox_backend::composition::Compose;
///
/// // Simple composition with default policies
/// let cache = l1_backend.compose(l2_backend);
///
/// // Composition with custom policies
/// let policy = CompositionPolicy::new()
///     .read(RaceReadPolicy::new())
///     .write(SequentialWritePolicy::new());
///
/// let cache = l1_backend.compose_with(l2_backend, policy);
/// ```
pub trait Compose: Backend + Sized {
    /// Compose this backend with another backend as L2, using default policies.
    ///
    /// This creates a `CompositionBackend` where:
    /// - `self` becomes L1 (first layer, checked first on reads)
    /// - `l2` becomes L2 (second layer, checked if L1 misses)
    ///
    /// Default policies:
    /// - Read: `SequentialReadPolicy` (try L1 first, then L2)
    /// - Write: `OptimisticParallelWritePolicy` (write to both, succeed if ≥1 succeeds)
    /// - Refill: `AlwaysRefill` (always populate L1 after L2 hit)
    ///
    /// # Arguments
    /// * `l2` - The second-layer backend
    ///
    /// # Example
    /// ```ignore
    /// use hitbox_backend::composition::Compose;
    /// use hitbox_moka::MokaBackend;
    /// use hitbox_redis::RedisBackend;
    ///
    /// let moka = MokaBackend::builder(1000).build();
    /// let redis = RedisBackend::new(client);
    ///
    /// // Moka as L1, Redis as L2
    /// let cache = moka.compose(redis);
    /// ```
    fn compose<L2>(self, l2: L2) -> CompositionBackend<Self, L2>
    where
        L2: Backend,
    {
        CompositionBackend::new(self, l2)
    }

    /// Compose this backend with another backend as L2, using custom policies.
    ///
    /// This provides full control over read, write, and refill policies.
    ///
    /// # Arguments
    /// * `l2` - The second-layer backend
    /// * `policy` - Custom composition policies
    ///
    /// # Example
    /// ```ignore
    /// use hitbox_backend::composition::{Compose, CompositionPolicy};
    /// use hitbox_backend::composition::policy::{RaceReadPolicy, SequentialWritePolicy};
    ///
    /// let policy = CompositionPolicy::new()
    ///     .read(RaceReadPolicy::new())
    ///     .write(SequentialWritePolicy::new());
    ///
    /// let cache = l1.compose_with(l2, policy);
    /// ```
    fn compose_with<L2, R, W, F>(
        self,
        l2: L2,
        policy: CompositionPolicy<R, W, F>,
    ) -> CompositionBackend<Self, L2, R, W, F>
    where
        L2: Backend,
        R: ReadPolicy,
        W: WritePolicy,
        F: RefillPolicy,
    {
        CompositionBackend::new(self, l2).with_policy(policy)
    }
}

// Blanket implementation for all Backend types
impl<T: Backend> Compose for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serializer::{Format, JsonFormat};
    use crate::{Backend, BackendResult, CacheBackend, CacheKeyFormat, Compressor, DeleteStatus, PassthroughCompressor};
    use async_trait::async_trait;
    use chrono::Utc;
    use hitbox_core::{CacheKey, CachePolicy, CacheValue, CacheableResponse, EntityPolicyConfig, Predicate, Raw};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

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

    struct MockResponse;

    #[async_trait]
    impl CacheableResponse for MockResponse {
        type Cached = String;
        type Subject = MockResponse;

        async fn cache_policy<P: Predicate<Subject = Self::Subject> + Send + Sync>(
            self,
            _predicate: P,
            _config: &EntityPolicyConfig,
        ) -> CachePolicy<CacheValue<Self::Cached>, Self> {
            unimplemented!()
        }

        async fn into_cached(self) -> CachePolicy<Self::Cached, Self> {
            unimplemented!()
        }

        async fn from_cached(_cached: Self::Cached) -> Self {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_compose_basic() {
        let l1 = TestBackend::new();
        let l2 = TestBackend::new();

        // Use compose trait
        let cache = l1.clone().compose(l2.clone());

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            "test_value".to_string(),
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write and read
        cache.set::<MockResponse>(&key, &value, Some(Duration::from_secs(60))).await.unwrap();

        let result = cache.get::<MockResponse>(&key).await.unwrap();
        assert_eq!(result.unwrap().data, "test_value");

        // Verify both layers have the data
        assert!(l1.get::<MockResponse>(&key).await.unwrap().is_some());
        assert!(l2.get::<MockResponse>(&key).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_compose_with_policy() {
        use super::super::policy::{NeverRefill, RaceReadPolicy};

        let l1 = TestBackend::new();
        let l2 = TestBackend::new();

        // Use compose_with to specify custom policies
        let policy = CompositionPolicy::new()
            .read(RaceReadPolicy::new())
            .refill(NeverRefill::new());

        let cache = l1.clone().compose_with(l2.clone(), policy);

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            "from_l2".to_string(),
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Populate only L2
        l2.set::<MockResponse>(&key, &value, Some(Duration::from_secs(60))).await.unwrap();

        // Read through composition (should use RaceReadPolicy)
        let result = cache.get::<MockResponse>(&key).await.unwrap();
        assert_eq!(result.unwrap().data, "from_l2");

        // With NeverRefill, L1 should NOT be populated
        assert!(l1.get::<MockResponse>(&key).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_compose_nested() {
        // Test that composed backends can be further composed
        let l1 = TestBackend::new();
        let l2 = TestBackend::new();
        let l3 = TestBackend::new();

        // Create L2+L3 composition
        let l2_l3 = l2.clone().compose(l3.clone());

        // Compose L1 with the (L2+L3) composition
        let cache = l1.clone().compose(l2_l3);

        let key = CacheKey::from_str("test", "nested_key");
        let value = CacheValue::new(
            "nested_value".to_string(),
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write through nested composition
        cache.set::<MockResponse>(&key, &value, Some(Duration::from_secs(60))).await.unwrap();

        // All three levels should have the data
        assert!(l1.get::<MockResponse>(&key).await.unwrap().is_some());
        assert!(l2.get::<MockResponse>(&key).await.unwrap().is_some());
        assert!(l3.get::<MockResponse>(&key).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_compose_chaining() {
        use super::super::policy::RaceReadPolicy;

        let l1 = TestBackend::new();
        let l2 = TestBackend::new();

        // Test method chaining: compose + builder methods
        let cache = l1.clone()
            .compose(l2.clone())
            .read(RaceReadPolicy::new());

        let key = CacheKey::from_str("test", "chain_key");
        let value = CacheValue::new(
            "chain_value".to_string(),
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        cache.set::<MockResponse>(&key, &value, Some(Duration::from_secs(60))).await.unwrap();

        let result = cache.get::<MockResponse>(&key).await.unwrap();
        assert_eq!(result.unwrap().data, "chain_value");
    }
}
