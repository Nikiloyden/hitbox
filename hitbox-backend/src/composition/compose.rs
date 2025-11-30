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
//! let cache = mem_backend.compose(redis_backend, offload);
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
//! let cache = mem_backend.compose_with(redis_backend, offload, policy);
//! ```

use super::policy::{CompositionReadPolicy, CompositionRefillPolicy, CompositionWritePolicy};
use super::{CompositionBackend, CompositionPolicy};
use crate::Backend;
use hitbox_core::Offload;

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
/// let cache = l1_backend.compose(l2_backend, offload);
///
/// // Composition with custom policies
/// let policy = CompositionPolicy::new()
///     .read(RaceReadPolicy::new())
///     .write(SequentialWritePolicy::new());
///
/// let cache = l1_backend.compose_with(l2_backend, offload, policy);
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
    /// * `offload` - Offload manager for background tasks
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
    /// let cache = moka.compose(redis, offload);
    /// ```
    fn compose<L2, O>(self, l2: L2, offload: O) -> CompositionBackend<Self, L2, O>
    where
        L2: Backend,
        O: Offload,
    {
        CompositionBackend::new(self, l2, offload)
    }

    /// Compose this backend with another backend as L2, using custom policies.
    ///
    /// This provides full control over read, write, and refill policies.
    ///
    /// # Arguments
    /// * `l2` - The second-layer backend
    /// * `offload` - Offload manager for background tasks
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
    /// let cache = l1.compose_with(l2, offload, policy);
    /// ```
    fn compose_with<L2, O, R, W, F>(
        self,
        l2: L2,
        offload: O,
        policy: CompositionPolicy<R, W, F>,
    ) -> CompositionBackend<Self, L2, O, R, W, F>
    where
        L2: Backend,
        O: Offload,
        R: CompositionReadPolicy,
        W: CompositionWritePolicy,
        F: CompositionRefillPolicy,
    {
        CompositionBackend::new(self, l2, offload).with_policy(policy)
    }
}

// Blanket implementation for all Backend types
impl<T: Backend> Compose for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{Format, JsonFormat};
    use crate::{
        Backend, BackendResult, CacheBackend, CacheKeyFormat, Compressor, DeleteStatus,
        PassthroughCompressor,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use hitbox_core::{
        BoxContext, CacheContext, CacheKey, CachePolicy, CacheValue, CacheableResponse,
        EntityPolicyConfig, Predicate, Raw,
    };
    use serde::{Deserialize, Serialize};
    use smol_str::SmolStr;
    use std::collections::HashMap;
    use std::future::Future;
    use std::sync::{Arc, Mutex};

    #[cfg(feature = "rkyv_format")]
    use rkyv::{Archive, Serialize as RkyvSerialize};
    #[cfg(feature = "rkyv_format")]
    use rkyv_typename::TypeName;

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

        async fn write(&self, key: &CacheKey, value: CacheValue<Raw>) -> BackendResult<()> {
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

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[cfg_attr(
        feature = "rkyv_format",
        derive(Archive, RkyvSerialize, rkyv::Deserialize, TypeName)
    )]
    #[cfg_attr(feature = "rkyv_format", archive(check_bytes))]
    #[cfg_attr(feature = "rkyv_format", archive_attr(derive(TypeName)))]
    struct CachedData {
        value: String,
    }

    struct MockResponse;

    #[async_trait]
    impl CacheableResponse for MockResponse {
        type Cached = CachedData;
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
        let offload = TestOffload;

        // Use compose trait
        let cache = l1.clone().compose(l2.clone(), offload);

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            CachedData {
                value: "test_value".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write and read
        let mut ctx: BoxContext = CacheContext::default().boxed();
        cache
            .set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        let mut ctx: BoxContext = CacheContext::default().boxed();
        let result = cache.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(result.unwrap().data.value, "test_value");

        // Verify both layers have the data
        let mut ctx: BoxContext = CacheContext::default().boxed();
        assert!(
            l1.get::<MockResponse>(&key, &mut ctx)
                .await
                .unwrap()
                .is_some()
        );
        let mut ctx: BoxContext = CacheContext::default().boxed();
        assert!(
            l2.get::<MockResponse>(&key, &mut ctx)
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_compose_with_policy() {
        use super::super::policy::{NeverRefill, RaceReadPolicy};

        let l1 = TestBackend::new();
        let l2 = TestBackend::new();
        let offload = TestOffload;

        // Use compose_with to specify custom policies
        let policy = CompositionPolicy::new()
            .read(RaceReadPolicy::new())
            .refill(NeverRefill::new());

        let cache = l1.clone().compose_with(l2.clone(), offload, policy);

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            CachedData {
                value: "from_l2".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Populate only L2
        let mut ctx: BoxContext = CacheContext::default().boxed();
        l2.set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        // Read through composition (should use RaceReadPolicy)
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let result = cache.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(result.unwrap().data.value, "from_l2");

        // With NeverRefill, L1 should NOT be populated
        let mut ctx: BoxContext = CacheContext::default().boxed();
        assert!(
            l1.get::<MockResponse>(&key, &mut ctx)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_compose_nested() {
        // Test that composed backends can be further composed
        let l1 = TestBackend::new();
        let l2 = TestBackend::new();
        let l3 = TestBackend::new();
        let offload = TestOffload;

        // Create L2+L3 composition
        let l2_l3 = l2.clone().compose(l3.clone(), offload.clone());

        // Compose L1 with the (L2+L3) composition
        let cache = l1.clone().compose(l2_l3, offload);

        let key = CacheKey::from_str("test", "nested_key");
        let value = CacheValue::new(
            CachedData {
                value: "nested_value".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write through nested composition
        let mut ctx: BoxContext = CacheContext::default().boxed();
        cache
            .set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        // All three levels should have the data
        let mut ctx: BoxContext = CacheContext::default().boxed();
        assert!(
            l1.get::<MockResponse>(&key, &mut ctx)
                .await
                .unwrap()
                .is_some()
        );
        let mut ctx: BoxContext = CacheContext::default().boxed();
        assert!(
            l2.get::<MockResponse>(&key, &mut ctx)
                .await
                .unwrap()
                .is_some()
        );
        let mut ctx: BoxContext = CacheContext::default().boxed();
        assert!(
            l3.get::<MockResponse>(&key, &mut ctx)
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_compose_chaining() {
        use super::super::policy::RaceReadPolicy;

        let l1 = TestBackend::new();
        let l2 = TestBackend::new();
        let offload = TestOffload;

        // Test method chaining: compose + builder methods
        let cache = l1
            .clone()
            .compose(l2.clone(), offload)
            .read(RaceReadPolicy::new());

        let key = CacheKey::from_str("test", "chain_key");
        let value = CacheValue::new(
            CachedData {
                value: "chain_value".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        let mut ctx: BoxContext = CacheContext::default().boxed();
        cache
            .set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        let mut ctx: BoxContext = CacheContext::default().boxed();
        let result = cache.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(result.unwrap().data.value, "chain_value");
    }
}
