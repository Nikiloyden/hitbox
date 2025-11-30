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
//! let backend = CompositionBackend::new(moka, redis, offload);
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
pub mod envelope;
pub mod format;
pub mod policy;

pub use compose::Compose;
pub use context::{CompositionContext, CompositionLayer, upgrade_context};
pub use format::CompositionFormat;
pub use policy::CompositionPolicy;

use crate::format::Format;
use crate::{
    Backend, BackendError, BackendResult, CacheBackend, CacheKeyFormat, Compressor, DeleteStatus,
    PassthroughCompressor,
};
use async_trait::async_trait;
use envelope::CompositionEnvelope;
use hitbox_core::{
    BoxContext, CacheContext, CacheKey, CacheValue, Cacheable, CacheableResponse, Offload, Raw,
};
use policy::{
    AlwaysRefill, CompositionReadPolicy, CompositionRefillPolicy, CompositionWritePolicy,
    OptimisticParallelWritePolicy, ReadResult, SequentialReadPolicy,
};
use smol_str::SmolStr;
use std::sync::Arc;
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

/// A backend that composes two cache backends into a layered caching system.
///
/// The first backend (L1) is checked first on reads, and if not found,
/// the second backend (L2) is checked. On writes, both backends are updated.
///
/// Each layer can use its own serialization format and compression since
/// `CacheBackend` operates on typed data, not raw bytes.
///
/// Behavior can be customized via `CompositionReadPolicy`, `CompositionWritePolicy`, and `CompositionRefillPolicy` to control
/// how reads, writes, and L1 refills are executed across the layers.
pub struct CompositionBackend<
    L1,
    L2,
    O,
    R = SequentialReadPolicy,
    W = OptimisticParallelWritePolicy,
    F = AlwaysRefill,
> where
    L1: Backend,
    L2: Backend,
    O: Offload,
    R: CompositionReadPolicy,
    W: CompositionWritePolicy,
    F: CompositionRefillPolicy,
{
    /// First-layer cache (typically fast, local)
    l1: L1,
    /// Second-layer cache (typically distributed, persistent)
    l2: L2,
    /// Composition format
    format: CompositionFormat,
    /// Offload for background tasks
    offload: O,
    /// Read policy
    read_policy: R,
    /// Write policy
    write_policy: W,
    /// Refill policy
    refill_policy: F,
    /// Name of this backend for source path composition
    name: SmolStr,
}

impl<L1, L2, O>
    CompositionBackend<L1, L2, O, SequentialReadPolicy, OptimisticParallelWritePolicy, AlwaysRefill>
where
    L1: Backend,
    L2: Backend,
    O: Offload,
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
    /// * `offload` - Offload manager for background tasks (e.g., race policy losers)
    pub fn new(l1: L1, l2: L2, offload: O) -> Self {
        let format = CompositionFormat::new(
            Arc::new(l1.value_format().clone_box()),
            Arc::new(l2.value_format().clone_box()),
            Arc::new(l1.compressor().clone_box()),
            Arc::new(l2.compressor().clone_box()),
        );
        Self {
            l1,
            l2,
            format,
            offload,
            read_policy: SequentialReadPolicy::new(),
            write_policy: OptimisticParallelWritePolicy::new(),
            refill_policy: AlwaysRefill::new(),
            name: SmolStr::new_static("composition"),
        }
    }
}

impl<L1, L2, O, R, W, F> CompositionBackend<L1, L2, O, R, W, F>
where
    L1: Backend,
    L2: Backend,
    O: Offload,
    R: CompositionReadPolicy,
    W: CompositionWritePolicy,
    F: CompositionRefillPolicy,
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

    /// Returns a reference to the offload manager.
    pub fn offload(&self) -> &O {
        &self.offload
    }

    /// Set a custom name for this backend.
    ///
    /// The name is used for source path composition in multi-layer caches.
    /// For example, with name "cache", the source path might be "cache.L1".
    pub fn name(mut self, name: impl Into<SmolStr>) -> Self {
        self.name = name.into();
        self
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
    /// let backend = CompositionBackend::new(l1, l2, offload)
    ///     .with_policy(policy);
    /// ```
    pub fn with_policy<NewR, NewW, NewF>(
        self,
        policy: CompositionPolicy<NewR, NewW, NewF>,
    ) -> CompositionBackend<L1, L2, O, NewR, NewW, NewF>
    where
        NewR: CompositionReadPolicy,
        NewW: CompositionWritePolicy,
        NewF: CompositionRefillPolicy,
    {
        CompositionBackend {
            l1: self.l1,
            l2: self.l2,
            format: self.format,
            offload: self.offload,
            read_policy: policy.read,
            write_policy: policy.write,
            refill_policy: policy.refill,
            name: self.name,
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
    /// let backend = CompositionBackend::new(l1, l2, offload)
    ///     .read(RaceReadPolicy::new());
    /// ```
    pub fn read<NewR: CompositionReadPolicy>(
        self,
        read_policy: NewR,
    ) -> CompositionBackend<L1, L2, O, NewR, W, F> {
        CompositionBackend {
            l1: self.l1,
            l2: self.l2,
            format: self.format,
            offload: self.offload,
            read_policy,
            write_policy: self.write_policy,
            refill_policy: self.refill_policy,
            name: self.name,
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
    /// let backend = CompositionBackend::new(l1, l2, offload)
    ///     .write(SequentialWritePolicy::new());
    /// ```
    pub fn write<NewW: CompositionWritePolicy>(
        self,
        write_policy: NewW,
    ) -> CompositionBackend<L1, L2, O, R, NewW, F> {
        CompositionBackend {
            l1: self.l1,
            l2: self.l2,
            format: self.format,
            offload: self.offload,
            read_policy: self.read_policy,
            write_policy,
            refill_policy: self.refill_policy,
            name: self.name,
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
    /// let backend = CompositionBackend::new(l1, l2, offload)
    ///     .refill(NeverRefill::new());
    /// ```
    pub fn refill<NewF: CompositionRefillPolicy>(
        self,
        refill_policy: NewF,
    ) -> CompositionBackend<L1, L2, O, R, W, NewF> {
        CompositionBackend {
            l1: self.l1,
            l2: self.l2,
            format: self.format,
            offload: self.offload,
            read_policy: self.read_policy,
            write_policy: self.write_policy,
            refill_policy,
            name: self.name,
        }
    }
}

impl<L1, L2, O, R, W, F> Clone for CompositionBackend<L1, L2, O, R, W, F>
where
    L1: Clone + Backend,
    L2: Clone + Backend,
    O: Offload,
    R: Clone + CompositionReadPolicy,
    W: Clone + CompositionWritePolicy,
    F: Clone + CompositionRefillPolicy,
{
    fn clone(&self) -> Self {
        Self {
            l1: self.l1.clone(),
            l2: self.l2.clone(),
            format: self.format.clone(),
            offload: self.offload.clone(),
            read_policy: self.read_policy.clone(),
            write_policy: self.write_policy.clone(),
            refill_policy: self.refill_policy.clone(),
            name: self.name.clone(),
        }
    }
}

impl<L1, L2, O, R, W, F> std::fmt::Debug for CompositionBackend<L1, L2, O, R, W, F>
where
    L1: std::fmt::Debug + Backend,
    L2: std::fmt::Debug + Backend,
    O: std::fmt::Debug + Offload,
    R: std::fmt::Debug + CompositionReadPolicy,
    W: std::fmt::Debug + CompositionWritePolicy,
    F: std::fmt::Debug + CompositionRefillPolicy,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositionBackend")
            .field("name", &self.name)
            .field("l1", &self.l1)
            .field("l2", &self.l2)
            .field("format", &self.format)
            .field("offload", &self.offload)
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
impl<L1, L2, O, R, W, F> Backend for CompositionBackend<L1, L2, O, R, W, F>
where
    L1: Backend + Clone + Send + Sync + 'static,
    L2: Backend + Clone + Send + Sync + 'static,
    O: Offload,
    R: CompositionReadPolicy,
    W: CompositionWritePolicy,
    F: CompositionRefillPolicy,
{
    #[tracing::instrument(skip(self), level = "trace")]
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        // Clone backends for 'static closures
        let l1 = self.l1.clone();
        let l2 = self.l2.clone();

        let read_l1_with_envelope = |k: CacheKey| async move {
            let ctx: BoxContext = CacheContext::default().boxed();
            let result = match l1.read(&k).await {
                Ok(Some(l1_value)) => {
                    let (expire, stale) = (l1_value.expire, l1_value.stale);
                    let envelope = CompositionEnvelope::L1(l1_value);
                    match envelope.serialize() {
                        Ok(packed) => Ok(Some(CacheValue::new(packed, expire, stale))),
                        Err(e) => Err(e),
                    }
                }
                Ok(None) => Ok(None),
                Err(e) => Err(e),
            };
            (result, ctx)
        };

        let read_l2_with_envelope = |k: CacheKey| async move {
            let ctx: BoxContext = CacheContext::default().boxed();
            let result = match l2.read(&k).await {
                Ok(Some(l2_value)) => {
                    let (expire, stale) = (l2_value.expire, l2_value.stale);
                    let envelope = CompositionEnvelope::L2(l2_value);
                    match envelope.serialize() {
                        Ok(packed) => Ok(Some(CacheValue::new(packed, expire, stale))),
                        Err(e) => Err(e),
                    }
                }
                Ok(None) => Ok(None),
                Err(e) => Err(e),
            };
            (result, ctx)
        };

        let ReadResult { value, .. } = self
            .read_policy
            .execute_with(
                key.clone(),
                read_l1_with_envelope,
                read_l2_with_envelope,
                &self.offload,
            )
            .await?;

        // No context creation - Format will extract context from envelope during deserialization
        Ok(value)
    }

    #[tracing::instrument(skip(self, value), level = "trace")]
    async fn write(&self, key: &CacheKey, value: CacheValue<Raw>) -> BackendResult<()> {
        // Unpack CompositionEnvelope using zero-copy format
        let composition = CompositionEnvelope::deserialize(&value.data)?;

        // Write to appropriate layers
        // In normal usage via CacheBackend::set, this is always Both variant
        // The L1/L2 branches are defensive code for edge cases
        match composition {
            CompositionEnvelope::Both { l1, l2 } => {
                // Clone backends for 'static closures
                let l1_backend = self.l1.clone();
                let l2_backend = self.l2.clone();

                let write_l1 = |k: CacheKey| async move { l1_backend.write(&k, l1).await };
                let write_l2 = |k: CacheKey| async move { l2_backend.write(&k, l2).await };

                self.write_policy
                    .execute_with(key.clone(), write_l1, write_l2, &self.offload)
                    .await
            }
            CompositionEnvelope::L1(l1) => self.l1.write(key, l1).await,
            CompositionEnvelope::L2(l2) => self.l2.write(key, l2).await,
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

    fn name(&self) -> &str {
        &self.name
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

impl<L1, L2, O, R, W, F> CacheBackend for CompositionBackend<L1, L2, O, R, W, F>
where
    L1: CacheBackend + Clone + Send + Sync + 'static,
    L2: CacheBackend + Clone + Send + Sync + 'static,
    O: Offload,
    R: CompositionReadPolicy,
    W: CompositionWritePolicy,
    F: CompositionRefillPolicy + Clone + 'static,
{
    #[tracing::instrument(skip(self, ctx), level = "trace")]
    async fn get<T>(
        &self,
        key: &CacheKey,
        ctx: &mut BoxContext,
    ) -> BackendResult<Option<CacheValue<T::Cached>>>
    where
        T: CacheableResponse,
        T::Cached: Cacheable,
    {
        // Clone backends and refill policy for 'static closures
        let l1 = self.l1.clone();
        let l2 = self.l2.clone();
        let refill_policy = self.refill_policy.clone();

        // Clone context for internal L1/L2 operations
        let l1_ctx = ctx.clone_box();
        let l2_ctx = ctx.clone_box();

        // Clone l1 for refill in L2 closure
        let l1_for_refill = l1.clone();

        let read_l1 = |k: CacheKey| async move {
            let mut internal_ctx = l1_ctx;
            let result = l1.get::<T>(&k, &mut internal_ctx).await;
            (result, internal_ctx)
        };

        let read_l2_with_refill = |k: CacheKey| async move {
            let mut internal_ctx = l2_ctx;
            let result = l2.get::<T>(&k, &mut internal_ctx).await;

            // Refill L1 on hit using policy (best-effort)
            // Metrics are recorded directly in internal_ctx
            if let Ok(Some(ref v)) = result {
                refill_policy
                    .execute(v, || async {
                        l1_for_refill.set::<T>(&k, v, &mut internal_ctx).await
                    })
                    .await;
            }

            (result, internal_ctx)
        };

        let ReadResult {
            value,
            context: inner_ctx,
            ..
        } = self
            .read_policy
            .execute_with(key.clone(), read_l1, read_l2_with_refill, &self.offload)
            .await?;

        // Merge inner context into outer context, composing source paths
        if value.is_some() {
            ctx.merge_from(&*inner_ctx, &self.name);
        }

        Ok(value)
    }

    #[tracing::instrument(skip(self, value, ctx), level = "trace")]
    async fn set<T>(
        &self,
        key: &CacheKey,
        value: &CacheValue<T::Cached>,
        ctx: &mut BoxContext,
    ) -> BackendResult<()>
    where
        T: CacheableResponse,
        T::Cached: Cacheable,
    {
        // Clone backends for 'static closures
        let l1 = self.l1.clone();
        let l2 = self.l2.clone();

        // Clone context for internal operations
        let l1_ctx = ctx.clone_box();
        let l2_ctx = ctx.clone_box();

        // Clone value for closures
        let value_for_l1 = value.clone();
        let value_for_l2 = value.clone();

        let write_l1 = |k: CacheKey| async move {
            let mut internal_ctx = l1_ctx;
            l1.set::<T>(&k, &value_for_l1, &mut internal_ctx).await
        };
        let write_l2 = |k: CacheKey| async move {
            let mut internal_ctx = l2_ctx;
            l2.set::<T>(&k, &value_for_l2, &mut internal_ctx).await
        };

        self.write_policy
            .execute_with(key.clone(), write_l1, write_l2, &self.offload)
            .await
    }

    #[tracing::instrument(skip(self, ctx), level = "trace")]
    async fn delete(&self, key: &CacheKey, ctx: &mut BoxContext) -> BackendResult<DeleteStatus> {
        // Delete from both layers in parallel for better performance
        let mut l1_ctx = ctx.clone_box();
        let mut l2_ctx = ctx.clone_box();
        let (l1_result, l2_result) = futures::join!(
            self.l1.delete(key, &mut l1_ctx),
            self.l2.delete(key, &mut l2_ctx)
        );

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
    use crate::format::{Format, JsonFormat};
    use crate::{Backend, CacheKeyFormat, Compressor, PassthroughCompressor};
    use async_trait::async_trait;
    use chrono::Utc;
    use hitbox_core::{
        BoxContext, CacheContext, CachePolicy, CacheStatus, CacheValue, CacheableResponse,
        EntityPolicyConfig, Predicate, Raw, ResponseSource,
    };
    use serde::{Deserialize, Serialize};
    use smol_str::SmolStr;
    use std::collections::HashMap;
    use std::future::Future;
    use std::sync::{Arc, Mutex};

    #[cfg(feature = "rkyv_format")]
    use rkyv::{Archive, Serialize as RkyvSerialize};
    #[cfg(feature = "rkyv_format")]
    use rkyv_typename::TypeName;

    /// Test offload that spawns tasks with tokio::spawn
    #[derive(Clone, Debug)]
    struct TestOffload;

    impl Offload for TestOffload {
        fn spawn<F>(&self, _kind: impl Into<SmolStr>, future: F)
        where
            F: Future<Output = ()> + Send + 'static,
        {
            tokio::spawn(future);
        }
    }

    // Simple in-memory backend for testing
    #[derive(Clone, Debug)]
    struct TestBackend {
        store: Arc<Mutex<HashMap<CacheKey, CacheValue<Raw>>>>,
        name: &'static str,
    }

    impl TestBackend {
        fn new() -> Self {
            Self {
                store: Arc::new(Mutex::new(HashMap::new())),
                name: "test",
            }
        }

        fn with_name(name: &'static str) -> Self {
            Self {
                store: Arc::new(Mutex::new(HashMap::new())),
                name,
            }
        }
    }

    #[async_trait]
    impl Backend for TestBackend {
        async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
            Ok(self.store.lock().unwrap().get(key).cloned())
        }

        async fn write(&self, key: &CacheKey, value: CacheValue<Raw>) -> BackendResult<()> {
            self.store.lock().unwrap().insert(key.clone(), value);
            Ok(())
        }

        async fn remove(&self, key: &CacheKey) -> BackendResult<DeleteStatus> {
            match self.store.lock().unwrap().remove(key) {
                Some(_) => Ok(DeleteStatus::Deleted(1)),
                None => Ok(DeleteStatus::Missing),
            }
        }

        fn name(&self) -> &str {
            self.name
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

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[cfg_attr(
        feature = "rkyv_format",
        derive(Archive, RkyvSerialize, rkyv::Deserialize, TypeName)
    )]
    #[cfg_attr(feature = "rkyv_format", archive(check_bytes))]
    #[cfg_attr(feature = "rkyv_format", archive_attr(derive(TypeName)))]
    struct CachedData {
        value: String,
    }

    // Mock CacheableResponse for testing
    // We only need the associated type, the actual methods are not used in these tests
    struct MockResponse;

    // Note: This is a minimal implementation just for testing CacheBackend.
    // The methods are not actually called in these tests.
    #[async_trait]
    impl CacheableResponse for MockResponse {
        type Cached = CachedData;
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
        let l1 = TestBackend::with_name("moka");
        let l2 = TestBackend::with_name("redis");
        let backend = CompositionBackend::new(l1.clone(), l2, TestOffload).name("cache");

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            CachedData {
                value: "value1".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write to populate both layers
        let mut ctx: BoxContext = CacheContext::default().boxed();
        backend
            .set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        // Read should hit L1
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let result = backend.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(result.unwrap().data.value, "value1");

        // Verify source path is composed correctly: "cache.moka"
        assert_eq!(ctx.status(), CacheStatus::Hit);
        assert_eq!(ctx.source(), &ResponseSource::Backend("cache.moka".into()));
    }

    #[tokio::test]
    async fn test_l2_hit_populates_l1() {
        let l1 = TestBackend::with_name("moka");
        let l2 = TestBackend::with_name("redis");

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            CachedData {
                value: "value1".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write only to L2
        let mut ctx: BoxContext = CacheContext::default().boxed();
        l2.set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        let backend = CompositionBackend::new(l1.clone(), l2, TestOffload).name("cache");

        // First read should hit L2 and populate L1
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let result = backend.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(result.unwrap().data.value, "value1");

        // Verify source path is composed correctly: "cache.redis" (hit L2)
        assert_eq!(ctx.status(), CacheStatus::Hit);
        assert_eq!(ctx.source(), &ResponseSource::Backend("cache.redis".into()));

        // Verify L1 was populated from L2 (cache warming)
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let l1_result = l1.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert!(l1_result.is_some(), "L1 should be populated from L2 hit");
        assert_eq!(l1_result.unwrap().data.value, "value1");
    }

    #[tokio::test]
    async fn test_miss_both_layers() {
        let l1 = TestBackend::new();
        let l2 = TestBackend::new();
        let backend = CompositionBackend::new(l1, l2, TestOffload);

        let key = CacheKey::from_str("test", "nonexistent");

        let mut ctx: BoxContext = CacheContext::default().boxed();
        let result = backend.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_write_to_both_layers() {
        let l1 = TestBackend::new();
        let l2 = TestBackend::new();

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            CachedData {
                value: "value1".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        let backend = CompositionBackend::new(l1.clone(), l2.clone(), TestOffload);

        let mut ctx: BoxContext = CacheContext::default().boxed();
        backend
            .set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        // Verify both layers have the value
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let l1_result = l1.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(l1_result.unwrap().data.value, "value1");

        let mut ctx: BoxContext = CacheContext::default().boxed();
        let l2_result = l2.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(l2_result.unwrap().data.value, "value1");
    }

    #[tokio::test]
    async fn test_delete_from_both_layers() {
        let l1 = TestBackend::new();
        let l2 = TestBackend::new();

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            CachedData {
                value: "value1".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        let backend = CompositionBackend::new(l1.clone(), l2.clone(), TestOffload);

        // Write to both
        let mut ctx: BoxContext = CacheContext::default().boxed();
        backend
            .set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        // Delete from both
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let status = backend.delete(&key, &mut ctx).await.unwrap();
        assert_eq!(status, DeleteStatus::Deleted(2));

        // Verify both layers no longer have the value
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let l1_result = l1.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert!(l1_result.is_none());

        let mut ctx: BoxContext = CacheContext::default().boxed();
        let l2_result = l2.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert!(l2_result.is_none());
    }

    #[tokio::test]
    async fn test_clone() {
        let l1 = TestBackend::new();
        let l2 = TestBackend::new();
        let backend = CompositionBackend::new(l1, l2, TestOffload);

        let cloned = backend.clone();

        let key = CacheKey::from_str("test", "key1");
        let value = CacheValue::new(
            CachedData {
                value: "value1".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write via original
        let mut ctx: BoxContext = CacheContext::default().boxed();
        backend
            .set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        // Read via clone should work (shared backends)
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let result = cloned.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(result.unwrap().data.value, "value1");
    }

    #[tokio::test]
    async fn test_nested_composition_source_path() {
        // Create a nested composition: outer(inner(l1, l2), l3)
        // to test hierarchical source paths like "outer.inner.moka"

        let l1 = TestBackend::with_name("moka");
        let l2 = TestBackend::with_name("redis");
        let l3 = TestBackend::with_name("disk");

        // Inner composition: L1=moka, L2=redis
        let inner = CompositionBackend::new(l1.clone(), l2.clone(), TestOffload).name("inner");

        // Outer composition: L1=inner, L2=disk
        let outer = CompositionBackend::new(inner, l3.clone(), TestOffload).name("outer");

        let key = CacheKey::from_str("test", "nested");
        let value = CacheValue::new(
            CachedData {
                value: "nested_value".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write only to innermost L1 (moka)
        let mut ctx: BoxContext = CacheContext::default().boxed();
        l1.set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        // Read through outer composition - should hit inner.L1 (moka)
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let result = outer.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(result.unwrap().data.value, "nested_value");

        // Verify nested source path: "outer.inner.moka"
        assert_eq!(ctx.status(), CacheStatus::Hit);
        assert_eq!(
            ctx.source(),
            &ResponseSource::Backend("outer.inner.moka".into())
        );
    }

    #[tokio::test]
    async fn test_nested_composition_l2_source_path() {
        // Test nested composition where hit comes from inner L2

        let l1 = TestBackend::with_name("moka");
        let l2 = TestBackend::with_name("redis");
        let l3 = TestBackend::with_name("disk");

        // Inner composition: L1=moka, L2=redis
        let inner = CompositionBackend::new(l1.clone(), l2.clone(), TestOffload).name("inner");

        // Outer composition: L1=inner, L2=disk
        let outer = CompositionBackend::new(inner, l3.clone(), TestOffload).name("outer");

        let key = CacheKey::from_str("test", "nested_l2");
        let value = CacheValue::new(
            CachedData {
                value: "from_redis".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write only to inner L2 (redis) - not to moka
        let mut ctx: BoxContext = CacheContext::default().boxed();
        l2.set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        // Read through outer composition - should hit inner.L2 (redis)
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let result = outer.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(result.unwrap().data.value, "from_redis");

        // Verify nested source path: "outer.inner.redis"
        assert_eq!(ctx.status(), CacheStatus::Hit);
        assert_eq!(
            ctx.source(),
            &ResponseSource::Backend("outer.inner.redis".into())
        );
    }

    #[tokio::test]
    async fn test_nested_composition_outer_l2_source_path() {
        // Test nested composition where hit comes from outer L2 (disk)

        let l1 = TestBackend::with_name("moka");
        let l2 = TestBackend::with_name("redis");
        let l3 = TestBackend::with_name("disk");

        // Inner composition: L1=moka, L2=redis
        let inner = CompositionBackend::new(l1.clone(), l2.clone(), TestOffload).name("inner");

        // Outer composition: L1=inner, L2=disk
        let outer = CompositionBackend::new(inner, l3.clone(), TestOffload).name("outer");

        let key = CacheKey::from_str("test", "outer_l2");
        let value = CacheValue::new(
            CachedData {
                value: "from_disk".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write only to outer L2 (disk) - not to inner composition
        let mut ctx: BoxContext = CacheContext::default().boxed();
        l3.set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        // Read through outer composition - should hit outer L2 (disk)
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let result = outer.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(result.unwrap().data.value, "from_disk");

        // Verify source path: "outer.disk"
        assert_eq!(ctx.status(), CacheStatus::Hit);
        assert_eq!(ctx.source(), &ResponseSource::Backend("outer.disk".into()));
    }

    #[tokio::test]
    async fn test_metrics_recorded_on_l1_hit() {
        let l1 = TestBackend::with_name("moka");
        let l2 = TestBackend::with_name("redis");
        let backend = CompositionBackend::new(l1.clone(), l2, TestOffload).name("cache");

        let key = CacheKey::from_str("test", "metrics1");
        let value = CacheValue::new(
            CachedData {
                value: "value1".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write directly to L1 backend to set up the test
        let mut ctx: BoxContext = CacheContext::default().boxed();
        l1.set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        // Verify write metrics on direct L1 write
        let metrics = ctx.metrics();
        assert!(metrics.layers.contains_key("moka"));
        assert_eq!(metrics.layers["moka"].writes, 1);
        assert!(metrics.layers["moka"].bytes_written > 0);

        // Read through composition should hit L1
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let result = backend.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(result.unwrap().data.value, "value1");

        // Verify read metrics - should have read from moka (L1 hit)
        let metrics = ctx.metrics();
        assert!(
            metrics.layers.contains_key("cache.moka"),
            "expected cache.moka in {:?}",
            metrics.layers.keys().collect::<Vec<_>>()
        );
        let moka_metrics = &metrics.layers["cache.moka"];
        assert_eq!(moka_metrics.reads, 1);
        assert!(moka_metrics.bytes_read > 0);
    }

    #[tokio::test]
    async fn test_metrics_recorded_on_l2_hit_with_refill() {
        let l1 = TestBackend::with_name("moka");
        let l2 = TestBackend::with_name("redis");

        let key = CacheKey::from_str("test", "metrics2");
        let value = CacheValue::new(
            CachedData {
                value: "from_l2".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write only to L2
        let mut ctx: BoxContext = CacheContext::default().boxed();
        l2.set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        let backend = CompositionBackend::new(l1.clone(), l2, TestOffload).name("cache");

        // Read should hit L2 and refill L1
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let result = backend.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(result.unwrap().data.value, "from_l2");

        // Verify metrics - should have:
        // - L1 read miss (moka)
        // - L2 read hit (redis)
        // - L1 refill write (moka) - metrics are now captured!
        let metrics = ctx.metrics();

        // L1 (moka) should have 1 read (miss) and 1 write (refill)
        assert!(
            metrics.layers.contains_key("cache.moka"),
            "expected cache.moka in {:?}",
            metrics.layers.keys().collect::<Vec<_>>()
        );
        let moka_metrics = &metrics.layers["cache.moka"];
        assert_eq!(moka_metrics.reads, 1, "moka should have 1 read (miss)");
        assert_eq!(moka_metrics.writes, 1, "moka should have 1 write (refill)");

        // L2 (redis) should have 1 read (hit)
        assert!(
            metrics.layers.contains_key("cache.redis"),
            "expected cache.redis in {:?}",
            metrics.layers.keys().collect::<Vec<_>>()
        );
        let redis_metrics = &metrics.layers["cache.redis"];
        assert_eq!(redis_metrics.reads, 1, "redis should have 1 read (hit)");
        assert!(redis_metrics.bytes_read > 0, "redis should have bytes read");
    }

    #[tokio::test]
    async fn test_metrics_nested_composition() {
        let l1 = TestBackend::with_name("moka");
        let l2 = TestBackend::with_name("redis");
        let l3 = TestBackend::with_name("disk");

        let inner = CompositionBackend::new(l1.clone(), l2.clone(), TestOffload).name("inner");
        let outer = CompositionBackend::new(inner, l3.clone(), TestOffload).name("outer");

        let key = CacheKey::from_str("test", "nested_metrics");
        let value = CacheValue::new(
            CachedData {
                value: "nested".to_string(),
            },
            Some(Utc::now() + chrono::Duration::seconds(60)),
            None,
        );

        // Write to innermost L1 (moka)
        let mut ctx: BoxContext = CacheContext::default().boxed();
        l1.set::<MockResponse>(&key, &value, &mut ctx)
            .await
            .unwrap();

        // Read through outer composition
        let mut ctx: BoxContext = CacheContext::default().boxed();
        let result = outer.get::<MockResponse>(&key, &mut ctx).await.unwrap();
        assert_eq!(result.unwrap().data.value, "nested");

        // Verify nested metrics with composed source paths
        let metrics = ctx.metrics();
        assert!(
            metrics.layers.contains_key("outer.inner.moka"),
            "should have nested path metrics"
        );
        let moka_metrics = &metrics.layers["outer.inner.moka"];
        assert_eq!(moka_metrics.reads, 1);
        assert!(moka_metrics.bytes_read > 0);
    }
}
