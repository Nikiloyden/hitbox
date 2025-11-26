use bytes::Bytes;
use hitbox_core::{BoxContext, Raw};

use super::{Format, FormatDeserializer, FormatError, FormatSerializer, FormatTypeId};
use crate::Context;

// Newtype wrapper for Vec<u8> to implement bincode's Writer trait
// (bincode has an internal VecWriter but it's not publicly exported)
#[derive(Default)]
pub struct BincodeVecWriter(Vec<u8>);

impl BincodeVecWriter {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

impl ::bincode::enc::write::Writer for BincodeVecWriter {
    fn write(&mut self, bytes: &[u8]) -> Result<(), ::bincode::error::EncodeError> {
        self.0.extend_from_slice(bytes);
        Ok(())
    }
}

/// Bincode format
#[derive(Debug, Clone, Copy)]
pub struct BincodeFormat;

impl Format for BincodeFormat {
    fn with_serializer(
        &self,
        f: &mut dyn FnMut(&mut FormatSerializer) -> Result<(), FormatError>,
        _context: &dyn Context,
    ) -> Result<Raw, FormatError> {
        let writer = BincodeVecWriter::new();
        let config = ::bincode::config::standard();
        let mut encoder = ::bincode::enc::EncoderImpl::new(writer, config);

        // Create FormatSerializer::Bincode variant with concrete EncoderImpl
        let mut format_ser = FormatSerializer::Bincode(&mut encoder);
        f(&mut format_ser)?;

        // Extract the buffer from the encoder
        let buf = encoder.into_writer().into_vec();
        Ok(Bytes::from(buf))
    }

    fn with_deserializer(
        &self,
        data: &[u8],
        f: &mut dyn FnMut(&mut FormatDeserializer) -> Result<(), FormatError>,
        _ctx: &mut BoxContext,
    ) -> Result<(), FormatError> {
        use ::bincode::de::read::SliceReader;

        let reader = SliceReader::new(data);
        let config = ::bincode::config::standard();
        let context = (); // Context for the decoder
        let mut decoder = ::bincode::de::DecoderImpl::new(reader, config, context);

        // Create FormatDeserializer::Bincode variant with concrete DecoderImpl
        let mut format_deser = FormatDeserializer::Bincode(&mut decoder);
        f(&mut format_deser)?;

        Ok(())
    }

    fn clone_box(&self) -> Box<dyn Format> {
        Box::new(*self)
    }

    fn format_type_id(&self) -> FormatTypeId {
        FormatTypeId::Bincode
    }
}
