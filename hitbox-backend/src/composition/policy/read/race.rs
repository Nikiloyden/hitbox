//! Race read policy implementation.
//!
//! This policy queries both L1 and L2 simultaneously and returns the first
//! successful hit, minimizing tail latency at the cost of increased backend load.

use async_trait::async_trait;
use futures::future::{select, Either};
use futures::pin_mut;
use hitbox_core::{CacheKey, CacheValue};
use std::future::Future;

use super::ReadPolicy;

/// Race read policy: Query both L1 and L2 simultaneously, return first hit.
///
/// This strategy provides:
/// - Minimal tail latency by racing both backends
/// - Resilience to variable L1 performance
/// - Guaranteed fastest response time
///
/// # Behavior
/// 1. Start both `read_l1(key)` and `read_l2(key)` in parallel
/// 2. Whichever completes first:
///    - If hit (Ok(Some)): Return immediately
///    - If miss/error: Wait for the second backend
/// 3. Aggregate results if neither hit first
///
/// # Tradeoffs
/// - **Pros**: Best latency, especially for P99/P999
/// - **Cons**: 2x backend load (always queries both layers)
///
/// # Use Cases
/// - L1 has variable/unpredictable latency (remote cache)
/// - Tail latency is critical (user-facing APIs)
/// - Backend capacity allows double load
///
/// # Note
/// The closures passed to `execute_with` are responsible for any post-processing
/// like L1 population or envelope wrapping.
#[derive(Debug, Clone, Copy, Default)]
pub struct RaceReadPolicy;

impl RaceReadPolicy {
    /// Create a new race read policy.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ReadPolicy for RaceReadPolicy {
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
        let l1_fut = read_l1(key);
        let l2_fut = read_l2(key);

        // Pin both futures for select
        pin_mut!(l1_fut, l2_fut);

        // Race both futures
        match select(l1_fut, l2_fut).await {
            Either::Left((l1_result, l2_fut)) => {
                // L1 completed first
                if let Ok(Some(value)) = l1_result {
                    tracing::trace!("L1 hit (won race)");
                    return Ok(Some(value));
                }

                // L1 miss/error, wait for L2
                tracing::trace!("L1 completed first without hit, waiting for L2");
                let l2_result = l2_fut.await;

                // Aggregate results
                match (l1_result, l2_result) {
                    (Ok(Some(value)), _) | (_, Ok(Some(value))) => {
                        tracing::trace!("Cache hit");
                        Ok(Some(value))
                    }
                    (Ok(None), Ok(None)) => {
                        tracing::trace!("Both layers miss");
                        Ok(None)
                    }
                    (Err(e1), Err(e2)) => {
                        tracing::error!(l1_error = ?e1, l2_error = ?e2, "Both layers failed");
                        Err(e2)
                    }
                    (Ok(None), Err(e)) | (Err(e), Ok(None)) => {
                        tracing::warn!(error = ?e, "One layer failed, one missed");
                        Ok(None)
                    }
                }
            }
            Either::Right((l2_result, l1_fut)) => {
                // L2 completed first
                if let Ok(Some(value)) = l2_result {
                    tracing::trace!("L2 hit (won race)");
                    return Ok(Some(value));
                }

                // L2 miss/error, wait for L1
                tracing::trace!("L2 completed first without hit, waiting for L1");
                let l1_result = l1_fut.await;

                // Aggregate results
                match (l1_result, l2_result) {
                    (Ok(Some(value)), _) | (_, Ok(Some(value))) => {
                        tracing::trace!("Cache hit");
                        Ok(Some(value))
                    }
                    (Ok(None), Ok(None)) => {
                        tracing::trace!("Both layers miss");
                        Ok(None)
                    }
                    (Err(e1), Err(e2)) => {
                        tracing::error!(l1_error = ?e1, l2_error = ?e2, "Both layers failed");
                        Err(e2)
                    }
                    (Ok(None), Err(e)) | (Err(e), Ok(None)) => {
                        tracing::warn!(error = ?e, "One layer failed, one missed");
                        Ok(None)
                    }
                }
            }
        }
    }
}
