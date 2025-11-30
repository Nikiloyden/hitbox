//! Sequential read policy implementation.
//!
//! This policy tries L1 first, then falls back to L2 on miss or error.
//! It's the default and most common strategy for multi-tier caching.

use async_trait::async_trait;
use hitbox_core::{BoxContext, CacheKey, CacheValue, Offload};
use std::future::Future;

use super::{CompositionReadPolicy, ReadResult};
use crate::composition::CompositionLayer;

/// Sequential read policy: Try L1 first, then L2 on miss.
///
/// This is the default and most common strategy. It provides:
/// - Fast reads from L1 when available
/// - Fallback to L2 if L1 misses or fails
/// - Graceful degradation if L1 fails
///
/// # Behavior
/// 1. Call `read_l1(key)`
///    - Hit: Return immediately with L1 context
///    - Miss or Error: Continue to L2
/// 2. Call `read_l2(key)`
///    - Hit: Return value with L2 context (L2 closure handles any L1 population)
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
impl CompositionReadPolicy for SequentialReadPolicy {
    #[tracing::instrument(skip(self, key, read_l1, read_l2, _offload), level = "trace")]
    async fn execute_with<T, E, F1, F2, Fut1, Fut2, O>(
        &self,
        key: CacheKey,
        read_l1: F1,
        read_l2: F2,
        _offload: &O,
    ) -> Result<ReadResult<T>, E>
    where
        T: Send + 'static,
        E: Send + std::fmt::Debug + 'static,
        F1: FnOnce(CacheKey) -> Fut1 + Send,
        F2: FnOnce(CacheKey) -> Fut2 + Send,
        Fut1: Future<Output = (Result<Option<CacheValue<T>>, E>, BoxContext)> + Send + 'static,
        Fut2: Future<Output = (Result<Option<CacheValue<T>>, E>, BoxContext)> + Send + 'static,
        O: Offload,
    {
        // Try L1 first
        let (l1_result, l1_ctx) = read_l1(key.clone()).await;
        match l1_result {
            Ok(Some(value)) => {
                // L1 hit - return immediately with L1 context
                tracing::trace!("L1 hit");
                return Ok(ReadResult {
                    value: Some(value),
                    source: CompositionLayer::L1,
                    context: l1_ctx,
                });
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

        // Try L2 - keep L1 context to merge metrics
        let (l2_result, mut l2_ctx) = read_l2(key).await;

        // Merge L1 metrics into L2 context (L1 was queried even if it missed)
        // We merge directly without prefix since both are at the same level
        for (source, layer_metrics) in l1_ctx.metrics().layers.iter() {
            l2_ctx
                .metrics_mut()
                .layers
                .entry(source.clone())
                .or_default()
                .merge(layer_metrics);
        }

        match l2_result {
            Ok(Some(value)) => {
                // L2 hit - return with combined context
                tracing::trace!("L2 hit");
                Ok(ReadResult {
                    value: Some(value),
                    source: CompositionLayer::L2,
                    context: l2_ctx,
                })
            }
            Ok(None) => {
                // L2 miss - return combined context
                tracing::trace!("L2 miss");
                Ok(ReadResult {
                    value: None,
                    source: CompositionLayer::L2,
                    context: l2_ctx,
                })
            }
            Err(e) => {
                // L2 error
                tracing::error!(error = ?e, "L2 read failed");
                Err(e)
            }
        }
    }
}
