//! Refill policies for controlling L1 population after L2 hits.
//!
//! This module defines the CompositionRefillPolicy trait and its implementations.
//! Different strategies (always, never, conditional) can be used to optimize
//! L1 cache utilization based on access patterns and workload characteristics.

use async_trait::async_trait;
use hitbox_core::CacheValue;
use std::future::Future;

use crate::BackendError;

pub mod always;
pub mod never;

pub use always::AlwaysRefill;
pub use never::NeverRefill;

/// Policy trait for controlling L1 refill after L2 hits.
///
/// When a read misses L1 but hits L2, the refill policy determines whether
/// and how to populate L1 with the value from L2. This is also known as
/// "cache promotion" or "backfilling."
///
/// # Motivation
///
/// Not all L2 hits should populate L1:
/// - L1 may have limited capacity (don't evict hot entries for cold data)
/// - Some data is expensive to serialize/deserialize
/// - Short-lived data may not benefit from L1 caching
/// - Write-only L1 patterns (L1 for recent writes, L2 for reads)
///
/// # Design
///
/// The policy receives the value and a refill closure. It decides whether to
/// execute the refill and controls how the refill is executed (synchronously,
/// asynchronously, with retries, etc.).
///
/// This design is consistent with `CompositionReadPolicy` and `CompositionWritePolicy`, and allows
/// policies to control execution strategy, not just make boolean decisions.
///
/// # Error Handling
///
/// Refill is **best-effort**. Policies should not propagate errors from failed
/// refills. The read operation succeeds as long as L2 returns the value.
///
/// # Example
///
/// ```ignore
/// use hitbox_backend::composition::policy::CompositionRefillPolicy;
///
/// let policy = AlwaysRefill::default();
///
/// // After L2 hit - metrics are recorded directly in ctx
/// policy.execute(
///     &value,
///     || async { l1.set(key, &value, ttl, &mut ctx).await }
/// ).await;
/// ```
#[async_trait]
pub trait CompositionRefillPolicy: Send + Sync {
    /// Execute refill operation according to policy strategy.
    ///
    /// The policy controls whether and how the refill operation is executed.
    /// Different policies can:
    /// - Skip refill entirely (NeverRefill)
    /// - Execute refill synchronously (AlwaysRefill)
    /// - Execute refill in background (AsyncRefill - future)
    /// - Execute with retry logic (RetryRefill - future)
    /// - Execute conditionally (ConditionalRefill - future)
    ///
    /// # Arguments
    /// * `value` - The value read from L2
    /// * `refill_fn` - Closure that performs the refill (writes to L1)
    ///
    /// # Error Handling
    /// Policies should catch and log refill errors, not propagate them.
    /// Refill failure should not cause the read operation to fail.
    ///
    /// # Note
    /// This method is called on the read path. Policies should minimize overhead
    /// for cases where refill is skipped. Expensive logic should use caching.
    async fn execute<'a, T, F, Fut>(&self, value: &'a CacheValue<T>, refill_fn: F)
    where
        T: Send + Sync,
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<(), BackendError>> + Send + 'a;
}
