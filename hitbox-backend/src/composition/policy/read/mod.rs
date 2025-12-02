//! Read policies for controlling read operations across cache layers.
//!
//! This module defines the CompositionReadPolicy trait and its implementations.
//! Different strategies (sequential, race, parallel) can be used to optimize
//! read performance based on application requirements.

use async_trait::async_trait;
use hitbox_core::{BoxContext, CacheKey, CacheValue, Offload};
use std::future::Future;

use crate::composition::CompositionLayer;

pub mod parallel;
pub mod race;
pub mod sequential;

pub use parallel::ParallelReadPolicy;
pub use race::{RaceLoserPolicy, RaceReadPolicy};
pub use sequential::SequentialReadPolicy;

/// Result of a read operation including the value, source layer, and context.
pub struct ReadResult<T> {
    /// The cached value if found.
    pub value: Option<CacheValue<T>>,
    /// Which layer provided the data.
    pub source: CompositionLayer,
    /// The context from the layer that provided the data (for merging).
    pub context: BoxContext,
}

/// Policy trait for controlling read operations across composition cache layers.
///
/// This trait encapsulates the **control flow strategy** (sequential, race, parallel)
/// for reading from multiple cache layers (L1/L2), while delegating the actual read
/// operations to provided closures. This design allows the same policy to be used at
/// both the `CacheBackend` level (typed data) and `Backend` level (raw bytes).
///
/// # Type Parameters
///
/// The policy is generic over:
/// * `T` - The return type (e.g., `CacheValue<User>` or `CacheValue<Raw>`)
/// * `E` - The error type (e.g., `BackendError`)
/// * `F1, F2` - Closures for reading from L1 and L2
/// * `O` - The offload type for background task execution
///
/// # Example
///
/// ```ignore
/// use hitbox_backend::composition::policy::CompositionReadPolicy;
///
/// let policy = SequentialReadPolicy::default();
///
/// // Use with CacheBackend level (key is cloned for each closure)
/// let result = policy.execute_with(
///     key.clone(),
///     |k| async move { (l1.get::<User>(&k, &mut ctx).await, ctx) },
///     |k| async move {
///         let value = l2.get::<User>(&k, &mut ctx).await?;
///         // Populate L1 on L2 hit
///         if let Some(ref v) = value {
///             l1.set::<User>(&k, v, v.ttl(), &mut ctx.clone_box()).await.ok();
///         }
///         Ok((value, ctx))
///     },
///     &offload,
/// ).await?;
/// ```
#[async_trait]
pub trait CompositionReadPolicy: Send + Sync {
    /// Execute a read operation with custom read closures for each layer.
    ///
    /// The policy determines the control flow (when and how to call the closures),
    /// while the closures handle the actual reading and any post-processing
    /// (like L1 population or envelope wrapping).
    ///
    /// # Arguments
    /// * `key` - The cache key to look up
    /// * `read_l1` - Closure that reads from L1, returns (result, context)
    /// * `read_l2` - Closure that reads from L2, returns (result, context)
    /// * `offload` - Offload manager for spawning background tasks (e.g., losing race futures)
    ///
    /// # Returns
    /// A `ReadResult` containing the value, source layer, and context from the layer
    /// that provided the data. The context can be used for merging with outer context.
    async fn execute_with<T, E, F1, F2, Fut1, Fut2, O>(
        &self,
        key: CacheKey,
        read_l1: F1,
        read_l2: F2,
        offload: &O,
    ) -> Result<ReadResult<T>, E>
    where
        T: Send + 'static,
        E: Send + std::fmt::Debug + 'static,
        F1: FnOnce(CacheKey) -> Fut1 + Send,
        F2: FnOnce(CacheKey) -> Fut2 + Send,
        Fut1: Future<Output = (Result<Option<CacheValue<T>>, E>, BoxContext)> + Send + 'static,
        Fut2: Future<Output = (Result<Option<CacheValue<T>>, E>, BoxContext)> + Send + 'static,
        O: Offload;
}
