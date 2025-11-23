use bytes::Bytes;
use hitbox_core::Raw;
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

use crate::BackendContext;

// Bincode imports for concrete types (use absolute paths to avoid conflict with our bincode module)
use ::bincode::{Encode, Decode};
use ::bincode::enc::EncoderImpl;
use ::bincode::de::DecoderImpl;
use ::bincode::de::read::SliceReader;
use ::bincode::config::Configuration;
use ::bincode::serde::Compat;

// Import the BincodeVecWriter from bincode module
use self::bincode::BincodeVecWriter;

#[cfg(feature = "rkyv_format")]
use ::rkyv::{Archive, Deserialize as RkyvDeserialize, archived_root, Infallible};

mod json;
mod bincode;
mod ron;
#[cfg(feature = "rkyv_format")]
mod rkyv;

pub use json::JsonFormat;
pub use bincode::BincodeFormat;
pub use ron::RonFormat;
#[cfg(feature = "rkyv_format")]
pub use rkyv::RkyvFormat;

#[derive(Error, Debug)]
pub enum FormatError {
    #[error(transparent)]
    Serialize(Box<dyn std::error::Error + Send>),

    #[error(transparent)]
    Deserialize(Box<dyn std::error::Error + Send>),
}

/// Unique identifier for format types, used to compare format equality
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatTypeId {
    Json,
    Bincode,
    Ron,
    Rkyv,
    /// For user-defined custom formats. The string should be a unique identifier.
    Custom(&'static str),
}

/// Unified serializer enum that can hold serde, rkyv, or bincode serializers
pub enum FormatSerializer<'a> {
    Serde(&'a mut dyn erased_serde::Serializer),
    #[cfg(feature = "rkyv_format")]
    Rkyv(&'a mut dyn rkyv_dyn::DynSerializer),
    Bincode(&'a mut EncoderImpl<BincodeVecWriter, Configuration>),
}

impl<'a> FormatSerializer<'a> {
    /// Serialize a value - handles serde and bincode
    #[cfg(not(feature = "rkyv_format"))]
    pub fn serialize<T>(&mut self, value: &T) -> Result<(), FormatError>
    where
        T: Serialize,
    {
        match self {
            FormatSerializer::Serde(ser) => {
                let erased_value = value as &dyn erased_serde::Serialize;
                erased_value.erased_serialize(*ser)
                    .map_err(|e| FormatError::Serialize(Box::new(e)))
            }
            FormatSerializer::Bincode(enc) => {
                // Use Compat wrapper to bridge serde and bincode
                let compat = Compat(value);
                Encode::encode(&compat, enc)
                    .map_err(|e| FormatError::Serialize(Box::new(e)))
            }
        }
    }

    /// Serialize a value - handles serde, rkyv, and bincode based on serializer type
    #[cfg(feature = "rkyv_format")]
    pub fn serialize<T>(&mut self, value: &T) -> Result<(), FormatError>
    where
        T: Serialize + rkyv_dyn::SerializeDyn,
    {
        match self {
            FormatSerializer::Serde(ser) => {
                let erased_value = value as &dyn erased_serde::Serialize;
                erased_value.erased_serialize(*ser)
                    .map_err(|e| FormatError::Serialize(Box::new(e)))
            }
            FormatSerializer::Rkyv(ser) => {
                let rkyv_value = value as &dyn rkyv_dyn::SerializeDyn;
                rkyv_value.serialize_dyn(*ser)
                    .map(|_| ())  // Discard the position, return ()
                    .map_err(|_e| FormatError::Serialize(Box::new(std::io::Error::other("rkyv serialization error"))))
            }
            FormatSerializer::Bincode(enc) => {
                // Use Compat wrapper to bridge serde and bincode
                let compat = Compat(value);
                Encode::encode(&compat, enc)
                    .map_err(|e| FormatError::Serialize(Box::new(e)))
            }
        }
    }
}

/// Unified deserializer enum that can hold serde, rkyv, or bincode deserializers
pub enum FormatDeserializer<'a> {
    Serde(&'a mut dyn erased_serde::Deserializer<'a>),
    #[cfg(feature = "rkyv_format")]
    Rkyv(&'a [u8]), // For rkyv, we just need access to the archived bytes
    Bincode(&'a mut DecoderImpl<SliceReader<'a>, Configuration, ()>),
}

impl<'a> FormatDeserializer<'a> {
    /// Deserialize a value - handles serde and bincode
    #[cfg(not(feature = "rkyv_format"))]
    pub fn deserialize<T>(&mut self) -> Result<T, FormatError>
    where
        T: DeserializeOwned,
    {
        match self {
            FormatDeserializer::Serde(deser) => {
                erased_serde::deserialize(*deser)
                    .map_err(|e| FormatError::Deserialize(Box::new(e)))
            }
            FormatDeserializer::Bincode(dec) => {
                // Use Compat wrapper to decode from bincode
                let compat: Compat<T> = Decode::decode(dec)
                    .map_err(|e| FormatError::Deserialize(Box::new(e)))?;
                Ok(compat.0)
            }
        }
    }

    /// Deserialize a value - handles serde, rkyv, and bincode based on deserializer type
    #[cfg(feature = "rkyv_format")]
    pub fn deserialize<T>(&mut self) -> Result<T, FormatError>
    where
        T: DeserializeOwned + Archive,
        T::Archived: RkyvDeserialize<T, Infallible>,
    {
        use ::rkyv::Deserialize as _;

        match self {
            FormatDeserializer::Serde(deser) => {
                erased_serde::deserialize(*deser)
                    .map_err(|e| FormatError::Deserialize(Box::new(e)))
            }
            FormatDeserializer::Rkyv(data) => {
                // Access the archived data
                let archived = unsafe { archived_root::<T>(data) };

                // Deserialize from the archive
                let value: T = archived.deserialize(&mut Infallible)
                    .map_err(|_e| FormatError::Deserialize(Box::new(std::io::Error::other("rkyv deserialization error"))))?;

                Ok(value)
            }
            FormatDeserializer::Bincode(dec) => {
                // Use Compat wrapper to decode from bincode
                let compat: Compat<T> = Decode::decode(dec)
                    .map_err(|e| FormatError::Deserialize(Box::new(e)))?;
                Ok(compat.0)
            }
        }
    }
}

/// Unified serialization interface that bridges serde and rkyv
/// This trait allows different serialization libraries to work with the same Format interface
pub trait FormatSerialize {
    /// Serialize the value to bytes
    fn serialize(&self) -> Result<Bytes, FormatError>;
}

/// Object-safe format trait (uses erased-serde for type erasure)
/// This trait can be used with `&dyn Format` for dynamic dispatch
pub trait Format: std::fmt::Debug + Send + Sync {
    /// Provides access to a serializer via a callback to avoid lifetime issues
    fn with_serializer(
        &self,
        f: &mut dyn FnMut(&mut FormatSerializer) -> Result<(), FormatError>,
        context: &dyn BackendContext,
    ) -> Result<Raw, FormatError>;

    /// Provides access to a deserializer via a callback to avoid lifetime issues
    /// Returns a tuple of (result, context) where context is extracted from the data
    fn with_deserializer(
        &self,
        data: &[u8],
        f: &mut dyn FnMut(&mut FormatDeserializer) -> Result<(), FormatError>,
    ) -> Result<((), std::sync::Arc<dyn BackendContext>), FormatError>;

    /// Clone this format into a box (for object safety)
    fn clone_box(&self) -> Box<dyn Format>;

    /// Returns a unique identifier for this format type.
    /// Used to compare format equality without knowing the concrete type.
    fn format_type_id(&self) -> FormatTypeId;
}

/// Extension trait providing generic serialize/deserialize methods
/// This is automatically implemented for all Format types
pub trait FormatExt: Format {
    #[cfg(not(feature = "rkyv_format"))]
    fn serialize<T>(&self, value: &T, context: &dyn BackendContext) -> Result<Raw, FormatError>
    where
        T: Serialize,
    {
        self.with_serializer(
            &mut |serializer| serializer.serialize(value),
            context,
        )
    }

    #[cfg(feature = "rkyv_format")]
    fn serialize<T>(&self, value: &T, context: &dyn BackendContext) -> Result<Raw, FormatError>
    where
        T: Serialize + rkyv_dyn::SerializeDyn,
    {
        self.with_serializer(
            &mut |serializer| serializer.serialize(value),
            context,
        )
    }

    #[cfg(not(feature = "rkyv_format"))]
    fn deserialize<T>(&self, data: &Raw) -> Result<T, FormatError>
    where
        T: DeserializeOwned,
    {
        let mut result: Option<T> = None;
        let (_, _context) = self.with_deserializer(data, &mut |deserializer| {
            let value: T = deserializer.deserialize()?;
            result = Some(value);
            Ok(())
        })?;

        result.ok_or_else(|| {
            FormatError::Deserialize(Box::new(std::io::Error::other(
                "deserialization produced no result",
            )))
        })
    }

    #[cfg(feature = "rkyv_format")]
    fn deserialize<T>(&self, data: &Raw) -> Result<T, FormatError>
    where
        T: DeserializeOwned + Archive,
        T::Archived: RkyvDeserialize<T, Infallible>,
    {
        let mut result: Option<T> = None;
        let (_, _context) = self.with_deserializer(data, &mut |deserializer| {
            let value: T = deserializer.deserialize()?;
            result = Some(value);
            Ok(())
        })?;

        result.ok_or_else(|| {
            FormatError::Deserialize(Box::new(std::io::Error::other(
                "deserialization produced no result",
            )))
        })
    }
}

// Blanket implementation: all Formats automatically get generic methods
impl<T: Format + ?Sized> FormatExt for T {}

// Implement Clone for Box<dyn Format>
impl Clone for Box<dyn Format> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

// Implement Format for Box<dyn Format>
impl Format for Box<dyn Format> {
    fn with_serializer(
        &self,
        f: &mut dyn FnMut(&mut FormatSerializer) -> Result<(), FormatError>,
        context: &dyn BackendContext,
    ) -> Result<Raw, FormatError> {
        (**self).with_serializer(f, context)
    }

    fn with_deserializer(
        &self,
        data: &[u8],
        f: &mut dyn FnMut(&mut FormatDeserializer) -> Result<(), FormatError>,
    ) -> Result<((), std::sync::Arc<dyn BackendContext>), FormatError> {
        (**self).with_deserializer(data, f)
    }

    fn clone_box(&self) -> Box<dyn Format> {
        (**self).clone_box()
    }

    fn format_type_id(&self) -> FormatTypeId {
        (**self).format_type_id()
    }
}

// Implement Format for Arc<dyn Format>
impl Format for std::sync::Arc<dyn Format> {
    fn with_serializer(
        &self,
        f: &mut dyn FnMut(&mut FormatSerializer) -> Result<(), FormatError>,
        context: &dyn BackendContext,
    ) -> Result<Raw, FormatError> {
        (**self).with_serializer(f, context)
    }

    fn with_deserializer(
        &self,
        data: &[u8],
        f: &mut dyn FnMut(&mut FormatDeserializer) -> Result<(), FormatError>,
    ) -> Result<((), std::sync::Arc<dyn BackendContext>), FormatError> {
        (**self).with_deserializer(data, f)
    }

    fn clone_box(&self) -> Box<dyn Format> {
        (**self).clone_box()
    }

    fn format_type_id(&self) -> FormatTypeId {
        (**self).format_type_id()
    }
}
