//! Write policies for controlling write operations across cache layers.
//!
//! This module defines the CompositionWritePolicy trait and its implementations.
//! Different strategies (sequential, optimistic parallel) can be used to optimize
//! write performance and availability based on application requirements.

use async_trait::async_trait;
use hitbox_core::{CacheKey, Offload};
use std::future::Future;

use crate::BackendError;

pub mod optimistic_parallel;
pub mod race;
pub mod sequential;

pub use optimistic_parallel::OptimisticParallelWritePolicy;
pub use race::{RaceLoserPolicy, RaceWritePolicy};
pub use sequential::SequentialWritePolicy;

/// Policy trait for controlling write operations across cache layers.
///
/// This trait encapsulates the **control flow strategy** (sequential, parallel, conditional)
/// for writing to multiple cache layers, while delegating the actual write operations
/// to provided closures. This design allows the same policy to be used at both the
/// `CacheBackend` level (typed data) and `Backend` level (raw bytes).
///
/// # Type Parameters
///
/// The policy is generic over:
/// * `E` - The error type (e.g., `BackendError`)
/// * `F1, F2` - Closures for writing to L1 and L2
/// * `O` - The offload type for background task execution
///
/// # Example
///
/// ```ignore
/// use hitbox_backend::composition::policy::CompositionWritePolicy;
///
/// let policy = SequentialWritePolicy::default();
///
/// // Use with CacheBackend level
/// policy.execute_with(
///     key.clone(),
///     |k| async { l1.set::<User>(&k, value, ttl).await },
///     |k| async { l2.set::<User>(&k, value, ttl).await },
///     &offload,
/// ).await?;
/// ```
#[async_trait]
pub trait CompositionWritePolicy: Send + Sync {
    /// Execute a write operation with custom write closures for each layer.
    ///
    /// The policy determines the control flow (when and how to call the closures),
    /// while the closures handle the actual writing and any pre-processing
    /// (like serialization or validation).
    ///
    /// # Arguments
    /// * `key` - The cache key to write (owned for `'static` futures)
    /// * `write_l1` - Closure that writes to L1
    /// * `write_l2` - Closure that writes to L2
    /// * `offload` - Offload manager for spawning background tasks (e.g., losing race futures)
    ///
    /// # Returns
    /// Success if the policy's success criteria are met. Different policies have
    /// different success criteria (e.g., both must succeed, or at least one must succeed).
    ///
    /// # Generic Parameters
    /// * `F1, F2` - Closures for writing to L1 and L2 that return `BackendResult<()>`
    /// * `O` - The offload type for background task execution
    ///
    /// # Error Handling
    /// When both layers fail, implementations should preserve both errors in a
    /// `CompositionError::BothLayersFailed` for better debugging.
    async fn execute_with<F1, F2, Fut1, Fut2, O>(
        &self,
        key: CacheKey,
        write_l1: F1,
        write_l2: F2,
        offload: &O,
    ) -> Result<(), BackendError>
    where
        F1: FnOnce(CacheKey) -> Fut1 + Send,
        F2: FnOnce(CacheKey) -> Fut2 + Send,
        Fut1: Future<Output = Result<(), BackendError>> + Send + 'static,
        Fut2: Future<Output = Result<(), BackendError>> + Send + 'static,
        O: Offload<'static>;
}
