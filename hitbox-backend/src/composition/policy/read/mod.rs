//! Read policies for controlling read operations across cache layers.
//!
//! This module defines the ReadPolicy trait and its implementations.
//! Different strategies (sequential, race, parallel) can be used to optimize
//! read performance based on application requirements.

use async_trait::async_trait;
use hitbox_core::{CacheKey, CacheValue};
use std::future::Future;

use crate::composition::CompositionSource;

pub mod parallel;
pub mod race;
pub mod sequential;

pub use parallel::ParallelReadPolicy;
pub use race::RaceReadPolicy;
pub use sequential::SequentialReadPolicy;

/// Policy trait for controlling read operations across cache layers.
///
/// This trait encapsulates the **control flow strategy** (sequential, race, parallel)
/// for reading from multiple cache layers, while delegating the actual read operations
/// to provided closures. This design allows the same policy to be used at both the
/// `CacheBackend` level (typed data) and `Backend` level (raw bytes).
///
/// # Type Parameters
///
/// The policy is generic over:
/// * `T` - The return type (e.g., `CacheValue<User>` or `CacheValue<Raw>`)
/// * `E` - The error type (e.g., `BackendError`)
/// * `F1, F2` - Closures for reading from L1 and L2
///
/// # Example
///
/// ```ignore
/// use hitbox_backend::composition::policy::ReadPolicy;
///
/// let policy = SequentialReadPolicy::default();
///
/// // Use with CacheBackend level
/// let result = policy.execute_with(
///     &key,
///     |k| async { l1.get::<User>(k).await },
///     |k| async {
///         let value = l2.get::<User>(k).await?;
///         // Populate L1 on L2 hit
///         if let Some(ref v) = value {
///             l1.set::<User>(k, v, v.ttl()).await.ok();
///         }
///         Ok(value)
///     },
/// ).await?;
/// ```
#[async_trait]
pub trait ReadPolicy: Send + Sync {
    /// Execute a read operation with custom read closures for each layer.
    ///
    /// The policy determines the control flow (when and how to call the closures),
    /// while the closures handle the actual reading and any post-processing
    /// (like L1 population or envelope wrapping).
    ///
    /// # Arguments
    /// * `key` - The cache key to look up
    /// * `read_l1` - Closure that reads from L1
    /// * `read_l2` - Closure that reads from L2 (only called if L1 misses/fails)
    ///
    /// # Returns
    /// A tuple of (value, source) where source indicates which layer provided the data.
    async fn execute_with<'a, T, E, F1, F2, Fut1, Fut2>(
        &self,
        key: &'a CacheKey,
        read_l1: F1,
        read_l2: F2,
    ) -> Result<(Option<CacheValue<T>>, CompositionSource), E>
    where
        T: Send + 'a,
        E: Send + std::fmt::Debug + 'a,
        F1: FnOnce(&'a CacheKey) -> Fut1 + Send,
        F2: FnOnce(&'a CacheKey) -> Fut2 + Send,
        Fut1: Future<Output = Result<Option<CacheValue<T>>, E>> + Send + 'a,
        Fut2: Future<Output = Result<Option<CacheValue<T>>, E>> + Send + 'a;
}
