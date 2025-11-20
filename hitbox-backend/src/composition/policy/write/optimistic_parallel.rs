//! Optimistic parallel write policy implementation.
//!
//! This policy writes to both L1 and L2 simultaneously and succeeds if at least
//! one write succeeds, maximizing availability at the cost of potential inconsistency.

use async_trait::async_trait;
use hitbox_core::CacheKey;
use std::future::Future;

use crate::{BackendError, composition::CompositionError};
use super::WritePolicy;

/// Optimistic parallel write policy: Write to both simultaneously, succeed if ≥1 succeeds.
///
/// This strategy provides:
/// - Maximum availability (succeeds unless both fail)
/// - Fast writes (parallel execution)
/// - Weak consistency (layers may diverge)
///
/// # Behavior
/// 1. Start both `write_l1(key)` and `write_l2(key)` in parallel
/// 2. Wait for both to complete
/// 3. Aggregate results:
///    - Both succeed: Return Ok (best case)
///    - One succeeds: Return Ok with warning (partial success)
///    - Both fail: Return Err
///
/// # Consistency Guarantee
/// If this operation returns `Ok(())`, **at least one** of L1 or L2 has been updated.
/// This could mean:
/// - Both updated (strong consistency)
/// - Only L1 updated (L2 failed)
/// - Only L2 updated (L1 failed)
///
/// # Tradeoffs
/// - **Pros**: Highest availability, fast writes, tolerates partial failures
/// - **Cons**: Layers may diverge, need monitoring for partial failures
///
/// # Use Cases
/// - High availability requirements
/// - Non-critical data where eventual consistency is acceptable
/// - Systems with background reconciliation
/// - Degraded mode operation
#[derive(Debug, Clone, Copy, Default)]
pub struct OptimisticParallelWritePolicy;

impl OptimisticParallelWritePolicy {
    /// Create a new optimistic parallel write policy.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl WritePolicy for OptimisticParallelWritePolicy {
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
        // Write to both in parallel
        let (l1_result, l2_result) = futures::join!(write_l1(key), write_l2(key));

        // Aggregate results - succeed if at least one succeeds
        match (l1_result, l2_result) {
            (Ok(()), Ok(())) => {
                // Both succeeded - ideal case
                tracing::trace!("Both L1 and L2 writes succeeded");
                Ok(())
            }
            (Ok(()), Err(e)) => {
                // L1 succeeded, L2 failed - partial success
                tracing::warn!(
                    error = ?e,
                    "L2 write failed but L1 succeeded - partial success"
                );
                Ok(()) // Optimistic: succeed if L1 is ok
            }
            (Err(e), Ok(())) => {
                // L1 failed, L2 succeeded - partial success
                tracing::warn!(
                    error = ?e,
                    "L1 write failed but L2 succeeded - partial success"
                );
                Ok(()) // Optimistic: succeed if L2 is ok
            }
            (Err(e1), Err(e2)) => {
                // Both failed - preserve both errors for debugging
                tracing::error!(
                    l1_error = ?e1,
                    l2_error = ?e2,
                    "Both L1 and L2 writes failed"
                );
                Err(BackendError::InternalError(Box::new(
                    CompositionError::BothLayersFailed { l1: e1, l2: e2 }
                )))
            }
        }
    }
}
