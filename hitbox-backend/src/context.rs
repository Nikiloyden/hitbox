//! Backend context for controlling high-level cache behavior.
//!
//! This module provides the `BackendContext` trait which allows backends to provide
//! hints and control how higher-level cache operations behave. The primary use case
//! is enabling refill operations in composition backends when using dynamic dispatch.

use std::any::Any;

/// Policy hints that control cache behavior.
///
/// These hints are provided by backends through `BackendContext` to inform
/// higher-level layers (like `CacheBackend`) how to handle the cached data.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BackendPolicy {
    /// Indicates that the value should be written back after being read and deserialized.
    ///
    /// This is used for cache refill operations in composition backends where data
    /// read from L2 should be populated into L1.
    pub write_after_read: bool,
}

/// Context provided by backend operations.
///
/// Backends can provide context alongside cached values to control how higher-level
/// operations handle the data. The context provides:
/// - Policy hints (via `policy()`)
/// - Type information for downcasting (via `as_any()`)
///
/// # Example
///
/// ```ignore
/// // Simple backend with no context
/// impl BackendContext for () {
///     fn policy(&self) -> BackendPolicy {
///         BackendPolicy::default()
///     }
/// }
///
/// // Composition backend with refill policy
/// struct CompositionContext {
///     policy: BackendPolicy,
///     // ... other fields
/// }
///
/// impl BackendContext for CompositionContext {
///     fn policy(&self) -> BackendPolicy {
///         self.policy
///     }
/// }
/// ```
pub trait BackendContext: Send + Sync {
    /// Returns the policy hints for this context.
    fn policy(&self) -> BackendPolicy;

    /// Returns a reference to self as `Any` for downcasting.
    ///
    /// This allows specific context types to be identified and accessed
    /// when needed (e.g., `CompositionContext` for special serialization).
    fn as_any(&self) -> &dyn Any;
}

/// Default context implementation for unit type.
///
/// The unit type `()` serves as a "no context" marker with default policy values.
/// This allows APIs to always accept `&dyn BackendContext` while simple cases
/// can just pass `&()`.
impl BackendContext for () {
    fn policy(&self) -> BackendPolicy {
        BackendPolicy::default()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy() {
        let policy = BackendPolicy::default();
        assert!(!policy.write_after_read);
    }

    #[test]
    fn test_unit_context() {
        let ctx = ();
        let policy = ctx.policy();
        assert_eq!(policy, BackendPolicy::default());
    }

    #[test]
    fn test_custom_policy() {
        let policy = BackendPolicy {
            write_after_read: true,
        };
        assert!(policy.write_after_read);
    }
}
