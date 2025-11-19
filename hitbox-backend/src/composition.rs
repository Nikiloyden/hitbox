//! Composition backend that provides multi-tier caching by combining two backends.
//!
//! This backend implements a layered caching strategy where:
//! - L1 (first layer): Typically a fast local cache (e.g., Moka)
//! - L2 (second layer): Typically a distributed cache (e.g., Redis)
//!
//! # Read Strategy
//! 1. Check L1 → Hit: return value
//! 2. Check L2 → Hit: populate L1, return value
//! 3. Miss: return None (upstream will be called, then set() populates both layers)
//!
//! # Write Strategy
//! - Write-through: Writes to both L1 and L2
//! - L1 is written first for fast local access
//! - If either write fails, logs warning but continues (best-effort)
//!
//! # Delete Strategy
//! - Deletes from both layers
//! - Returns success if at least one deletion succeeds
//!
//! # Example
//! ```ignore
//! use hitbox_backend::CompositionBackend;
//! use hitbox_moka::MokaBackend;
//! use hitbox_redis::RedisBackend;
//!
//! let moka = MokaBackend::builder(1000).build();
//! let redis = RedisBackend::new(client);
//! let backend = CompositionBackend::new(moka, redis);
//! ```

use crate::serializer::{Format, FormatError};
use crate::{
    Backend, BackendError, BackendResult, CacheBackend, CacheKeyFormat, Compressor, DeleteStatus,
    PassthroughCompressor,
};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use hitbox_core::{CacheKey, CacheValue, CacheableResponse, Raw};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;
use thiserror::Error;

/// Error type for composition backend operations.
///
/// This error type preserves errors from both cache layers for debugging,
/// while keeping the implementation details encapsulated.
#[derive(Debug, Error)]
enum CompositionError {
    /// Both L1 and L2 cache layers failed.
    #[error("Both cache layers failed - L1: {l1}, L2: {l2}")]
    BothLayersFailed {
        l1: BackendError,
        l2: BackendError,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CacheValueData {
    pub data: Raw,
    pub expire: Option<DateTime<Utc>>,
    pub stale: Option<DateTime<Utc>>,
}

impl CacheValueData {
    pub fn new(data: Raw) -> Self {
        Self {
            data,
            expire: None,
            stale: None,
        }
    }
}

impl From<CacheValue<Raw>> for CacheValueData {
    fn from(value: CacheValue<Raw>) -> Self {
        CacheValueData {
            data: value.data,
            expire: value.expire,
            stale: value.stale,
        }
    }
}

impl From<CacheValueData> for CacheValue<Raw> {
    fn from(data: CacheValueData) -> Self {
        CacheValue::new(data.data, data.expire, data.stale)
    }
}

/// Envelope for multi-layer cache data.
/// - Read operations can return L1, L2, or Both variants
/// - Write operations via CacheBackend::set always create Both variant
/// - Defensive code handles all variants in write() for edge cases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum CompositionEnvelope {
    L1(CacheValueData),
    L2(CacheValueData),
    Both {
        l1: CacheValueData,
        l2: CacheValueData,
    },
}

/// Format implementation for CompositionBackend that handles multi-layer serialization.
///
/// This format serializes data in both formats and packs them together into a CompositionEnvelope.
/// On deserialization, it unpacks the CompositionEnvelope and deserializes from L1 if available, otherwise L2.
#[derive(Debug, Clone)]
pub struct CompositionFormat {
    l1_format: Box<dyn Format>,
    l2_format: Box<dyn Format>,
}

impl CompositionFormat {
    pub fn new(l1_format: Box<dyn Format>, l2_format: Box<dyn Format>) -> Self {
        CompositionFormat {
            l1_format,
            l2_format,
        }
    }

    /// Check if L1 and L2 formats are the same type.
    /// Returns true if both formats have the same FormatTypeId.
    fn same_format(&self) -> bool {
        self.l1_format.format_type_id() == self.l2_format.format_type_id()
    }
}

impl Format for CompositionFormat {
    fn erased_serialize(&self, value: &dyn erased_serde::Serialize) -> Result<Raw, FormatError> {
        // Serialize the value in L1 format
        let l1_serialized = self.l1_format.erased_serialize(value)?;

        // If L1 and L2 use the same format, reuse the serialized data instead of serializing again
        let l2_serialized = if self.same_format() {
            l1_serialized.clone()
        } else {
            self.l2_format.erased_serialize(value)?
        };

        // Pack both serialized values into CompositionEnvelope
        let composition = CompositionEnvelope::Both {
            l1: CacheValueData::new(l1_serialized),
            l2: CacheValueData::new(l2_serialized),
        };

        // Serialize the CompositionEnvelope itself using bitcode for better performance
        bitcode::serialize(&composition)
            .map(Bytes::from)
            .map_err(|e| FormatError::Serialize(Box::new(e)))
    }

    fn with_deserializer(
        &self,
        data: &[u8],
        f: &mut dyn FnMut(&mut dyn erased_serde::Deserializer) -> Result<(), erased_serde::Error>,
    ) -> Result<(), FormatError> {
        // Deserialize the CompositionEnvelope using bitcode
        let composition: CompositionEnvelope = bitcode::deserialize(data)
            .map_err(|e| FormatError::Deserialize(Box::new(e)))?;

        // Get data from L1 if available, otherwise L2, and use the corresponding format
        let (layer_data, format): (&Bytes, &dyn Format) = match &composition {
            CompositionEnvelope::L1(v) => (&v.data, &self.l1_format),
            CompositionEnvelope::L2(v) => (&v.data, &self.l2_format),
            CompositionEnvelope::Both { l1, .. } => (&l1.data, &self.l1_format),
        };

        // Use the appropriate format to deserialize
        format.with_deserializer(layer_data.as_ref(), f)
    }

    fn clone_box(&self) -> Box<dyn Format> {
        Box::new(self.clone())
    }

    fn format_type_id(&self) -> crate::serializer::FormatTypeId {
        // CompositionFormat is a custom format
        crate::serializer::FormatTypeId::Custom("composition")
    }
}

/// A backend that composes two cache backends into a layered caching system.
///
/// The first backend (L1) is checked first on reads, and if not found,
/// the second backend (L2) is checked. On writes, both backends are updated.
///
/// Each layer can use its own serialization format and compression since
/// `CacheBackend` operates on typed data, not raw bytes.
pub struct CompositionBackend<L1, L2>
where
    L1: Backend,
    L2: Backend,
{
    /// First-layer cache (typically fast, local)
    l1: L1,
    /// Second-layer cache (typically distributed, persistent)
    l2: L2,
    /// Composition format
    format: CompositionFormat,
}

impl<L1, L2> CompositionBackend<L1, L2>
where
    L1: Backend,
    L2: Backend,
{
    /// Creates a new composition backend with two layers.
    ///
    /// # Arguments
    /// * `l1` - First-layer backend (checked first on reads)
    /// * `l2` - Second-layer backend (checked if L1 misses)
    pub fn new(l1: L1, l2: L2) -> Self {
        let format =
            CompositionFormat::new(l1.value_format().clone_box(), l2.value_format().clone_box());
        Self { l1, l2, format }
    }
}

impl<L1, L2> Clone for CompositionBackend<L1, L2>
where
    L1: Clone + Backend,
    L2: Clone + Backend,
{
    fn clone(&self) -> Self {
        Self {
            l1: self.l1.clone(),
            l2: self.l2.clone(),
            format: self.format.clone(),
        }
    }
}

impl<L1, L2> std::fmt::Debug for CompositionBackend<L1, L2>
where
    L1: std::fmt::Debug + Backend,
    L2: std::fmt::Debug + Backend,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositionBackend")
            .field("l1", &self.l1)
            .field("l2", &self.l2)
            .field("format", &self.format)
            .finish()
    }
}

// Backend implementation for CompositionBackend
// This implementation packs/unpacks CompositionEnvelope to enable
// use as Box<dyn Backend> trait object
//
// PERFORMANCE NOTE: Negligible overhead - only metadata (expire/stale timestamps + envelope
// discriminant) is serialized using bitcode. The already-serialized cached data (Bytes) is
// copied into the buffer as-is without re-serialization. When using CompositionBackend
// directly via CacheBackend::get/set, even this minimal envelope overhead is avoided.
#[async_trait]
impl<L1, L2> Backend for CompositionBackend<L1, L2>
where
    L1: Backend + Send + Sync,
    L2: Backend + Send + Sync,
{
    #[tracing::instrument(skip(self), level = "trace")]
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        // Try L1 first
        if let Some(l1_value) = self.l1.read(key).await? {
            let expire = l1_value.expire;
            let stale = l1_value.stale;
            let envelope = CompositionEnvelope::L1(l1_value.into());
            // Use bitcode for faster serialization (matches CacheKey format)
            let packed = bitcode::serialize(&envelope)
                .map(Bytes::from)
                .map_err(|e| BackendError::InternalError(Box::new(e)))?;
            return Ok(Some(CacheValue::new(packed, expire, stale)));
        }

        // Try L2 if L1 miss
        if let Some(l2_value) = self.l2.read(key).await? {
            let expire = l2_value.expire;
            let stale = l2_value.stale;
            let envelope = CompositionEnvelope::L2(l2_value.into());
            // Use bitcode for faster serialization (matches CacheKey format)
            let packed = bitcode::serialize(&envelope)
                .map(Bytes::from)
                .map_err(|e| BackendError::InternalError(Box::new(e)))?;
            return Ok(Some(CacheValue::new(packed, expire, stale)));
        }

        // Both miss
        Ok(None)
    }

    #[tracing::instrument(skip(self, value), level = "trace")]
    async fn write(
        &self,
        key: &CacheKey,
        value: CacheValue<Raw>,
        ttl: Option<Duration>,
    ) -> BackendResult<()> {
        // Unpack CompositionEnvelope using bitcode
        let composition: CompositionEnvelope = bitcode::deserialize(&value.data)
            .map_err(|e| BackendError::InternalError(Box::new(e)))?;

        // Write to appropriate layers
        // In normal usage via CacheBackend::set, this is always Both variant
        // The L1/L2 branches are defensive code for edge cases
        match composition {
            CompositionEnvelope::Both { l1, l2 } => {
                // Write to both layers in parallel for better performance
                let (l1_result, l2_result) = futures::join!(
                    self.l1.write(key, l1.into(), ttl),
                    self.l2.write(key, l2.into(), ttl)
                );

                // Return error if both fail
                match (l1_result, l2_result) {
                    (Err(e1), Err(e2)) => {
                        tracing::error!(l1_error = ?e1, l2_error = ?e2, "Both L1 and L2 write failed");
                        Err(BackendError::InternalError(Box::new(
                            CompositionError::BothLayersFailed { l1: e1, l2: e2 }
                        )))
                    }
                    (Err(e), Ok(())) => {
                        tracing::warn!(error = ?e, "L1 write failed");
                        Ok(())
                    }
                    (Ok(()), Err(e)) => {
                        tracing::warn!(error = ?e, "L2 write failed");
                        Ok(())
                    }
                    (Ok(()), Ok(())) => Ok(()),
                }
            }
            CompositionEnvelope::L1(l1) => self.l1.write(key, l1.into(), ttl).await,
            CompositionEnvelope::L2(l2) => self.l2.write(key, l2.into(), ttl).await,
        }
    }

    #[tracing::instrument(skip(self), level = "trace")]
    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        // Delete from both layers in parallel for better performance
        let (l1_result, l2_result) = futures::join!(
            self.l1.remove(key),
            self.l2.remove(key)
        );

        match (l1_result, l2_result) {
            (Err(e1), Err(e2)) => {
                tracing::error!(l1_error = ?e1, l2_error = ?e2, "Both L1 and L2 delete failed");
                Err(BackendError::InternalError(Box::new(
                    CompositionError::BothLayersFailed { l1: e1, l2: e2 }
                )))
            }
            (Err(e), Ok(status)) => {
                tracing::warn!(error = ?e, "L1 delete failed");
                Ok(status)
            }
            (Ok(status), Err(e)) => {
                tracing::warn!(error = ?e, "L2 delete failed");
                Ok(status)
            }
            (Ok(DeleteStatus::Deleted(n1)), Ok(DeleteStatus::Deleted(n2))) => {
                Ok(DeleteStatus::Deleted(n1 + n2))
            }
            (Ok(DeleteStatus::Deleted(n)), Ok(DeleteStatus::Missing))
            | (Ok(DeleteStatus::Missing), Ok(DeleteStatus::Deleted(n))) => {
                Ok(DeleteStatus::Deleted(n))
            }
            (Ok(DeleteStatus::Missing), Ok(DeleteStatus::Missing)) => Ok(DeleteStatus::Missing),
        }
    }

    fn value_format(&self) -> &dyn Format {
        &self.format
    }

    fn key_format(&self) -> &CacheKeyFormat {
        &CacheKeyFormat::Bitcode
    }

    fn compressor(&self) -> &dyn Compressor {
        &PassthroughCompressor
    }
}

impl<L1, L2> CacheBackend for CompositionBackend<L1, L2>
where
    L1: CacheBackend + Send + Sync,
    L2: CacheBackend + Send + Sync,
{
    #[tracing::instrument(skip(self), level = "trace")]
    async fn get<T>(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<T::Cached>>>
    where
        T: CacheableResponse,
        T::Cached: Serialize + DeserializeOwned + Send + Sync,
    {
        // Try L1 first
        match self.l1.get::<T>(key).await {
            Ok(Some(value)) => {
                // L1 hit - return immediately
                tracing::trace!("L1 hit");
                return Ok(Some(value));
            }
            Ok(None) => {
                // L1 miss - continue to L2
                tracing::trace!("L1 miss");
            }
            Err(e) => {
                // L1 error - log and continue to L2
                tracing::warn!(error = ?e, "L1 get failed");
            }
        }

        // Try L2
        match self.l2.get::<T>(key).await {
            Ok(Some(value)) => {
                // L2 hit - populate L1 for future fast access
                tracing::trace!("L2 hit, populating L1");

                // Use the value's ttl() method to get remaining time
                let ttl = value.ttl();

                // Populate L1 (best-effort, don't fail the read if this fails)
                if let Err(e) = self.l1.set::<T>(key, &value, ttl).await {
                    tracing::warn!(error = ?e, "Failed to populate L1 from L2");
                }

                Ok(Some(value))
            }
            Ok(None) => {
                // L2 miss
                tracing::trace!("L2 miss");
                Ok(None)
            }
            Err(e) => {
                // L2 error
                tracing::error!(error = ?e, "L2 get failed");
                Err(e)
            }
        }
    }

    #[tracing::instrument(skip(self, value), level = "trace")]
    async fn set<T>(
        &self,
        key: &CacheKey,
        value: &CacheValue<T::Cached>,
        ttl: Option<Duration>,
    ) -> BackendResult<()>
    where
        T: CacheableResponse,
        T::Cached: Serialize + Send + Sync,
    {
        // Write to both layers in parallel for better performance
        let (l1_result, l2_result) = futures::join!(
            self.l1.set::<T>(key, value, ttl),
            self.l2.set::<T>(key, value, ttl)
        );

        // Success if at least one succeeds
        match (l1_result, l2_result) {
            (Err(e1), Err(e2)) => {
                tracing::error!(l1_error = ?e1, l2_error = ?e2, "Both L1 and L2 set failed");
                Err(BackendError::InternalError(Box::new(
                    CompositionError::BothLayersFailed { l1: e1, l2: e2 }
                )))
            }
            (Err(e), Ok(())) => {
                tracing::warn!(error = ?e, "L1 set failed");
                Ok(()) // L2 succeeded
            }
            (Ok(()), Err(e)) => {
                tracing::warn!(error = ?e, "L2 set failed");
                Ok(()) // L1 succeeded
            }
            (Ok(()), Ok(())) => {
                tracing::trace!("Successfully set in both L1 and L2");
                Ok(())
            }
        }
    }

    #[tracing::instrument(skip(self), level = "trace")]
    async fn delete(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        // Delete from both layers in parallel for better performance
        let (l1_result, l2_result) = futures::join!(
            self.l1.delete(key),
            self.l2.delete(key)
        );

        // Aggregate results
        match (l1_result, l2_result) {
            (Err(e1), Err(e2)) => {
                tracing::error!(l1_error = ?e1, l2_error = ?e2, "Both L1 and L2 delete failed");
                Err(BackendError::InternalError(Box::new(
                    CompositionError::BothLayersFailed { l1: e1, l2: e2 }
                )))
            }
            (Err(e), Ok(status)) => {
                tracing::warn!(error = ?e, "L1 delete failed");
                Ok(status)
            }
            (Ok(status), Err(e)) => {
                tracing::warn!(error = ?e, "L2 delete failed");
                Ok(status)
            }
            (Ok(DeleteStatus::Deleted(n1)), Ok(DeleteStatus::Deleted(n2))) => {
                tracing::trace!("Deleted from both L1 and L2");
                Ok(DeleteStatus::Deleted(n1 + n2))
            }
            (Ok(DeleteStatus::Deleted(n)), Ok(DeleteStatus::Missing))
            | (Ok(DeleteStatus::Missing), Ok(DeleteStatus::Deleted(n))) => {
                tracing::trace!("Deleted from one layer");
                Ok(DeleteStatus::Deleted(n))
            }
            (Ok(DeleteStatus::Missing), Ok(DeleteStatus::Missing)) => {
                tracing::trace!("Key missing from both layers");
                Ok(DeleteStatus::Missing)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serializer::{Format, JsonFormat};
    use crate::{Backend, CacheKeyFormat, Compressor, PassthroughCompressor};
    use async_trait::async_trait;
    use chrono::Utc;
    use hitbox_core::{CachePolicy, CacheValue, CacheableResponse, EntityPolicyConfig, Predicate, Raw};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    // Simple in-memory backend for testing
    #[derive(Clone, Debug)]
    struct TestBackend {
        store: Arc<Mutex<HashMap<CacheKey, CacheValue<Raw>>>>,
    }

    impl TestBackend {
        fn new() -> Self {
            Self {
                store: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait]
    impl Backend for TestBackend {
        async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
            Ok(self.store.lock().unwrap().get(key).cloned())
        }

        async fn write(
            &self,
            key: &CacheKey,
            value: CacheValue<Raw>,
            _ttl: Option<Duration>,
        ) -> BackendResult<()> {
            self.store.lock().unwrap().insert(key.clone(), value);
            Ok(())
        }

        async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
            match self.store.lock().unwrap().remove(key) {
                Some(_) => Ok(DeleteStatus::Deleted(1)),
                None => Ok(DeleteStatus::Missing),
            }
        }

        fn value_format(&self) -> &dyn Format {
            &JsonFormat
        }

        fn key_format(&self) -> &CacheKeyFormat {
            &CacheKeyFormat::Bitcode
        }

        fn compressor(&self) -> &dyn Compressor {
            &PassthroughCompressor
        }
    }

    impl CacheBackend for TestBackend {}

    // Mock CacheableResponse for testing
    // We only need the associated type, the actual methods are not used in these tests
    struct MockResponse;

    // Note: This is a minimal implementation just for testing CacheBackend.
    // The methods are not actually called in these tests.
    #[async_trait]
    impl CacheableResponse for MockResponse {
        type Cached = String;
        type Subject = MockResponse;

        async fn cache_policy<P: Predicate<Subject = Self::Subject> + Send + Sync>(
            self,
            _predicate: P,
            _config: &EntityPolicyConfig,
        ) -> CachePolicy<CacheValue<Self::Cached>, Self> {
            unimplemented!("Not used in these tests")
        }

        async fn into_cached(self) -> CachePolicy<Self::Cached, Self> {
            unimplemented!("Not used in these tests")
        }

        async fn from_cached(_cached: Self::Cached) -> Self {
            unimplemented!("Not used in these tests")
        }
    }

    #[tokio::test]
    async fn test_l1_hit() {
        let l1 = TestBackend::new();
        let l2 = TestBackend::new();
        let backend = CompositionBackend::new(l1.clone(), l2);

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            "value1".to_string(),
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write to populate both layers
        backend
            .set::<MockResponse>(&key, &value, Some(Duration::from_secs(60)))
            .await
            .unwrap();

        // Read should hit L1
        let result = backend.get::<MockResponse>(&key).await.unwrap();
        assert_eq!(result.unwrap().data, "value1");
    }

    #[tokio::test]
    async fn test_l2_hit_populates_l1() {
        let l1 = TestBackend::new();
        let l2 = TestBackend::new();

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            "value1".to_string(),
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write only to L2
        l2.set::<MockResponse>(&key, &value, Some(Duration::from_secs(60)))
            .await
            .unwrap();

        let backend = CompositionBackend::new(l1.clone(), l2);

        // First read should hit L2 and populate L1
        let result = backend.get::<MockResponse>(&key).await.unwrap();
        assert_eq!(result.unwrap().data, "value1");

        // Verify L1 was populated from L2 (cache warming)
        let l1_result = l1.get::<MockResponse>(&key).await.unwrap();
        assert!(l1_result.is_some(), "L1 should be populated from L2 hit");
        assert_eq!(l1_result.unwrap().data, "value1");
    }

    #[tokio::test]
    async fn test_miss_both_layers() {
        let l1 = TestBackend::new();
        let l2 = TestBackend::new();
        let backend = CompositionBackend::new(l1, l2);

        let key = CacheKey::from_str("test", "nonexistent");

        let result = backend.get::<MockResponse>(&key).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_write_to_both_layers() {
        let l1 = TestBackend::new();
        let l2 = TestBackend::new();

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            "value1".to_string(),
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        let backend = CompositionBackend::new(l1.clone(), l2.clone());

        backend
            .set::<MockResponse>(&key, &value, Some(Duration::from_secs(60)))
            .await
            .unwrap();

        // Verify both layers have the value
        let l1_result = l1.get::<MockResponse>(&key).await.unwrap();
        assert_eq!(l1_result.unwrap().data, "value1");

        let l2_result = l2.get::<MockResponse>(&key).await.unwrap();
        assert_eq!(l2_result.unwrap().data, "value1");
    }

    #[tokio::test]
    async fn test_delete_from_both_layers() {
        let l1 = TestBackend::new();
        let l2 = TestBackend::new();

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            "value1".to_string(),
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        let backend = CompositionBackend::new(l1.clone(), l2.clone());

        // Write to both
        backend
            .set::<MockResponse>(&key, &value, Some(Duration::from_secs(60)))
            .await
            .unwrap();

        // Delete from both
        let status = backend.delete(&key).await.unwrap();
        assert_eq!(status, DeleteStatus::Deleted(2));

        // Verify both layers no longer have the value
        let l1_result = l1.get::<MockResponse>(&key).await.unwrap();
        assert!(l1_result.is_none());

        let l2_result = l2.get::<MockResponse>(&key).await.unwrap();
        assert!(l2_result.is_none());
    }

    #[tokio::test]
    async fn test_clone() {
        let l1 = TestBackend::new();
        let l2 = TestBackend::new();
        let backend = CompositionBackend::new(l1, l2);

        let cloned = backend.clone();

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            "value1".to_string(),
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write via original
        backend
            .set::<MockResponse>(&key, &value, Some(Duration::from_secs(60)))
            .await
            .unwrap();

        // Read via clone should work (shared backends)
        let result = cloned.get::<MockResponse>(&key).await.unwrap();
        assert_eq!(result.unwrap().data, "value1");
    }
}
