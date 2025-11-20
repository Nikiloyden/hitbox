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
//!
//! # Using the Compose Trait
//! ```ignore
//! use hitbox_backend::composition::Compose;
//!
//! // Fluent API for composition
//! let cache = moka.compose(redis);
//! ```

pub mod compose;
pub mod context;
pub mod policy;

pub use compose::Compose;
pub use context::{CompositionContext, CompositionSource};
pub use policy::CompositionPolicy;

use crate::serializer::{Format, FormatError, FormatTypeId};
use crate::{
    Backend, BackendContext, BackendError, BackendResult, BackendValue, CacheBackend,
    CacheKeyFormat, Compressor, DeleteStatus, PassthroughCompressor,
};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use hitbox_core::{CacheKey, CacheValue, CacheableResponse, Raw};
use policy::{
    AlwaysRefill, OptimisticParallelWritePolicy, ReadPolicy, RefillPolicy, SequentialReadPolicy,
    WritePolicy,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// Error type for composition backend operations.
///
/// This error type preserves errors from both cache layers for debugging,
/// while keeping the implementation details encapsulated.
#[derive(Debug, Error)]
pub enum CompositionError {
    /// Both L1 and L2 cache layers failed.
    #[error("Both cache layers failed - L1: {l1}, L2: {l2}")]
    BothLayersFailed {
        /// Error from L1 layer
        l1: BackendError,
        /// Error from L2 layer
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
///
/// This enum encapsulates data from one or both cache layers when using
/// CompositionBackend as a trait object (`Box<dyn Backend>`).
///
/// # Variants Usage
///
/// - **`Both`**: Normal case created by `CacheBackend::set()`. Contains data
///   serialized in both L1 and L2 formats for optimal performance.
///
/// - **`L1`**: Created by `Backend::read()` when data exists only in L1.
///   This occurs during read operations through the trait object interface
///   when L1 has data but L2 doesn't.
///
/// - **`L2`**: Created by `Backend::read()` when data exists only in L2.
///   This occurs during read operations through the trait object interface
///   when L2 has data but L1 doesn't (before refill).
///
/// # Performance Note
///
/// When using `CompositionBackend` directly via `CacheBackend::get/set`,
/// the envelope wrapping is bypassed entirely for better performance.
/// The envelope is only used when CompositionBackend is accessed through
/// the `Backend` trait (e.g., `Box<dyn Backend>`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum CompositionEnvelope {
    /// Data from L1 only (created during Backend::read from L1 hit)
    L1(CacheValueData),
    /// Data from L2 only (created during Backend::read from L2 hit)
    L2(CacheValueData),
    /// Data from both layers (created by CacheBackend::set - normal case)
    Both {
        l1: CacheValueData,
        l2: CacheValueData,
    },
}

impl CompositionEnvelope {
    /// Serialize the envelope using bitcode and wrap in a CacheValue.
    fn serialize_to_cache_value(
        self,
        expire: Option<DateTime<Utc>>,
        stale: Option<DateTime<Utc>>,
    ) -> BackendResult<CacheValue<Raw>> {
        let packed = bitcode::serialize(&self)
            .map(Bytes::from)
            .map_err(|e| BackendError::InternalError(Box::new(e)))?;
        Ok(CacheValue::new(packed, expire, stale))
    }
}

/// Format implementation for CompositionBackend that handles multi-layer serialization.
///
/// This format serializes data in both formats and packs them together into a CompositionEnvelope.
/// On deserialization, it unpacks the CompositionEnvelope and deserializes from L1 if available, otherwise L2.
#[derive(Debug, Clone)]
pub struct CompositionFormat {
    l1_format: Arc<dyn Format>,
    l2_format: Arc<dyn Format>,
}

impl CompositionFormat {
    pub fn new(l1_format: Arc<dyn Format>, l2_format: Arc<dyn Format>) -> Self {
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
    fn erased_serialize(
        &self,
        value: &dyn erased_serde::Serialize,
        context: &dyn BackendContext,
    ) -> Result<Raw, FormatError> {
        // Check if this is a refill operation (writing L2 data back to L1)
        if let Some(comp_ctx) = context.as_any().downcast_ref::<CompositionContext>() {
            if comp_ctx.policy.write_after_read {
                // For refill operations, create an L1-only envelope
                // This data came from L2, so serialize it in L1 format for L1 storage
                let l1_serialized = self.l1_format.erased_serialize(value, context)?;
                let composition = CompositionEnvelope::L1(CacheValueData::new(l1_serialized));

                return bitcode::serialize(&composition)
                    .map(Bytes::from)
                    .map_err(|e| FormatError::Serialize(Box::new(e)));
            }
        }

        // Normal write path: Create Both envelope with data for both layers
        let l1_serialized = self.l1_format.erased_serialize(value, context)?;

        // If L1 and L2 use the same format, reuse the serialized data instead of serializing again
        let l2_serialized = if self.same_format() {
            l1_serialized.clone()
        } else {
            self.l2_format.erased_serialize(value, context)?
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
        let composition: CompositionEnvelope =
            bitcode::deserialize(data).map_err(|e| FormatError::Deserialize(Box::new(e)))?;

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

    fn format_type_id(&self) -> FormatTypeId {
        // CompositionFormat is a custom format
        FormatTypeId::Custom("composition")
    }
}

/// A backend that composes two cache backends into a layered caching system.
///
/// The first backend (L1) is checked first on reads, and if not found,
/// the second backend (L2) is checked. On writes, both backends are updated.
///
/// Each layer can use its own serialization format and compression since
/// `CacheBackend` operates on typed data, not raw bytes.
///
/// Behavior can be customized via `ReadPolicy`, `WritePolicy`, and `RefillPolicy` to control
/// how reads, writes, and L1 refills are executed across the layers.
pub struct CompositionBackend<
    L1,
    L2,
    R = SequentialReadPolicy,
    W = OptimisticParallelWritePolicy,
    F = AlwaysRefill,
> where
    L1: Backend,
    L2: Backend,
    R: ReadPolicy,
    W: WritePolicy,
    F: RefillPolicy,
{
    /// First-layer cache (typically fast, local)
    l1: L1,
    /// Second-layer cache (typically distributed, persistent)
    l2: L2,
    /// Composition format
    format: CompositionFormat,
    /// Read policy
    read_policy: R,
    /// Write policy
    write_policy: W,
    /// Refill policy
    refill_policy: F,
}

impl<L1, L2>
    CompositionBackend<L1, L2, SequentialReadPolicy, OptimisticParallelWritePolicy, AlwaysRefill>
where
    L1: Backend,
    L2: Backend,
{
    /// Creates a new composition backend with two layers using default policies.
    ///
    /// Default policies:
    /// - Read: `SequentialReadPolicy` (try L1 first, then L2)
    /// - Write: `OptimisticParallelWritePolicy` (write to both, succeed if ≥1 succeeds)
    /// - Refill: `AlwaysRefill` (always populate L1 after L2 hit)
    ///
    /// # Arguments
    /// * `l1` - First-layer backend (checked first on reads)
    /// * `l2` - Second-layer backend (checked if L1 misses)
    pub fn new(l1: L1, l2: L2) -> Self {
        let format = CompositionFormat::new(
            Arc::new(l1.value_format().clone_box()),
            Arc::new(l2.value_format().clone_box()),
        );
        Self {
            l1,
            l2,
            format,
            read_policy: SequentialReadPolicy::new(),
            write_policy: OptimisticParallelWritePolicy::new(),
            refill_policy: AlwaysRefill::new(),
        }
    }
}

impl<L1, L2, R, W, F> CompositionBackend<L1, L2, R, W, F>
where
    L1: Backend,
    L2: Backend,
    R: ReadPolicy,
    W: WritePolicy,
    F: RefillPolicy,
{
    /// Returns a reference to the read policy.
    pub fn read_policy(&self) -> &R {
        &self.read_policy
    }

    /// Returns a reference to the write policy.
    pub fn write_policy(&self) -> &W {
        &self.write_policy
    }

    /// Returns a reference to the refill policy.
    pub fn refill_policy(&self) -> &F {
        &self.refill_policy
    }

    /// Set all policies at once using CompositionPolicy builder.
    ///
    /// This is the preferred way to configure multiple policies.
    ///
    /// # Example
    /// ```ignore
    /// use hitbox_backend::{CompositionBackend, composition::CompositionPolicy};
    /// use hitbox_backend::composition::policy::{RaceReadPolicy, SequentialWritePolicy, NeverRefill};
    ///
    /// let policy = CompositionPolicy::new()
    ///     .read(RaceReadPolicy::new())
    ///     .write(SequentialWritePolicy::new())
    ///     .refill(NeverRefill::new());
    ///
    /// let backend = CompositionBackend::new(l1, l2)
    ///     .with_policy(policy);
    /// ```
    pub fn with_policy<NewR, NewW, NewF>(
        self,
        policy: CompositionPolicy<NewR, NewW, NewF>,
    ) -> CompositionBackend<L1, L2, NewR, NewW, NewF>
    where
        NewR: ReadPolicy,
        NewW: WritePolicy,
        NewF: RefillPolicy,
    {
        CompositionBackend {
            l1: self.l1,
            l2: self.l2,
            format: self.format,
            read_policy: policy.read,
            write_policy: policy.write,
            refill_policy: policy.refill,
        }
    }

    /// Set the read policy (builder pattern).
    ///
    /// This consumes the backend and returns a new one with the updated read policy.
    ///
    /// # Example
    /// ```ignore
    /// use hitbox_backend::CompositionBackend;
    /// use hitbox_backend::composition::policy::RaceReadPolicy;
    ///
    /// let backend = CompositionBackend::new(l1, l2)
    ///     .read(RaceReadPolicy::new());
    /// ```
    pub fn read<NewR: ReadPolicy>(
        self,
        read_policy: NewR,
    ) -> CompositionBackend<L1, L2, NewR, W, F> {
        CompositionBackend {
            l1: self.l1,
            l2: self.l2,
            format: self.format,
            read_policy,
            write_policy: self.write_policy,
            refill_policy: self.refill_policy,
        }
    }

    /// Set the write policy (builder pattern).
    ///
    /// This consumes the backend and returns a new one with the updated write policy.
    ///
    /// # Example
    /// ```ignore
    /// use hitbox_backend::CompositionBackend;
    /// use hitbox_backend::composition::policy::SequentialWritePolicy;
    ///
    /// let backend = CompositionBackend::new(l1, l2)
    ///     .write(SequentialWritePolicy::new());
    /// ```
    pub fn write<NewW: WritePolicy>(
        self,
        write_policy: NewW,
    ) -> CompositionBackend<L1, L2, R, NewW, F> {
        CompositionBackend {
            l1: self.l1,
            l2: self.l2,
            format: self.format,
            read_policy: self.read_policy,
            write_policy,
            refill_policy: self.refill_policy,
        }
    }

    /// Set the refill policy (builder pattern).
    ///
    /// This consumes the backend and returns a new one with the updated refill policy.
    ///
    /// # Example
    /// ```ignore
    /// use hitbox_backend::CompositionBackend;
    /// use hitbox_backend::composition::policy::NeverRefill;
    ///
    /// let backend = CompositionBackend::new(l1, l2)
    ///     .refill(NeverRefill::new());
    /// ```
    pub fn refill<NewF: RefillPolicy>(
        self,
        refill_policy: NewF,
    ) -> CompositionBackend<L1, L2, R, W, NewF> {
        CompositionBackend {
            l1: self.l1,
            l2: self.l2,
            format: self.format,
            read_policy: self.read_policy,
            write_policy: self.write_policy,
            refill_policy,
        }
    }
}

impl<L1, L2, R, W, F> Clone for CompositionBackend<L1, L2, R, W, F>
where
    L1: Clone + Backend,
    L2: Clone + Backend,
    R: Clone + ReadPolicy,
    W: Clone + WritePolicy,
    F: Clone + RefillPolicy,
{
    fn clone(&self) -> Self {
        Self {
            l1: self.l1.clone(),
            l2: self.l2.clone(),
            format: self.format.clone(),
            read_policy: self.read_policy.clone(),
            write_policy: self.write_policy.clone(),
            refill_policy: self.refill_policy.clone(),
        }
    }
}

impl<L1, L2, R, W, F> std::fmt::Debug for CompositionBackend<L1, L2, R, W, F>
where
    L1: std::fmt::Debug + Backend,
    L2: std::fmt::Debug + Backend,
    R: std::fmt::Debug + ReadPolicy,
    W: std::fmt::Debug + WritePolicy,
    F: std::fmt::Debug + RefillPolicy,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositionBackend")
            .field("l1", &self.l1)
            .field("l2", &self.l2)
            .field("format", &self.format)
            .field("read_policy", &self.read_policy)
            .field("write_policy", &self.write_policy)
            .field("refill_policy", &self.refill_policy)
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
impl<L1, L2, R, W, F> Backend for CompositionBackend<L1, L2, R, W, F>
where
    L1: Backend + Send + Sync,
    L2: Backend + Send + Sync,
    R: ReadPolicy,
    W: WritePolicy,
    F: RefillPolicy,
{
    #[tracing::instrument(skip(self), level = "trace")]
    async fn read(&self, key: &CacheKey) -> BackendResult<BackendValue> {
        let l1 = &self.l1;
        let l2 = &self.l2;
        let format = &self.format;

        let read_l1_with_envelope = |k| async move {
            let backend_value = l1.read(k).await?;
            match backend_value.value {
                Some(l1_value) => {
                    let (expire, stale) = (l1_value.expire, l1_value.stale);
                    CompositionEnvelope::L1(l1_value.into())
                        .serialize_to_cache_value(expire, stale)
                        .map(Some)
                }
                None => Ok(None),
            }
        };

        let read_l2_with_envelope = |k| async move {
            let backend_value = l2.read(k).await?;
            match backend_value.value {
                Some(l2_value) => {
                    let (expire, stale) = (l2_value.expire, l2_value.stale);
                    CompositionEnvelope::L2(l2_value.into())
                        .serialize_to_cache_value(expire, stale)
                        .map(Some)
                }
                None => Ok(None),
            }
        };

        let (value, source) = self
            .read_policy
            .execute_with(key, read_l1_with_envelope, read_l2_with_envelope)
            .await?;

        // Create context based on which layer provided the data
        let backend_value = if value.is_some() {
            let ctx = context::CompositionContext::new(source, format.clone());
            BackendValue::with_context(value, ctx)
        } else {
            BackendValue::new(None)
        };

        Ok(backend_value)
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
                // Use write policy to determine how to write to both layers
                let l1_ref = &self.l1;
                let l2_ref = &self.l2;
                let l1_value = l1.into();
                let l2_value = l2.into();

                let write_l1 = |k| async move { l1_ref.write(k, l1_value, ttl).await };
                let write_l2 = |k| async move { l2_ref.write(k, l2_value, ttl).await };

                self.write_policy
                    .execute_with(key, write_l1, write_l2)
                    .await
            }
            CompositionEnvelope::L1(l1) => self.l1.write(key, l1.into(), ttl).await,
            CompositionEnvelope::L2(l2) => self.l2.write(key, l2.into(), ttl).await,
        }
    }

    #[tracing::instrument(skip(self), level = "trace")]
    async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        // Delete from both layers in parallel for better performance
        let (l1_result, l2_result) = futures::join!(self.l1.remove(key), self.l2.remove(key));

        match (l1_result, l2_result) {
            (Err(e1), Err(e2)) => {
                tracing::error!(l1_error = ?e1, l2_error = ?e2, "Both L1 and L2 delete failed");
                Err(BackendError::InternalError(Box::new(
                    CompositionError::BothLayersFailed { l1: e1, l2: e2 },
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

impl<L1, L2, R, W, F> CacheBackend for CompositionBackend<L1, L2, R, W, F>
where
    L1: CacheBackend + Send + Sync,
    L2: CacheBackend + Send + Sync,
    R: ReadPolicy,
    W: WritePolicy,
    F: RefillPolicy,
{
    #[tracing::instrument(skip(self), level = "trace")]
    async fn get<T>(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<T::Cached>>>
    where
        T: CacheableResponse,
        T::Cached: Serialize + DeserializeOwned + Send + Sync,
    {
        let l1 = &self.l1;
        let l2 = &self.l2;
        let refill_policy = &self.refill_policy;

        let read_l1 = |k| async move { l1.get::<T>(k).await };

        let read_l2_with_refill = |k| async move {
            let value = l2.get::<T>(k).await?;

            // Refill L1 on hit using policy (best-effort)
            if let Some(ref v) = value {
                refill_policy
                    .execute(v, || async { l1.set::<T>(k, v, v.ttl(), &()).await })
                    .await;
            }

            Ok(value)
        };

        let (value, _source) = self
            .read_policy
            .execute_with(key, read_l1, read_l2_with_refill)
            .await?;

        Ok(value)
    }

    #[tracing::instrument(skip(self, value, context), level = "trace")]
    async fn set<T>(
        &self,
        key: &CacheKey,
        value: &CacheValue<T::Cached>,
        ttl: Option<Duration>,
        context: &dyn BackendContext,
    ) -> BackendResult<()>
    where
        T: CacheableResponse,
        T::Cached: Serialize + Send + Sync,
    {
        // Use write policy to determine how to write to both layers
        let l1 = &self.l1;
        let l2 = &self.l2;

        let write_l1 = |k| async move { l1.set::<T>(k, value, ttl, context).await };
        let write_l2 = |k| async move { l2.set::<T>(k, value, ttl, context).await };

        self.write_policy
            .execute_with(key, write_l1, write_l2)
            .await
    }

    #[tracing::instrument(skip(self), level = "trace")]
    async fn delete(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
        // Delete from both layers in parallel for better performance
        let (l1_result, l2_result) = futures::join!(self.l1.delete(key), self.l2.delete(key));

        // Aggregate results
        match (l1_result, l2_result) {
            (Err(e1), Err(e2)) => {
                tracing::error!(l1_error = ?e1, l2_error = ?e2, "Both L1 and L2 delete failed");
                Err(BackendError::InternalError(Box::new(
                    CompositionError::BothLayersFailed { l1: e1, l2: e2 },
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
    use hitbox_core::{
        CachePolicy, CacheValue, CacheableResponse, EntityPolicyConfig, Predicate, Raw,
    };
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
        async fn read(&self, key: &CacheKey) -> BackendResult<BackendValue> {
            Ok(BackendValue::new(self.store.lock().unwrap().get(key).cloned()))
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
            .set::<MockResponse>(&key, &value, Some(Duration::from_secs(60)), &())
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
        l2.set::<MockResponse>(&key, &value, Some(Duration::from_secs(60)), &())
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
            .set::<MockResponse>(&key, &value, Some(Duration::from_secs(60)), &())
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
            .set::<MockResponse>(&key, &value, Some(Duration::from_secs(60)), &())
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
            .set::<MockResponse>(&key, &value, Some(Duration::from_secs(60)), &())
            .await
            .unwrap();

        // Read via clone should work (shared backends)
        let result = cloned.get::<MockResponse>(&key).await.unwrap();
        assert_eq!(result.unwrap().data, "value1");
    }
}
