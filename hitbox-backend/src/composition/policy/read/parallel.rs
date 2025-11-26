//! Parallel read policy implementation.
//!
//! This policy queries both L1 and L2 simultaneously and waits for both to complete,
//! preferring the response with the longest remaining TTL (freshest data).

use async_trait::async_trait;
use hitbox_core::{BoxContext, CacheContext, CacheKey, CacheValue};
use std::future::Future;

use super::{CompositionReadPolicy, ReadResult};
use crate::composition::CompositionLayer;

/// Parallel read policy: Query both L1 and L2 in parallel, prefer freshest data (by TTL).
///
/// This strategy provides:
/// - Freshness guarantee (returns data with longest remaining TTL)
/// - Cache warming (keeps both layers hot)
/// - Observability into both layer performance
/// - Natural cache coherency (prefers recently updated data)
///
/// # Behavior
/// 1. Start both `read_l1(key)` and `read_l2(key)` in parallel
/// 2. Wait for **both** to complete
/// 3. Compare TTLs and prefer the response with **longest remaining TTL**
/// 4. Fall back to any available value if one layer misses/errors
///
/// # TTL Comparison Rules
/// When both L1 and L2 have data:
/// - Compare remaining TTLs using `CacheValue::ttl()`
/// - Prefer response with longer TTL (fresher data)
/// - If one has no expiry (`None` TTL), prefer it (infinite freshness)
/// - If TTLs are equal, prefer L1 (tie-breaker)
/// - If both have no expiry, prefer L1 (tie-breaker)
///
/// # Tradeoffs
/// - **Pros**: Freshness guarantee, handles L1/L2 consistency naturally, production-viable
/// - **Cons**: 2x backend load, latency limited by slower backend
///
/// # Use Cases
/// - Production systems where data freshness is critical
/// - Multi-region setups where L2 may get updated first
/// - Cache warming while ensuring freshest data
/// - Validating L1/L2 consistency
/// - Monitoring both layer health
///
/// # Note
/// Unlike `RaceReadPolicy`, this policy always waits for both backends to complete,
/// making it slower but providing freshness guarantees and better observability.
#[derive(Debug, Clone, Copy, Default)]
pub struct ParallelReadPolicy;

impl ParallelReadPolicy {
    /// Create a new parallel read policy.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CompositionReadPolicy for ParallelReadPolicy {
    #[tracing::instrument(skip(self, read_l1, read_l2), level = "trace")]
    async fn execute_with<'a, T, E, F1, F2, Fut1, Fut2>(
        &self,
        key: &'a CacheKey,
        read_l1: F1,
        read_l2: F2,
    ) -> Result<ReadResult<T>, E>
    where
        T: Send + 'a,
        E: Send + std::fmt::Debug + 'a,
        F1: FnOnce(&'a CacheKey) -> Fut1 + Send,
        F2: FnOnce(&'a CacheKey) -> Fut2 + Send,
        Fut1: Future<Output = (Result<Option<CacheValue<T>>, E>, BoxContext)> + Send + 'a,
        Fut2: Future<Output = (Result<Option<CacheValue<T>>, E>, BoxContext)> + Send + 'a,
    {
        // Query both in parallel and wait for both to complete
        let ((l1_result, l1_ctx), (l2_result, l2_ctx)) =
            futures::join!(read_l1(key), read_l2(key));

        // Aggregate results, preferring freshest data (by TTL)
        match (l1_result, l2_result) {
            // Both hit - compare TTLs to select freshest
            (Ok(Some(l1_value)), Ok(Some(l2_value))) => {
                // Compare TTLs: prefer longer remaining TTL, or L1 on tie
                match (l1_value.ttl(), l2_value.ttl()) {
                    (Some(l1_ttl), Some(l2_ttl)) if l2_ttl > l1_ttl => {
                        tracing::trace!("Both hit, preferring L2 (fresher TTL)");
                        Ok(ReadResult {
                            value: Some(l2_value),
                            source: CompositionLayer::L2,
                            context: l2_ctx,
                        })
                    }
                    (Some(_), None) => {
                        tracing::trace!("Both hit, preferring L2 (no expiry)");
                        Ok(ReadResult {
                            value: Some(l2_value),
                            source: CompositionLayer::L2,
                            context: l2_ctx,
                        })
                    }
                    _ => {
                        // L1 >= L2, or L1 has no expiry, or both no expiry - prefer L1
                        tracing::trace!("Both hit, preferring L1 (fresher or equal TTL)");
                        Ok(ReadResult {
                            value: Some(l1_value),
                            source: CompositionLayer::L1,
                            context: l1_ctx,
                        })
                    }
                }
            }
            // L1 hit, L2 miss/error
            (Ok(Some(value)), _) => {
                tracing::trace!("L1 hit, L2 miss/error");
                Ok(ReadResult {
                    value: Some(value),
                    source: CompositionLayer::L1,
                    context: l1_ctx,
                })
            }
            // L2 hit, L1 miss/error
            (_, Ok(Some(value))) => {
                tracing::trace!("L2 hit, L1 miss/error");
                Ok(ReadResult {
                    value: Some(value),
                    source: CompositionLayer::L2,
                    context: l2_ctx,
                })
            }
            // Both miss
            (Ok(None), Ok(None)) => {
                tracing::trace!("Both layers miss");
                Ok(ReadResult {
                    value: None,
                    source: CompositionLayer::L2,
                    context: Box::new(CacheContext::default()),
                })
            }
            // Both error
            (Err(e1), Err(e2)) => {
                tracing::error!(l1_error = ?e1, l2_error = ?e2, "Both layers failed");
                Err(e2)
            }
            // One error, one miss
            (Ok(None), Err(e)) | (Err(e), Ok(None)) => {
                tracing::warn!(error = ?e, "One layer failed, one missed");
                Ok(ReadResult {
                    value: None,
                    source: CompositionLayer::L2,
                    context: Box::new(CacheContext::default()),
                })
            }
        }
    }
}
