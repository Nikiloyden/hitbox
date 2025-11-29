//! Race write policy implementation.
//!
//! This policy writes to both L1 and L2 simultaneously and returns as soon as
//! the first write succeeds, dropping the other (like RaceReadPolicy).

use async_trait::async_trait;
use futures::future::{Either, select};
use futures::pin_mut;
use hitbox_core::CacheKey;
use std::future::Future;

use super::CompositionWritePolicy;
use crate::BackendError;
use crate::composition::CompositionError;

/// Race write policy: Write to both simultaneously, return on first success.
///
/// This strategy provides:
/// - Minimal write latency (return as soon as one succeeds)
/// - High availability (succeeds if either layer is available)
///
/// # Behavior
/// 1. Start both `write_l1(key)` and `write_l2(key)` in parallel using `select`
/// 2. When the first completes:
///    - If success: Return Ok immediately, drop the other future
///    - If failure: Wait for the second to complete
/// 3. If both fail: Return error with both failures
///
/// # Consistency Guarantee
/// If this operation returns `Ok(())`, **at least one** of L1 or L2 has been updated.
/// The other layer may or may not be updated (its future is dropped on success).
///
/// # Tradeoffs
/// - **Pros**: Lowest latency, high availability
/// - **Cons**: Only one layer guaranteed to be written, potential inconsistency
///
/// # Use Cases
/// - Latency-critical write paths where one layer is sufficient
/// - Caches with background reconciliation
/// - Write-heavy workloads where L2 persistence is less critical
///
/// # Note
/// If you need both layers to be written, use [`OptimisticParallelWritePolicy`] instead,
/// which waits for both writes to complete.
#[derive(Debug, Clone, Copy, Default)]
pub struct RaceWritePolicy;

impl RaceWritePolicy {
    /// Create a new race write policy.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CompositionWritePolicy for RaceWritePolicy {
    #[tracing::instrument(skip(self, write_l1, write_l2), level = "trace")]
    async fn execute_with<'a, F1, F2, Fut1, Fut2>(
        &self,
        key: &'a CacheKey,
        write_l1: F1,
        write_l2: F2,
    ) -> Result<(), BackendError>
    where
        F1: FnOnce(&'a CacheKey) -> Fut1 + Send,
        F2: FnOnce(&'a CacheKey) -> Fut2 + Send,
        Fut1: Future<Output = Result<(), BackendError>> + Send + 'a,
        Fut2: Future<Output = Result<(), BackendError>> + Send + 'a,
    {
        let l1_fut = write_l1(key);
        let l2_fut = write_l2(key);

        // Pin both futures for select
        pin_mut!(l1_fut, l2_fut);

        // Race both futures
        match select(l1_fut, l2_fut).await {
            Either::Left((l1_result, l2_fut)) => {
                // L1 completed first
                match l1_result {
                    Ok(()) => {
                        // L1 succeeded - return immediately, drop L2 future
                        tracing::trace!("L1 write succeeded (won race)");
                        // l2_fut is dropped here
                        Ok(())
                    }
                    Err(e1) => {
                        // L1 failed - must wait for L2
                        tracing::trace!("L1 write failed, waiting for L2");
                        match l2_fut.await {
                            Ok(()) => {
                                tracing::trace!("L2 write succeeded after L1 failure");
                                Ok(())
                            }
                            Err(e2) => {
                                tracing::error!(
                                    l1_error = ?e1,
                                    l2_error = ?e2,
                                    "Both L1 and L2 writes failed"
                                );
                                Err(BackendError::InternalError(Box::new(
                                    CompositionError::BothLayersFailed { l1: e1, l2: e2 },
                                )))
                            }
                        }
                    }
                }
            }
            Either::Right((l2_result, l1_fut)) => {
                // L2 completed first
                match l2_result {
                    Ok(()) => {
                        // L2 succeeded - return immediately, drop L1 future
                        tracing::trace!("L2 write succeeded (won race)");
                        // l1_fut is dropped here
                        Ok(())
                    }
                    Err(e2) => {
                        // L2 failed - must wait for L1
                        tracing::trace!("L2 write failed, waiting for L1");
                        match l1_fut.await {
                            Ok(()) => {
                                tracing::trace!("L1 write succeeded after L2 failure");
                                Ok(())
                            }
                            Err(e1) => {
                                tracing::error!(
                                    l1_error = ?e1,
                                    l2_error = ?e2,
                                    "Both L1 and L2 writes failed"
                                );
                                Err(BackendError::InternalError(Box::new(
                                    CompositionError::BothLayersFailed { l1: e1, l2: e2 },
                                )))
                            }
                        }
                    }
                }
            }
        }
    }
}
