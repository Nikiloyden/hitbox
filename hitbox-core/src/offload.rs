//! Offload trait for background task execution.
//!
//! This module provides the [`Offload`] trait which abstracts over
//! different implementations for spawning background tasks.

use std::future::Future;

use smol_str::SmolStr;

/// Trait for spawning background tasks.
///
/// This trait allows components like `CacheFuture` and `CompositionBackend`
/// to offload work to be executed in the background without blocking the main
/// request path.
///
/// # Implementations
///
/// The primary implementation is `OffloadManager` in the `hitbox` crate, which
/// provides configurable background task execution with policies for handling
/// task completion.
///
/// # Clone bound
///
/// Implementors should use `Arc` internally to ensure all cloned instances
/// share the same configuration and state.
///
/// # Example
///
/// ```ignore
/// use hitbox_core::Offload;
///
/// fn offload_cache_write<O: Offload>(offload: &O, key: String) {
///     offload.spawn("cache_write", async move {
///         // Perform background cache write
///         println!("Writing to cache: {}", key);
///     });
/// }
/// ```
pub trait Offload: Send + Sync + Clone {
    /// Spawn a future to be executed in the background.
    ///
    /// The future will be executed asynchronously and its result will be
    /// handled according to the implementation's policy.
    ///
    /// # Arguments
    ///
    /// * `kind` - A label categorizing the task type (e.g., "revalidate", "cache_write").
    ///   Used for metrics and tracing.
    /// * `future` - The future to execute in the background. Must be `Send + 'static`
    ///   as it may be executed on a different thread.
    fn spawn<F>(&self, kind: impl Into<SmolStr>, future: F)
    where
        F: Future<Output = ()> + Send + 'static;
}
