//! Never refill policy implementation.
//!
//! This policy never refills L1 from L2 hits, treating L1 as write-only
//! and L2 as the read-through cache.

use async_trait::async_trait;
use hitbox_core::CacheValue;
use std::future::Future;

use super::RefillPolicy;
use crate::BackendError;

/// Never refill policy: Never populate L1 after L2 hit.
///
/// This strategy treats L1 as write-only and L2 as read-through. L1 only
/// contains data that was explicitly written, not data read from L2.
///
/// # Behavior
/// Always returns `false` from `should_refill()`, preventing L1 population
/// from L2 hits.
///
/// # Characteristics
/// - **L1 hit rate:** Low (only written data in L1)
/// - **L2 load:** High (all reads go to L2)
/// - **Latency:** Low (no refill overhead)
/// - **L1 utilization:** Low (only recent writes)
///
/// # Tradeoffs
/// - **Pros:** No refill overhead, predictable L1 contents, saves L1 capacity
/// - **Cons:** Lower L1 hit rate, higher L2 load, potentially higher read latency
///
/// # Use Cases
/// - L1 optimized for recent writes only
/// - L2 is fast enough for reads (e.g., local Redis)
/// - L1 has very limited capacity (e.g., 100 entries)
/// - Strict control over L1 contents required
/// - Cost optimization (L1 writes are expensive, e.g., replicated memory)
/// - Write-heavy workloads with rare re-reads
///
/// # Example
/// ```ignore
/// use hitbox_backend::CompositionBackend;
/// use hitbox_backend::composition::policy::NeverRefill;
///
/// // L1 for recent writes only, L2 for everything else
/// let backend = CompositionBackend::new(small_l1, large_l2)
///     .with_refill_policy(NeverRefill::new());
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct NeverRefill;

impl NeverRefill {
    /// Create a new never refill policy.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RefillPolicy for NeverRefill {
    async fn execute<'a, T, F, Fut>(&self, _value: &'a CacheValue<T>, _refill_fn: F) -> ()
    where
        T: Send + Sync,
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<(), BackendError>> + Send + 'a,
    {
        // Never execute refill - do nothing
        tracing::trace!("NeverRefill policy: skipping L1 refill");
    }
}
