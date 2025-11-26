//! Cache context types for tracking cache operation results.

use std::any::Any;
use std::collections::HashMap;

/// Whether the request resulted in a cache hit, miss, or stale data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CacheStatus {
    Hit,
    #[default]
    Miss,
    Stale,
}

/// Source of the response - either from upstream or from a cache backend.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ResponseSource {
    /// Response came from upstream service (cache miss or bypass).
    #[default]
    Upstream,
    /// Response came from cache backend with the given name.
    Backend(String),
}

/// Mode for cache read operations.
///
/// Controls post-read behavior, particularly for composition backends
/// where data read from one layer may need to be written to another.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReadMode {
    /// Direct read - return value without side effects.
    #[default]
    Direct,
    /// Refill mode - write value back to source layer after reading.
    ///
    /// Used in composition backends to populate L1 with data read from L2.
    Refill,
}

/// Metrics for a single cache backend/layer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayerMetrics {
    /// Number of read operations.
    pub reads: u32,
    /// Number of write operations.
    pub writes: u32,
    /// Number of delete operations.
    pub deletes: u32,
    /// Total bytes read.
    pub bytes_read: u64,
    /// Total bytes written.
    pub bytes_written: u64,
    /// Number of read errors.
    pub read_errors: u32,
    /// Number of write errors.
    pub write_errors: u32,
    /// Number of delete errors.
    pub delete_errors: u32,
}

impl LayerMetrics {
    /// Merge another LayerMetrics into this one by summing all fields.
    pub fn merge(&mut self, other: &LayerMetrics) {
        self.reads += other.reads;
        self.writes += other.writes;
        self.deletes += other.deletes;
        self.bytes_read += other.bytes_read;
        self.bytes_written += other.bytes_written;
        self.read_errors += other.read_errors;
        self.write_errors += other.write_errors;
        self.delete_errors += other.delete_errors;
    }
}

/// Aggregated metrics by source path.
///
/// Tracks cache operation metrics for each backend in the cache hierarchy.
/// Source paths are hierarchical, e.g., "composition.moka" or "outer.inner.redis".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metrics {
    /// Metrics aggregated by source path (e.g., "cache.moka" -> LayerMetrics).
    pub layers: HashMap<String, LayerMetrics>,
}

impl Metrics {
    /// Create new empty metrics.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a read operation.
    pub fn record_read(&mut self, source: &str, bytes: u64, success: bool) {
        let layer = self.layers.entry(source.to_owned()).or_default();
        layer.reads += 1;
        if success {
            layer.bytes_read += bytes;
        } else {
            layer.read_errors += 1;
        }
    }

    /// Record a write operation.
    pub fn record_write(&mut self, source: &str, bytes: u64, success: bool) {
        let layer = self.layers.entry(source.to_owned()).or_default();
        layer.writes += 1;
        if success {
            layer.bytes_written += bytes;
        } else {
            layer.write_errors += 1;
        }
    }

    /// Record a delete operation.
    pub fn record_delete(&mut self, source: &str, success: bool) {
        let layer = self.layers.entry(source.to_owned()).or_default();
        layer.deletes += 1;
        if !success {
            layer.delete_errors += 1;
        }
    }

    /// Merge metrics from another Metrics instance, prefixing all source paths.
    ///
    /// Used by composition backends to incorporate inner backend metrics
    /// with hierarchical naming.
    pub fn merge_with_prefix(&mut self, other: &Metrics, prefix: &str) {
        for (source, layer_metrics) in &other.layers {
            let prefixed_source = format!("{}.{}", prefix, source);
            self.layers
                .entry(prefixed_source)
                .or_default()
                .merge(layer_metrics);
        }
    }

    /// Check if metrics are empty.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}

/// Unified context for cache operations.
///
/// This trait combines operation tracking (status, source) with backend policy hints.
/// It allows a single context object to flow through the entire cache pipeline,
/// being transformed as needed by different layers.
///
/// # Usage
///
/// - `CacheFuture` creates a `Box<dyn Context>` at the start
/// - Context is passed as `&mut BoxContext` through backend operations
/// - Backends can upgrade the context type via `*ctx = Box::new(NewContext { ... })`
/// - Format uses `&dyn Context` for policy hints during serialization
/// - At the end, convert to `CacheContext` via `into_cache_context()`
pub trait Context: Send + Sync {
    // Operation tracking

    /// Returns the cache status.
    fn status(&self) -> CacheStatus;

    /// Sets the cache status.
    fn set_status(&mut self, status: CacheStatus);

    /// Returns the response source.
    fn source(&self) -> &ResponseSource;

    /// Sets the response source.
    fn set_source(&mut self, source: ResponseSource);

    // Read mode

    /// Returns the read mode for this context.
    fn read_mode(&self) -> ReadMode {
        ReadMode::default()
    }

    /// Sets the read mode.
    fn set_read_mode(&mut self, _mode: ReadMode) {
        // Default implementation does nothing - simple contexts ignore read mode
    }

    // Metrics

    /// Returns a reference to the metrics.
    fn metrics(&self) -> &Metrics;

    /// Returns a mutable reference to the metrics.
    fn metrics_mut(&mut self) -> &mut Metrics;

    // Type identity and conversion

    /// Returns a reference to self as `Any` for downcasting.
    fn as_any(&self) -> &dyn Any;

    /// Clone this context into a box.
    fn clone_box(&self) -> BoxContext;

    /// Consumes boxed self and returns a `CacheContext`.
    fn into_cache_context(self: Box<Self>) -> CacheContext;

    /// Merge fields from another context into this one.
    ///
    /// Used by composition backends to combine results from inner backends.
    /// The `prefix` is prepended to the source path for hierarchical naming.
    ///
    /// # Arguments
    /// * `other` - The inner context to merge from
    /// * `prefix` - Name prefix to prepend to source path (e.g., backend name)
    fn merge_from(&mut self, other: &dyn Context, prefix: &str) {
        // Merge status - take the inner status if it indicates a hit
        let inner_status = other.status();
        if inner_status == CacheStatus::Hit || inner_status == CacheStatus::Stale {
            self.set_status(inner_status);
        }

        // Merge source with path composition
        match other.source() {
            ResponseSource::Backend(inner_name) => {
                // Compose: prefix.inner_name (e.g., "composition.moka")
                let composed = format!("{}.{}", prefix, inner_name);
                self.set_source(ResponseSource::Backend(composed));
            }
            ResponseSource::Upstream => {
                // No backend hit, keep as upstream
            }
        }

        // Merge metrics with prefix
        self.metrics_mut()
            .merge_with_prefix(other.metrics(), prefix);
    }
}

/// Boxed context trait object.
///
/// Used to pass context through cache operations with dynamic dispatch.
/// Only one allocation at `CacheFuture` creation.
pub type BoxContext = Box<dyn Context>;

/// Context information about a cache operation.
#[derive(Debug, Clone, Default)]
pub struct CacheContext {
    /// Whether the request resulted in a cache hit, miss, or stale data.
    pub status: CacheStatus,
    /// Source of the response.
    pub source: ResponseSource,
    /// Metrics aggregated by source path.
    pub metrics: Metrics,
}

impl Context for CacheContext {
    fn status(&self) -> CacheStatus {
        self.status
    }

    fn set_status(&mut self, status: CacheStatus) {
        self.status = status;
    }

    fn source(&self) -> &ResponseSource {
        &self.source
    }

    fn set_source(&mut self, source: ResponseSource) {
        self.source = source;
    }

    fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    fn metrics_mut(&mut self) -> &mut Metrics {
        &mut self.metrics
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> BoxContext {
        Box::new(self.clone())
    }

    fn into_cache_context(self: Box<Self>) -> CacheContext {
        *self
    }
}
