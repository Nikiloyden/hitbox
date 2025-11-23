use bytes::Bytes;
use hitbox_core::Raw;

use crate::BackendContext;
use super::{Format, FormatDeserializer, FormatError, FormatSerializer, FormatTypeId};

/// RON (Rusty Object Notation) format - human-readable alternative to JSON
#[derive(Debug, Clone, Copy)]
pub struct RonFormat;

impl Format for RonFormat {
    fn with_serializer(
        &self,
        f: &mut dyn FnMut(&mut FormatSerializer) -> Result<(), FormatError>,
        _context: &dyn BackendContext,
    ) -> Result<Raw, FormatError> {
        // RON serializer writes to std::fmt::Write (String), unlike JSON which uses std::io::Write (Vec<u8>)
        let mut buf = String::new();
        {
            let mut ser = ron::ser::Serializer::new(&mut buf, None)
                .map_err(|error| FormatError::Serialize(Box::new(error)))?;
            let mut erased = <dyn erased_serde::Serializer>::erase(&mut ser);
            let mut format_ser = FormatSerializer::Serde(&mut erased);
            f(&mut format_ser)?;
        }
        Ok(Bytes::from(buf.into_bytes()))
    }

    fn with_deserializer(
        &self,
        data: &[u8],
        f: &mut dyn FnMut(&mut FormatDeserializer) -> Result<(), FormatError>,
    ) -> Result<((), std::sync::Arc<dyn BackendContext>), FormatError> {
        let s = std::str::from_utf8(data).map_err(|e| FormatError::Deserialize(Box::new(e)))?;
        let mut deser = ron::de::Deserializer::from_str(s)
            .map_err(|e| FormatError::Deserialize(Box::new(e)))?;
        let mut erased = <dyn erased_serde::Deserializer>::erase(&mut deser);
        let mut format_deser = FormatDeserializer::Serde(&mut erased);
        f(&mut format_deser)?;
        Ok(((), std::sync::Arc::new(()) as std::sync::Arc<dyn BackendContext>))
    }

    fn clone_box(&self) -> Box<dyn Format> {
        Box::new(*self)
    }

    fn format_type_id(&self) -> FormatTypeId {
        FormatTypeId::Ron
    }
}
