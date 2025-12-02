//! Sequential write policy implementation (Write-Through).
//!
//! This policy writes to L1 first, then writes to L2 sequentially.
//! It's the classic write-through strategy with strong consistency.

use async_trait::async_trait;
use hitbox_core::{CacheKey, Offload};
use std::future::Future;

use super::CompositionWritePolicy;
use crate::BackendError;

/// Sequential write policy: Write to L1, then L2 (write-through).
///
/// This is the default and most common strategy. It provides:
/// - Strong consistency (both layers updated before returning)
/// - Atomic updates from caller's perspective
/// - Graceful degradation if L1 succeeds but L2 fails
///
/// # Behavior
/// 1. Call `write_l1(key)`
///    - If fails: Return error immediately (don't write to L2)
///    - If succeeds: Continue to L2
/// 2. Call `write_l2(key)`
///    - If fails: Return error (L1 has data, L2 doesn't - inconsistent state)
///    - If succeeds: Return success
///
/// # Consistency Guarantees
///
/// **Success case (`Ok(())`)**: Both L1 and L2 have been updated successfully.
///
/// **Failure cases (`Err`)**:
/// - **L1 write failed**: Neither layer updated - cache remains consistent
/// - **L2 write failed**: L1 updated, L2 not updated - **inconsistent state**
///
/// ## Inconsistent State Handling
///
/// When L1 succeeds but L2 fails, the cache enters an inconsistent state where:
/// - **L1 contains the new value** - subsequent reads from this client will hit L1
/// - **L2 may contain stale data or no data** - other clients may see stale values
/// - **The error is logged** with tracing::error for monitoring
///
/// ### Mitigation Strategies:
///
/// 1. **Accept inconsistency** - If L1 is much faster and L2 failures are rare,
///    the inconsistency may be acceptable as L1 will mask it for most reads
///
/// 2. **Retry logic** - Implement retry at application level or use a RetryBackend
///    wrapper to retry failed L2 writes
///
/// 3. **Use OptimisticParallelWritePolicy** - Succeeds if either L1 or L2 succeeds,
///    providing better availability at the cost of potential inconsistency
///
/// 4. **Monitor and alert** - Track L2 write failures via metrics and investigate
///    persistent failures that could indicate L2 capacity or connectivity issues
///
/// ### When L2 Failures Are Acceptable:
/// - L2 is a persistent cache for cold starts (L1 mask inconsistency during normal operation)
/// - Cache data is regeneratable from source of truth
/// - Read-heavy workload where L1 hit rate is very high
#[derive(Debug, Clone, Copy, Default)]
pub struct SequentialWritePolicy;

impl SequentialWritePolicy {
    /// Create a new sequential write policy.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CompositionWritePolicy for SequentialWritePolicy {
    #[tracing::instrument(skip(self, key, write_l1, write_l2, _offload), level = "trace")]
    async fn execute_with<F1, F2, Fut1, Fut2, O>(
        &self,
        key: CacheKey,
        write_l1: F1,
        write_l2: F2,
        _offload: &O,
    ) -> Result<(), BackendError>
    where
        F1: FnOnce(CacheKey) -> Fut1 + Send,
        F2: FnOnce(CacheKey) -> Fut2 + Send,
        Fut1: Future<Output = Result<(), BackendError>> + Send + 'static,
        Fut2: Future<Output = Result<(), BackendError>> + Send + 'static,
        O: Offload,
    {
        // Write to L1 first
        match write_l1(key.clone()).await {
            Ok(()) => {
                tracing::trace!("L1 write succeeded");
            }
            Err(e) => {
                // L1 failed - don't write to L2
                tracing::error!(error = ?e, "L1 write failed");
                return Err(e);
            }
        }

        // Write to L2
        match write_l2(key).await {
            Ok(()) => {
                tracing::trace!("L2 write succeeded");
                Ok(())
            }
            Err(e) => {
                // L2 failed - inconsistent state (L1 has data, L2 doesn't)
                tracing::error!(
                    error = ?e,
                    "L2 write failed after L1 succeeded - inconsistent state"
                );
                Err(e)
            }
        }
    }
}
