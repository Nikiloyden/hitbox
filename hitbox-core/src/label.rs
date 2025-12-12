//! Backend label type for identifying cache backends.
//!
//! `BackendLabel` is a newtype wrapper around `SmolStr` that provides type safety
//! for backend identifiers used in metrics, source tracking, and composition.

use smol_str::SmolStr;
use std::fmt;

/// A label identifying a cache backend.
///
/// Used for:
/// - Backend identification in `Backend::label()`
/// - Response source tracking in `ResponseSource::Backend`
/// - Metrics labels for composed backends (e.g., "composition.moka")
///
/// # Example
/// ```
/// use hitbox_core::BackendLabel;
///
/// let label = BackendLabel::new("moka");
/// let composed = label.compose(&BackendLabel::new("inner"));
/// assert_eq!(composed.as_str(), "moka.inner");
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct BackendLabel(SmolStr);

impl BackendLabel {
    /// Creates a new backend label.
    #[inline]
    pub fn new(s: impl Into<SmolStr>) -> Self {
        Self(s.into())
    }

    /// Creates a backend label from a static string (no allocation).
    #[inline]
    pub const fn new_static(s: &'static str) -> Self {
        Self(SmolStr::new_static(s))
    }

    /// Returns the label as a string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns a reference to the inner `SmolStr`.
    #[inline]
    pub fn as_smol_str(&self) -> &SmolStr {
        &self.0
    }

    /// Composes two labels with a dot separator: "self.other".
    ///
    /// Used for hierarchical naming in composition backends,
    /// e.g., "composition.moka" or "outer.inner.redis".
    #[inline]
    pub fn compose(&self, other: &BackendLabel) -> Self {
        Self(SmolStr::from(format!("{}.{}", self.0, other.0)))
    }
}

impl fmt::Display for BackendLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<&str> for BackendLabel {
    #[inline]
    fn from(s: &str) -> Self {
        Self(SmolStr::new(s))
    }
}

impl From<String> for BackendLabel {
    #[inline]
    fn from(s: String) -> Self {
        Self(SmolStr::from(s))
    }
}

impl From<SmolStr> for BackendLabel {
    #[inline]
    fn from(s: SmolStr) -> Self {
        Self(s)
    }
}

impl AsRef<str> for BackendLabel {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let label = BackendLabel::new("moka");
        assert_eq!(label.as_str(), "moka");
    }

    #[test]
    fn test_new_static() {
        let label = BackendLabel::new_static("redis");
        assert_eq!(label.as_str(), "redis");
    }

    #[test]
    fn test_compose() {
        let outer = BackendLabel::new("composition");
        let inner = BackendLabel::new("moka");
        let composed = outer.compose(&inner);
        assert_eq!(composed.as_str(), "composition.moka");
    }

    #[test]
    fn test_compose_nested() {
        let outer = BackendLabel::new("outer");
        let inner = BackendLabel::new("inner");
        let leaf = BackendLabel::new("moka");

        let composed = outer.compose(&inner).compose(&leaf);
        assert_eq!(composed.as_str(), "outer.inner.moka");
    }

    #[test]
    fn test_from_str() {
        let label: BackendLabel = "test".into();
        assert_eq!(label.as_str(), "test");
    }

    #[test]
    fn test_display() {
        let label = BackendLabel::new("display_test");
        assert_eq!(format!("{}", label), "display_test");
    }

    #[test]
    fn test_equality() {
        let a = BackendLabel::new("same");
        let b = BackendLabel::new("same");
        let c = BackendLabel::new("different");

        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
