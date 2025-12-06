use chrono::{DateTime, Utc};
use std::time::Duration;

use crate::response::CacheState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheValue<T> {
    pub data: T,
    pub stale: Option<DateTime<Utc>>,
    pub expire: Option<DateTime<Utc>>,
}

impl<T> CacheValue<T> {
    pub fn new(data: T, expire: Option<DateTime<Utc>>, stale: Option<DateTime<Utc>>) -> Self {
        CacheValue {
            data,
            expire,
            stale,
        }
    }

    pub fn into_inner(self) -> T {
        self.data
    }

    pub fn into_parts(self) -> (CacheMeta, T) {
        (CacheMeta::new(self.expire, self.stale), self.data)
    }

    /// Calculate TTL (time-to-live) from the expire time.
    ///
    /// Returns `Some(Duration)` if there's a valid expire time in the future,
    /// or `None` if there's no expire time or it's already expired.
    pub fn ttl(&self) -> Option<Duration> {
        self.expire.and_then(|expire| {
            let duration = expire.signed_duration_since(Utc::now());
            if duration.num_seconds() > 0 {
                Some(Duration::from_secs(duration.num_seconds() as u64))
            } else {
                None
            }
        })
    }
}

impl<T> CacheValue<T> {
    /// Check the cache state based on expire/stale timestamps.
    ///
    /// Returns `CacheState<CacheValue<T>>` preserving the original value with metadata.
    /// This is a sync operation - just checks timestamps, no conversion.
    ///
    /// The caller is responsible for converting to Response via `from_cached()` when needed.
    pub fn cache_state(self) -> CacheState<Self> {
        let now = Utc::now();
        if let Some(expire) = self.expire
            && expire <= now
        {
            CacheState::Expired(self)
        } else if let Some(stale) = self.stale
            && stale <= now
        {
            CacheState::Stale(self)
        } else {
            CacheState::Actual(self)
        }
    }
}

pub struct CacheMeta {
    pub expire: Option<DateTime<Utc>>,
    pub stale: Option<DateTime<Utc>>,
}

impl CacheMeta {
    pub fn new(expire: Option<DateTime<Utc>>, stale: Option<DateTime<Utc>>) -> CacheMeta {
        CacheMeta { expire, stale }
    }
}
