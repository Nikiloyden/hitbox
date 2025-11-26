use bytes::Bytes;
use hitbox_core::{BoxContext, Raw};

use super::{Format, FormatDeserializer, FormatError, FormatSerializer, FormatTypeId};
use crate::Context;

/// JSON format (default)
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonFormat;

impl Format for JsonFormat {
    fn with_serializer(
        &self,
        f: &mut dyn FnMut(&mut FormatSerializer) -> Result<(), FormatError>,
        _context: &dyn Context,
    ) -> Result<Raw, FormatError> {
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::new(&mut buf);
        let mut erased = <dyn erased_serde::Serializer>::erase(&mut ser);
        let mut format_ser = FormatSerializer::Serde(&mut erased);
        f(&mut format_ser)?;
        Ok(Bytes::from(buf))
    }

    fn with_deserializer(
        &self,
        data: &[u8],
        f: &mut dyn FnMut(&mut FormatDeserializer) -> Result<(), FormatError>,
        _ctx: &mut BoxContext,
    ) -> Result<(), FormatError> {
        let mut deser = serde_json::Deserializer::from_slice(data);
        let mut erased = <dyn erased_serde::Deserializer>::erase(&mut deser);
        let mut format_deser = FormatDeserializer::Serde(&mut erased);
        f(&mut format_deser)?;
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn Format> {
        Box::new(*self)
    }

    fn format_type_id(&self) -> FormatTypeId {
        FormatTypeId::Json
    }
}
