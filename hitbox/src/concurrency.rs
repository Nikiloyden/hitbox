use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, broadcast};

use hitbox_core::{CacheValue, CacheableResponse};

use crate::CacheKey;

/// Errors that can occur when waiting for a concurrent request
#[derive(Debug, Clone)]
pub enum ConcurrencyError {
    /// Receiver lagged behind and missed messages
    Lagged(u64),
    /// Broadcast channel closed before receiving value
    Closed,
}

/// Result of concurrency check - whether to proceed with upstream call or await existing response
pub enum ConcurrencyDecision<Res> {
    /// Proceed with the upstream call, holding a semaphore permit
    Proceed(OwnedSemaphorePermit),
    /// Proceed without a permit (no concurrency control)
    ProceedWithoutPermit,
    /// Await response from another in-flight request
    Await(Pin<Box<dyn Future<Output = Result<Res, ConcurrencyError>> + Send>>),
}

/// Trait for managing concurrent requests to prevent dogpile effect
pub trait ConcurrencyManager<Res>: Send + Sync
where
    Res: CacheableResponse,
{
    /// Check if this request should proceed to upstream or await an existing request
    fn check(&self, cache_key: &CacheKey, concurrency: usize) -> ConcurrencyDecision<Res>;

    /// Notify waiting requests that the response is ready and return it back
    fn resolve(&self, cache_key: &CacheKey, cache_value: &CacheValue<Res::Cached>);

    /// Cleanup stale entry from in-flight map (e.g., after channel closed error)
    fn cleanup(&self, cache_key: &CacheKey);
}

/// No-op implementation that always allows requests to proceed without concurrency control
pub struct NoopConcurrencyManager;

impl<Res> ConcurrencyManager<Res> for NoopConcurrencyManager
where
    Res: CacheableResponse + Send + 'static,
{
    fn check(&self, _cache_key: &CacheKey, _concurrency: usize) -> ConcurrencyDecision<Res> {
        ConcurrencyDecision::ProceedWithoutPermit
    }

    fn resolve(&self, _cache_key: &CacheKey, _cache_value: &CacheValue<Res::Cached>) {
        // No-op: nothing to resolve
    }
}

/// Broadcast-based concurrency manager that prevents dogpile effect with semaphore-based concurrency control
///
/// When multiple requests arrive for the same cache key:
/// - First N requests (where N = semaphore capacity) proceed to upstream
/// - Subsequent requests subscribe to the broadcast channel and wait
/// - First request to complete broadcasts the result to all waiters
/// - Waiters reconstruct the response using CacheableResponse::from_cached
pub struct BroadcastConcurrencyManager<Res>
where
    Res: CacheableResponse,
{
    // Maps cache keys to (broadcast sender, semaphore) for in-flight requests
    in_flight: DashMap<
        CacheKey,
        (
            broadcast::Sender<Arc<CacheValue<Res::Cached>>>,
            Arc<Semaphore>,
        ),
    >,
}

impl<Res> BroadcastConcurrencyManager<Res>
where
    Res: CacheableResponse,
{
    pub fn new() -> Self {
        Self {
            in_flight: DashMap::new(),
        }
    }
}

impl<Res> Default for BroadcastConcurrencyManager<Res>
where
    Res: CacheableResponse,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Res> ConcurrencyManager<Res> for BroadcastConcurrencyManager<Res>
where
    Res: CacheableResponse + Send + 'static,
    Res::Cached: Send + Sync + Clone + Debug + 'static,
{
    fn check(&self, cache_key: &CacheKey, concurrency: usize) -> ConcurrencyDecision<Res> {
        // Use entry() API for atomic insert-if-absent to prevent race conditions
        match self.in_flight.entry(cache_key.clone()) {
            Entry::Occupied(entry) => {
                // Entry already exists - try to acquire a permit from the semaphore
                let (sender, semaphore) = entry.get();

                if let Ok(permit) = semaphore.clone().try_acquire_owned() {
                    // Got a permit - this request can proceed to upstream
                    ConcurrencyDecision::Proceed(permit)
                } else {
                    // No permits available - subscribe to broadcast and wait
                    let mut receiver = sender.subscribe();

                    let future = Box::pin(async move {
                        match receiver.recv().await {
                            Ok(cache_value) => {
                                // Successfully received the cached value from the in-flight request
                                // Convert Res::Cached back to Res using the trait method
                                Ok(Res::from_cached(cache_value.data.clone()).await)
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                // We lagged behind and missed the message
                                // Return error so FSM can handle (e.g., retry or fetch from upstream)
                                Err(ConcurrencyError::Lagged(n))
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                // Channel closed without sending - the request probably failed
                                // Return error so FSM can handle (e.g., retry or fetch from upstream)
                                Err(ConcurrencyError::Closed)
                            }
                        }
                    });

                    ConcurrencyDecision::Await(future)
                }
            }
            Entry::Vacant(entry) => {
                // No in-flight request for this key - atomically create and insert entry
                // Create a broadcast channel and semaphore with the specified concurrency limit
                let (sender, _receiver) = broadcast::channel(16);
                let semaphore = Arc::new(Semaphore::new(concurrency));

                // Acquire the first permit
                let permit = semaphore
                    .clone()
                    .try_acquire_owned()
                    .expect("First permit acquisition should never fail");

                // Atomically insert the entry
                entry.insert((sender, semaphore));

                ConcurrencyDecision::Proceed(permit)
            }
        }
    }

    fn resolve(&self, cache_key: &CacheKey, cache_value: &CacheValue<Res::Cached>) {
        // Remove the entry from the map and broadcast the result
        if let Some((_, (sender, _semaphore))) = self.in_flight.remove(cache_key) {
            // Broadcast to all waiting requests
            // Wrap in Arc to avoid expensive clones for each subscriber
            let shared_value = Arc::new(cache_value.clone());
            let _ = sender.send(shared_value);
            // Ignore send errors - it just means no one is waiting
            // Semaphore drops here, releasing any remaining permits
        }
    }

    fn cleanup(&self, cache_key: &CacheKey) {
        // Remove stale entry from in-flight map
        // Called when a waiter encounters an error and needs to ensure cleanup
        self.in_flight.remove(cache_key);
    }
}
