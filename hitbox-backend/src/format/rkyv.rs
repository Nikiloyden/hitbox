use bytes::Bytes;
use hitbox_core::Raw;

use super::{Format, FormatDeserializer, FormatError, FormatSerializer, FormatTypeId};
use crate::BackendContext;

/// Rkyv format - high-performance zero-copy serialization
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
        _context: &dyn BackendContext,
    ) -> Result<Raw, FormatError> {
        // Create a pre-allocated buffer with the configured hint
        let buffer = rkyv::AlignedVec::with_capacity(self.buffer_hint);

        // Wrap the buffer in an AlignedSerializer
        let aligned_ser = rkyv::ser::serializers::AlignedSerializer::new(buffer);

        // Create an rkyv serializer with scratch space and shared pointer tracker
        let mut serializer = rkyv::ser::serializers::AllocSerializer::<256>::new(
            aligned_ser,
            Default::default(),
            Default::default(),
        );

        // Box it to create a trait object
        {
            let mut boxed: Box<dyn rkyv_dyn::DynSerializer> = Box::new(&mut serializer);
            let mut format_ser = FormatSerializer::Rkyv(&mut *boxed);

            // Call the closure with our serializer
            f(&mut format_ser)?;
        } // boxed is dropped here, releasing the borrow

        // Extract the serialized bytes
        let bytes = serializer.into_serializer().into_inner().to_vec();
        Ok(Bytes::from(bytes))
    }

    fn with_deserializer(
        &self,
        data: &[u8],
        f: &mut dyn FnMut(&mut FormatDeserializer) -> Result<(), FormatError>,
    ) -> Result<((), std::sync::Arc<dyn BackendContext>), FormatError> {
        // For rkyv, we just pass the archived bytes directly
        let mut format_deser = FormatDeserializer::Rkyv(data);
        f(&mut format_deser)?;
        Ok((
            (),
            std::sync::Arc::new(()) as std::sync::Arc<dyn BackendContext>,
        ))
    }

    fn clone_box(&self) -> Box<dyn Format> {
        Box::new(*self)
    }

    fn format_type_id(&self) -> FormatTypeId {
        FormatTypeId::Rkyv
    }
}
