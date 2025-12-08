use bytes::Bytes;
use hitbox_core::{BoxContext, Raw};
use rkyv::util::AlignedVec;

use super::{Format, FormatDeserializer, FormatError, FormatSerializer, FormatTypeId};
use crate::Context;

/// Rkyv format - high-performance zero-copy serialization (rkyv 0.8)
///
/// # Buffer Allocation
///
/// RkyvFormat pre-allocates a buffer to reduce reallocations during serialization.
/// The buffer hint can be configured based on your typical payload size:
///
/// - Small payloads (<1KB): Use `RkyvFormat::new()` (default 4KB)
/// - Medium payloads (1KB-100KB): Use `RkyvFormat::with_buffer_hint(16384)` (16KB)
/// - Large payloads (>100KB): Use `RkyvFormat::with_buffer_hint(131072)` (128KB)
///
/// # Examples
///
/// ```
/// use hitbox_backend::RkyvFormat;
///
/// // Default buffer size (4KB)
/// let format = RkyvFormat::new();
///
/// // Custom buffer size for large payloads
/// let format = RkyvFormat::with_buffer_hint(128 * 1024);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct RkyvFormat {
    /// Initial buffer capacity hint in bytes.
    /// Helps reduce reallocations during serialization.
    buffer_hint: usize,
}

impl RkyvFormat {
    /// Default buffer size (4KB) - suitable for most small to medium payloads
    pub const DEFAULT_BUFFER_HINT: usize = 4096;

    /// Creates a new RkyvFormat with the default buffer hint (4KB)
    pub const fn new() -> Self {
        Self {
            buffer_hint: Self::DEFAULT_BUFFER_HINT,
        }
    }

    /// Creates a new RkyvFormat with a custom buffer hint
    ///
    /// # Arguments
    ///
    /// * `buffer_hint` - Initial buffer capacity in bytes
    ///
    /// # Examples
    ///
    /// ```
    /// use hitbox_backend::RkyvFormat;
    ///
    /// // For large payloads (128KB)
    /// let format = RkyvFormat::with_buffer_hint(128 * 1024);
    /// ```
    pub const fn with_buffer_hint(buffer_hint: usize) -> Self {
        Self { buffer_hint }
    }

    /// Returns the configured buffer hint
    pub const fn buffer_hint(&self) -> usize {
        self.buffer_hint
    }
}

impl Default for RkyvFormat {
    fn default() -> Self {
        Self::new()
    }
}

impl Format for RkyvFormat {
    fn with_serializer(
        &self,
        f: &mut dyn FnMut(&mut FormatSerializer) -> Result<(), FormatError>,
        _context: &dyn Context,
    ) -> Result<Raw, FormatError> {
        // Create a pre-allocated buffer with the configured hint
        let mut buffer = AlignedVec::with_capacity(self.buffer_hint);

        // Pass the buffer to the callback - serialization happens inside
        {
            let mut format_ser = FormatSerializer::Rkyv(&mut buffer);
            f(&mut format_ser)?;
        }

        // Convert to Bytes
        Ok(Bytes::from(buffer.into_vec()))
    }

    fn with_deserializer(
        &self,
        data: &[u8],
        f: &mut dyn FnMut(&mut FormatDeserializer) -> Result<(), FormatError>,
        _ctx: &mut BoxContext,
    ) -> Result<(), FormatError> {
        // For rkyv, we just pass the archived bytes directly
        let mut format_deserializer = FormatDeserializer::Rkyv(data);
        f(&mut format_deserializer)?;
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn Format> {
        Box::new(*self)
    }

    fn format_type_id(&self) -> FormatTypeId {
        FormatTypeId::Rkyv
    }
}
