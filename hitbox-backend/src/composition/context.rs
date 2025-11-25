//! Context types for composition backend operations.
//!
//! This module provides context types that enable refill operations in composition
//! backends when using dynamic dispatch (`Box<dyn Backend>`).

use std::any::Any;

use crate::{BackendContext, BackendPolicy};

use super::CompositionFormat;

/// Source marker indicating which layer provided the data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionSource {
    /// Data came from L1 cache
    L1,
    /// Data came from L2 cache
    L2,
}

/// Context for composition backend operations.
///
/// This context tracks which layer provided the data and stores the
/// CompositionFormat for proper serialization during refill operations.
///
/// When data is read from L2 in a composition backend, the context is created
/// with `write_after_read=true`, signaling that `CacheBackend::get()` should
/// write the value back to L1 (refill operation).
#[derive(Clone)]
pub struct CompositionContext {
    /// Which layer the data came from
    pub source: CompositionSource,
    /// The composition format for serialization
    pub format: CompositionFormat,
    /// Policy hints for this context
    pub policy: BackendPolicy,
}

impl CompositionContext {
    /// Create a new composition context with the given source and format.
    ///
    /// If the source is L2, write_after_read is enabled to signal that
    /// the data should be written back to L1 for cache refill.
    pub fn new(source: CompositionSource, format: CompositionFormat) -> Self {
        let write_after_read = matches!(source, CompositionSource::L2);
        Self {
            source,
            format,
            policy: BackendPolicy { write_after_read },
        }
    }
}

impl BackendContext for CompositionContext {
    fn policy(&self) -> BackendPolicy {
        self.policy
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl std::fmt::Debug for CompositionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositionContext")
            .field("source", &self.source)
            .field("policy", &self.policy)
            .finish()
    }
}
