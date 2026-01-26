//! Cache context types for tracking cache operation results.

use std::any::Any;

use smallbox::{SmallBox, smallbox, space::S4};

use crate::label::BackendLabel;

/// Whether the request resulted in a cache hit, miss, or stale data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CacheStatus {
    /// Cache hit - valid cached data was found and returned.
    Hit,
    /// Cache miss - no cached data was found.
    #[default]
    Miss,
    /// Stale data - cached data was found but has exceeded its freshness window.
    Stale,
}

impl CacheStatus {
    /// Returns the status as a string slice.
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            CacheStatus::Hit => "hit",
            CacheStatus::Miss => "miss",
            CacheStatus::Stale => "stale",
        }
    }
}

/// Source of the response - either from upstream or from a cache backend.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ResponseSource {
    /// Response came from upstream service (cache miss or bypass).
    #[default]
    Upstream,
    /// Response came from cache backend with the given label.
    Backend(BackendLabel),
}

impl ResponseSource {
    /// Returns the source as a string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            ResponseSource::Upstream => "upstream",
            ResponseSource::Backend(label) => label.as_str(),
        }
    }
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
    /// * `prefix` - Label prefix to prepend to source path (e.g., backend label)
    fn merge_from(&mut self, other: &dyn Context, prefix: &BackendLabel) {
        // Merge status - take the inner status if it indicates a hit
        let inner_status = other.status();
        if inner_status == CacheStatus::Hit || inner_status == CacheStatus::Stale {
            self.set_status(inner_status);
        }

        // Merge source with path composition
        match other.source() {
            ResponseSource::Backend(inner_label) => {
                // Compose: prefix.inner_label (e.g., "composition.moka")
                let composed = prefix.compose(inner_label);
                self.set_source(ResponseSource::Backend(composed));
            }
            ResponseSource::Upstream => {
                // No backend hit, keep as upstream
            }
        }
    }
}

/// Boxed context trait object using SmallBox for inline storage.
///
/// Uses SmallBox with S4 space (4 * usize = 32 bytes on 64-bit) to avoid
/// heap allocation for small contexts (like `CacheContext`). Larger contexts
/// (like `CompositionContext`) fall back to heap allocation automatically.
///
/// This optimization reduces allocation overhead in the common case
/// where only basic cache context tracking is needed.
pub type BoxContext = SmallBox<dyn Context, S4>;

/// Convert a BoxContext (SmallBox) into a CacheContext.
///
/// This function converts the SmallBox to a Box and then calls
/// `into_cache_context()`. The allocation happens only at the end
/// of the request lifecycle when the context is finalized.
pub fn finalize_context(ctx: BoxContext) -> CacheContext {
    let boxed: Box<dyn Context> = SmallBox::into_box(ctx);
    boxed.into_cache_context()
}

/// Context information about a cache operation.
#[derive(Debug, Clone, Default)]
pub struct CacheContext {
    /// Whether the request resulted in a cache hit, miss, or stale data.
    pub status: CacheStatus,
    /// Read mode for this operation.
    pub read_mode: ReadMode,
    /// Source of the response.
    pub source: ResponseSource,
}

impl CacheContext {
    /// Convert this context into a boxed trait object.
    ///
    /// This is a convenience method for creating `BoxContext` from `CacheContext`.
    /// Uses SmallBox for inline storage, avoiding heap allocation for small contexts.
    pub fn boxed(self) -> BoxContext {
        smallbox!(self)
    }
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

    fn read_mode(&self) -> ReadMode {
        self.read_mode
    }

    fn set_read_mode(&mut self, mode: ReadMode) {
        self.read_mode = mode;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> BoxContext {
        smallbox!(self.clone())
    }

    fn into_cache_context(self: Box<Self>) -> CacheContext {
        *self
    }
}

/// Extension trait for enriching responses with cache status information.
///
/// This trait provides a protocol-agnostic way to attach cache status
/// metadata to responses. Each protocol (HTTP, gRPC, etc.) implements
/// this trait with its own configuration type.
///
/// # Example
///
/// ```ignore
/// use hitbox_core::{CacheStatus, CacheStatusExt};
///
/// // For HTTP responses (implemented in hitbox-http)
/// response.cache_status(CacheStatus::Hit, &header_name);
/// ```
pub trait CacheStatusExt {
    /// Configuration type for applying cache status (e.g., header name for HTTP).
    type Config;

    /// Applies cache status information to the response.
    fn cache_status(&mut self, status: CacheStatus, config: &Self::Config);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_sizes() {
        use std::mem::size_of;
        let cache_ctx_size = size_of::<CacheContext>();
        let box_ctx_size = size_of::<BoxContext>();
        let s4_space = 4 * size_of::<usize>();

        println!("CacheContext size: {} bytes", cache_ctx_size);
        println!("  - CacheStatus: {} bytes", size_of::<CacheStatus>());
        println!("  - ResponseSource: {} bytes", size_of::<ResponseSource>());
        println!("BoxContext size: {} bytes", box_ctx_size);
        println!("S4 inline space: {} bytes", s4_space);

        // CacheContext should fit in S4 inline storage (32 bytes on 64-bit)
        assert!(
            cache_ctx_size <= s4_space,
            "CacheContext ({} bytes) should fit in S4 ({} bytes)",
            cache_ctx_size,
            s4_space
        );
    }
}
