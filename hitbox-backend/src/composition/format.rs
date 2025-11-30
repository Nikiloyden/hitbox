//! Format implementation for CompositionBackend.
//!
//! This module contains `CompositionFormat` which handles multi-layer serialization
//! for the composition backend, packing data for both L1 and L2 layers.

use std::sync::Arc;

use bytes::Bytes;
use hitbox_core::{BoxContext, CacheValue, Raw, ReadMode};

use super::context::{CompositionContext, CompositionLayer, upgrade_context};
use super::envelope::CompositionEnvelope;
use crate::format::{Format, FormatDeserializer, FormatError, FormatSerializer, FormatTypeId};
use crate::{Compressor, Context};

/// Format implementation for CompositionBackend that handles multi-layer serialization.
///
/// This format serializes data in both formats and packs them together into a CompositionEnvelope.
/// On deserialization, it unpacks the CompositionEnvelope and deserializes from L1 if available, otherwise L2.
///
/// Each layer can have its own compression: L1 typically uses PassthroughCompressor (fast memory access),
/// while L2 can use GzipCompressor or ZstdCompressor (reduce network bandwidth).
#[derive(Debug, Clone)]
pub struct CompositionFormat {
    l1_format: Arc<dyn Format>,
    l2_format: Arc<dyn Format>,
    l1_compressor: Arc<dyn Compressor>,
    l2_compressor: Arc<dyn Compressor>,
}

impl CompositionFormat {
    /// Create a new CompositionFormat with separate formats and compressors for each layer.
    pub fn new(
        l1_format: Arc<dyn Format>,
        l2_format: Arc<dyn Format>,
        l1_compressor: Arc<dyn Compressor>,
        l2_compressor: Arc<dyn Compressor>,
    ) -> Self {
        CompositionFormat {
            l1_format,
            l2_format,
            l1_compressor,
            l2_compressor,
        }
    }

    /// Check if L1 and L2 formats are the same type.
    /// Returns true if both formats have the same FormatTypeId.
    fn same_format(&self) -> bool {
        self.l1_format.format_type_id() == self.l2_format.format_type_id()
    }
}

impl Format for CompositionFormat {
    fn with_serializer(
        &self,
        f: &mut dyn FnMut(&mut FormatSerializer) -> Result<(), FormatError>,
        context: &dyn Context,
    ) -> Result<Raw, FormatError> {
        // Check if this is a refill operation (writing L2 data back to L1)
        // CompositionFormat is low-level code that knows about CompositionContext
        if let Some(comp_ctx) = context.as_any().downcast_ref::<CompositionContext>()
            && comp_ctx.read_mode() == ReadMode::Refill
        {
            // For refill operations, create an L1-only envelope
            // This data came from L2, so serialize and compress it for L1 storage
            let l1_serialized = self.l1_format.with_serializer(f, context)?;
            let l1_compressed = self
                .l1_compressor
                .compress(&l1_serialized)
                .map_err(|e| FormatError::Serialize(Box::new(e)))?;
            let composition =
                CompositionEnvelope::L1(CacheValue::new(Bytes::from(l1_compressed), None, None));

            return composition
                .serialize()
                .map_err(|e| FormatError::Serialize(Box::new(e)));
        }

        // Normal write path: Create Both envelope with data for both layers
        // Serialize and compress for L1
        let l1_serialized = self.l1_format.with_serializer(f, context)?;
        let l1_compressed = self
            .l1_compressor
            .compress(&l1_serialized)
            .map_err(|e| FormatError::Serialize(Box::new(e)))?;

        // Serialize and compress for L2
        // If L1 and L2 use the same format, reuse the serialized data instead of serializing again
        let l2_serialized = if self.same_format() {
            l1_serialized.clone()
        } else {
            self.l2_format.with_serializer(f, context)?
        };
        let l2_compressed = self
            .l2_compressor
            .compress(&l2_serialized)
            .map_err(|e| FormatError::Serialize(Box::new(e)))?;

        // Pack both compressed values into CompositionEnvelope
        let composition = CompositionEnvelope::Both {
            l1: CacheValue::new(Bytes::from(l1_compressed), None, None),
            l2: CacheValue::new(Bytes::from(l2_compressed), None, None),
        };

        // Serialize the CompositionEnvelope using zero-copy repr(C) format
        composition
            .serialize()
            .map_err(|e| FormatError::Serialize(Box::new(e)))
    }

    fn with_deserializer(
        &self,
        data: &[u8],
        f: &mut dyn FnMut(&mut FormatDeserializer) -> Result<(), FormatError>,
        ctx: &mut BoxContext,
    ) -> Result<(), FormatError> {
        // Deserialize the CompositionEnvelope using zero-copy repr(C) format
        let composition = CompositionEnvelope::deserialize(data)
            .map_err(|e| FormatError::Deserialize(Box::new(e)))?;

        // Extract source, compressed data, format, and compressor from envelope type
        let (compressed_data, format, compressor, source): (
            &Bytes,
            &dyn Format,
            &dyn Compressor,
            CompositionLayer,
        ) = match &composition {
            CompositionEnvelope::L1(v) => (
                &v.data,
                &*self.l1_format,
                &*self.l1_compressor,
                CompositionLayer::L1,
            ),
            CompositionEnvelope::L2(v) => (
                &v.data,
                &*self.l2_format,
                &*self.l2_compressor,
                CompositionLayer::L2,
            ),
            CompositionEnvelope::Both { l1, .. } => (
                &l1.data,
                &*self.l1_format,
                &*self.l1_compressor,
                CompositionLayer::L1,
            ),
        };

        // Decompress the data
        let decompressed = compressor
            .decompress(compressed_data.as_ref())
            .map_err(|e| FormatError::Deserialize(Box::new(e)))?;

        // Use the appropriate format to deserialize the decompressed data
        format.with_deserializer(&decompressed, f, ctx)?;

        // Upgrade context to CompositionContext with source layer info
        upgrade_context(ctx, source, self.clone());

        Ok(())
    }

    fn clone_box(&self) -> Box<dyn Format> {
        Box::new(self.clone())
    }

    fn format_type_id(&self) -> FormatTypeId {
        // CompositionFormat is a custom format
        FormatTypeId::Custom("composition")
    }
}
