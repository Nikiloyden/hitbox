//! Sequential read policy implementation.
//!
//! This policy tries L1 first, then falls back to L2 on miss or error.
//! It's the default and most common strategy for multi-tier caching.

use async_trait::async_trait;
use hitbox_core::{CacheKey, CacheValue};
use std::future::Future;

use super::ReadPolicy;

/// Sequential read policy: Try L1 first, then L2 on miss.
///
/// This is the default and most common strategy. It provides:
/// - Fast reads from L1 when available
/// - Fallback to L2 if L1 misses or fails
/// - Graceful degradation if L1 fails
///
/// # Behavior
/// 1. Call `read_l1(key)`
///    - Hit: Return immediately
///    - Miss or Error: Continue to L2
/// 2. Call `read_l2(key)`
///    - Hit: Return value (L2 closure handles any L1 population)
///    - Miss: Return None
///    - Error: Return error
///
/// # Note
/// The closures passed to `execute_with` are responsible for any post-processing
/// like L1 population or envelope wrapping. This keeps the policy focused purely
/// on the control flow strategy.
#[derive(Debug, Clone, Copy, Default)]
pub struct SequentialReadPolicy;

impl SequentialReadPolicy {
    /// Create a new sequential read policy.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ReadPolicy for SequentialReadPolicy {
    #[tracing::instrument(skip(self, read_l1, read_l2), level = "trace")]
    async fn execute_with<'a, T, E, F1, F2, Fut1, Fut2>(
        &self,
        key: &'a CacheKey,
        read_l1: F1,
        read_l2: F2,
    ) -> Result<Option<CacheValue<T>>, E>
    where
        T: Send + 'a,
        E: Send + std::fmt::Debug + 'a,
        F1: FnOnce(&'a CacheKey) -> Fut1 + Send,
        F2: FnOnce(&'a CacheKey) -> Fut2 + Send,
        Fut1: Future<Output = Result<Option<CacheValue<T>>, E>> + Send + 'a,
        Fut2: Future<Output = Result<Option<CacheValue<T>>, E>> + Send + 'a,
    {
        // Try L1 first
        match read_l1(key).await {
            Ok(Some(value)) => {
                // L1 hit - return immediately
                tracing::trace!("L1 hit");
                return Ok(Some(value));
            }
            Ok(None) => {
                // L1 miss - continue to L2
                tracing::trace!("L1 miss");
            }
            Err(e) => {
                // L1 error - log and continue to L2
                tracing::warn!(error = ?e, "L1 read failed");
            }
        }

        // Try L2
        match read_l2(key).await {
            Ok(Some(value)) => {
                // L2 hit
                tracing::trace!("L2 hit");
                Ok(Some(value))
            }
            Ok(None) => {
                // L2 miss
                tracing::trace!("L2 miss");
                Ok(None)
            }
            Err(e) => {
                // L2 error
                tracing::error!(error = ?e, "L2 read failed");
                Err(e)
            }
        }
    }
}
