use bytes::Bytes;
use hitbox_core::{Cacheable, Raw};
use thiserror::Error;

use hitbox_core::BoxContext;

use crate::Context;

// Bincode imports for concrete types (use absolute paths to avoid conflict with our bincode module)
use ::bincode::config::Configuration;
use ::bincode::de::DecoderImpl;
use ::bincode::de::read::SliceReader;
use ::bincode::enc::EncoderImpl;
use ::bincode::serde::Compat;
use ::bincode::{Decode, Encode};

// Import the BincodeVecWriter from bincode module
use self::bincode::BincodeVecWriter;

#[cfg(feature = "rkyv_format")]
use ::rkyv::Infallible;

mod bincode;
mod json;
#[cfg(feature = "rkyv_format")]
mod rkyv;
mod ron;

pub use bincode::BincodeFormat;
pub use json::JsonFormat;
#[cfg(feature = "rkyv_format")]
pub use rkyv::RkyvFormat;
pub use ron::RonFormat;

#[derive(Error, Debug)]
pub enum FormatError {
    #[error(transparent)]
    Serialize(Box<dyn std::error::Error + Send>),

    #[error(transparent)]
    Deserialize(Box<dyn std::error::Error + Send>),
}

/// Error type for rkyv serialization that preserves error information
/// when the actual error type cannot be directly boxed due to trait object constraints
#[cfg(feature = "rkyv_format")]
#[derive(Error, Debug)]
#[error("rkyv serialization failed: {message}")]
struct RkyvSerializeError {
    message: String,
}

#[cfg(feature = "rkyv_format")]
impl RkyvSerializeError {
    fn new(error: impl std::fmt::Debug) -> Self {
        Self {
            message: format!("{:?}", error),
        }
    }
}

/// Error type for rkyv validation that preserves error information
/// The underlying CheckArchiveError contains non-Send trait objects, so we
/// capture the error message as a Send-safe string
#[cfg(feature = "rkyv_format")]
#[derive(Error, Debug)]
#[error("rkyv validation failed: {message}")]
struct RkyvValidationError {
    message: String,
}

#[cfg(feature = "rkyv_format")]
impl RkyvValidationError {
    fn new(error: impl std::fmt::Display) -> Self {
        Self {
            message: error.to_string(),
        }
    }
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
    /// Serialize a value - handles serde, rkyv, and bincode based on serializer type
    pub fn serialize<T>(&mut self, value: &T) -> Result<(), FormatError>
    where
        T: Cacheable,
    {
        match self {
            FormatSerializer::Serde(ser) => {
                let erased_value = value as &dyn erased_serde::Serialize;
                erased_value
                    .erased_serialize(*ser)
                    .map_err(|e| FormatError::Serialize(Box::new(e)))
            }
            #[cfg(feature = "rkyv_format")]
            FormatSerializer::Rkyv(ser) => {
                let rkyv_value = value as &dyn rkyv_dyn::SerializeDyn;
                rkyv_value
                    .serialize_dyn(*ser)
                    .map(|_| ()) // Discard the position, return ()
                    .map_err(|e| {
                        // Use dedicated error type to preserve error information
                        FormatError::Serialize(Box::new(RkyvSerializeError::new(e)))
                    })
            }
            FormatSerializer::Bincode(enc) => {
                // Use Compat wrapper to bridge serde and bincode
                let compat = Compat(value);
                Encode::encode(&compat, enc).map_err(|e| FormatError::Serialize(Box::new(e)))
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
    /// Deserialize a value - handles serde, rkyv, and bincode based on deserializer type
    pub fn deserialize<T>(&mut self) -> Result<T, FormatError>
    where
        T: Cacheable,
    {
        #[cfg(feature = "rkyv_format")]
        use ::rkyv::Deserialize as _;

        match self {
            FormatDeserializer::Serde(deser) => {
                erased_serde::deserialize(*deser).map_err(|e| FormatError::Deserialize(Box::new(e)))
            }
            #[cfg(feature = "rkyv_format")]
            FormatDeserializer::Rkyv(data) => {
                // Safely validate and access the archived data
                // The CheckBytes bound on Cacheable ensures this is safe
                let archived = ::rkyv::check_archived_root::<T>(data)
                    .map_err(|e| FormatError::Deserialize(Box::new(RkyvValidationError::new(e))))?;

                // Deserialize from the validated archive
                // Note: With Infallible as the error type, this can never actually fail
                // The empty match on Infallible is exhaustive since it has no constructors
                let value: T = archived
                    .deserialize(&mut Infallible)
                    .map_err(|inf| match inf {})?;

                Ok(value)
            }
            FormatDeserializer::Bincode(dec) => {
                // Use Compat wrapper to decode from bincode
                let compat: Compat<T> =
                    Decode::decode(dec).map_err(|e| FormatError::Deserialize(Box::new(e)))?;
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
        context: &dyn Context,
    ) -> Result<Raw, FormatError>;

    /// Provides access to a deserializer via a callback to avoid lifetime issues.
    /// The context is passed by mutable reference and can be modified/upgraded during deserialization.
    fn with_deserializer(
        &self,
        data: &[u8],
        f: &mut dyn FnMut(&mut FormatDeserializer) -> Result<(), FormatError>,
        ctx: &mut BoxContext,
    ) -> Result<(), FormatError>;

    /// Clone this format into a box (for object safety)
    fn clone_box(&self) -> Box<dyn Format>;

    /// Returns a unique identifier for this format type.
    /// Used to compare format equality without knowing the concrete type.
    fn format_type_id(&self) -> FormatTypeId;
}

/// Extension trait providing generic serialize/deserialize methods
/// This is automatically implemented for all Format types
pub trait FormatExt: Format {
    fn serialize<T>(&self, value: &T, context: &dyn Context) -> Result<Raw, FormatError>
    where
        T: Cacheable,
    {
        self.with_serializer(&mut |serializer| serializer.serialize(value), context)
    }

    fn deserialize<T>(&self, data: &Raw, ctx: &mut BoxContext) -> Result<T, FormatError>
    where
        T: Cacheable,
    {
        let mut result: Option<T> = None;
        self.with_deserializer(data, &mut |deserializer| {
            let value: T = deserializer.deserialize()?;
            result = Some(value);
            Ok(())
        }, ctx)?;

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
        context: &dyn Context,
    ) -> Result<Raw, FormatError> {
        (**self).with_serializer(f, context)
    }

    fn with_deserializer(
        &self,
        data: &[u8],
        f: &mut dyn FnMut(&mut FormatDeserializer) -> Result<(), FormatError>,
        ctx: &mut BoxContext,
    ) -> Result<(), FormatError> {
        (**self).with_deserializer(data, f, ctx)
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
        context: &dyn Context,
    ) -> Result<Raw, FormatError> {
        (**self).with_serializer(f, context)
    }

    fn with_deserializer(
        &self,
        data: &[u8],
        f: &mut dyn FnMut(&mut FormatDeserializer) -> Result<(), FormatError>,
        ctx: &mut BoxContext,
    ) -> Result<(), FormatError> {
        (**self).with_deserializer(data, f, ctx)
    }

    fn clone_box(&self) -> Box<dyn Format> {
        (**self).clone_box()
    }

    fn format_type_id(&self) -> FormatTypeId {
        (**self).format_type_id()
    }
}
