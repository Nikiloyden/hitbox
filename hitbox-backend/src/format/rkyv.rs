use bytes::Bytes;
use hitbox_core::Raw;

use crate::BackendContext;
use super::{Format, FormatDeserializer, FormatError, FormatSerializer, FormatTypeId};

/// Rkyv format - high-performance zero-copy serialization
#[derive(Debug, Clone, Copy)]
pub struct RkyvFormat;

impl Format for RkyvFormat {
    fn with_serializer(
        &self,
        f: &mut dyn FnMut(&mut FormatSerializer) -> Result<(), FormatError>,
        _context: &dyn BackendContext,
    ) -> Result<Raw, FormatError> {
        // Create an rkyv serializer
        let mut serializer = rkyv::ser::serializers::AllocSerializer::<0>::default();

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
        Ok(((), std::sync::Arc::new(()) as std::sync::Arc<dyn BackendContext>))
    }

    fn clone_box(&self) -> Box<dyn Format> {
        Box::new(*self)
    }

    fn format_type_id(&self) -> FormatTypeId {
        FormatTypeId::Rkyv
    }
}
