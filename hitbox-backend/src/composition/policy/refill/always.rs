//! Always refill policy implementation.
//!
//! This policy always refills L1 when L2 has a hit, which is the classic
//! cache hierarchy behavior that maximizes L1 hit rate.

use async_trait::async_trait;
use hitbox_core::CacheValue;
use std::future::Future;

use super::RefillPolicy;
use crate::BackendError;

/// Always refill policy: Always populate L1 after L2 hit.
///
/// This is the default and most common refill strategy. It provides:
/// - Maximum L1 hit rate over time
/// - Classic cache hierarchy behavior
/// - Simple and predictable
///
/// # Behavior
/// Always returns `true` from `should_refill()`, causing L1 to be populated
/// with every value read from L2.
///
/// # Characteristics
/// - **L1 hit rate:** High (L1 eventually contains all accessed data)
/// - **L2 load:** Low (hot data moves to L1)
/// - **Latency:** Medium (first read has refill overhead)
/// - **L1 utilization:** High (all read data cached in L1)
///
/// # Tradeoffs
/// - **Pros:** Simple, maximizes L1 performance, reduces L2 load
/// - **Cons:** May evict useful L1 entries with infrequently accessed data
///
/// # Use Cases
/// - General-purpose caching (default choice)
/// - Read-heavy workloads
/// - When L1 has sufficient capacity
/// - When L1 writes are cheap (in-memory L1)
/// - Predictable access patterns
///
/// # Example
/// ```ignore
/// use hitbox_backend::CompositionBackend;
/// use hitbox_backend::composition::policy::AlwaysRefill;
///
/// let backend = CompositionBackend::new(l1, l2)
///     .with_refill_policy(AlwaysRefill::new());
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct AlwaysRefill;

impl AlwaysRefill {
    /// Create a new always refill policy.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RefillPolicy for AlwaysRefill {
    async fn execute<'a, T, F, Fut>(&self, _value: &'a CacheValue<T>, refill_fn: F) -> ()
    where
        T: Send + Sync,
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<(), BackendError>> + Send + 'a,
    {
        // Always execute refill (best-effort)
        if let Err(e) = refill_fn().await {
            tracing::warn!(error = ?e, "Failed to refill L1 from L2");
        }
    }
}
