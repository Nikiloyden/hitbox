use bitcode::__private::{
    Buffer, Decoder, Encoder, Result, VariantDecoder, VariantEncoder, Vec, View,
};
use bitcode::{Decode, Encode};
use core::num::NonZeroUsize;
use smol_str::SmolStr;

#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CacheKey {
    parts: Vec<KeyPart>,
    version: u32,
    prefix: SmolStr,
}

impl Encode for CacheKey {
    type Encoder = CacheKeyEncoder;
}

impl<'de> Decode<'de> for CacheKey {
    type Decoder = CacheKeyDecoder<'de>;
}

#[derive(Default)]
pub struct CacheKeyEncoder {
    parts: <Vec<KeyPart> as Encode>::Encoder,
    version: <u32 as Encode>::Encoder,
    prefix: <str as Encode>::Encoder,
}

impl Encoder<CacheKey> for CacheKeyEncoder {
    fn encode(&mut self, value: &CacheKey) {
        self.parts.encode(&value.parts);
        self.version.encode(&value.version);
        self.prefix.encode(value.prefix.as_str());
    }
}

impl Buffer for CacheKeyEncoder {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.parts.collect_into(out);
        self.version.collect_into(out);
        self.prefix.collect_into(out);
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.parts.reserve(additional);
        self.version.reserve(additional);
        self.prefix.reserve(additional);
    }
}

#[derive(Default)]
pub struct CacheKeyDecoder<'de> {
    parts: <Vec<KeyPart> as Decode<'de>>::Decoder,
    version: <u32 as Decode<'de>>::Decoder,
    prefix: <&'de str as Decode<'de>>::Decoder,
}

impl<'de> View<'de> for CacheKeyDecoder<'de> {
    fn populate(&mut self, input: &mut &'de [u8], length: usize) -> Result<()> {
        self.parts.populate(input, length)?;
        self.version.populate(input, length)?;
        self.prefix.populate(input, length)?;
        Ok(())
    }
}

impl<'de> Decoder<'de, CacheKey> for CacheKeyDecoder<'de> {
    fn decode(&mut self) -> CacheKey {
        let prefix_str: &str = self.prefix.decode();
        CacheKey {
            parts: self.parts.decode(),
            version: self.version.decode(),
            prefix: SmolStr::new(prefix_str),
        }
    }
}

impl CacheKey {
    pub fn parts(&self) -> impl Iterator<Item = &KeyPart> {
        self.parts.iter()
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn new(prefix: impl Into<SmolStr>, version: u32, parts: Vec<KeyPart>) -> Self {
        CacheKey {
            parts,
            version,
            prefix: prefix.into(),
        }
    }

    pub fn from_str(key: &str, value: &str) -> Self {
        CacheKey {
            parts: vec![KeyPart::new(key, Some(value))],
            version: 0,
            prefix: SmolStr::default(),
        }
    }

    pub fn from_slice(parts: &[(&str, Option<&str>)]) -> Self {
        CacheKey {
            parts: parts
                .iter()
                .map(|(key, value)| KeyPart::new(key, *value))
                .collect(),
            version: 0,
            prefix: SmolStr::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct KeyPart {
    key: SmolStr,
    value: Option<SmolStr>,
}

impl Encode for KeyPart {
    type Encoder = KeyPartEncoder;
}

impl<'de> Decode<'de> for KeyPart {
    type Decoder = KeyPartDecoder<'de>;
}

/// Custom encoder for KeyPart that handles Option<SmolStr> without lifetime issues
#[derive(Default)]
pub struct KeyPartEncoder {
    key: <str as Encode>::Encoder,
    // Manual Option encoding: variant (0=None, 1=Some) + value
    value_variant: VariantEncoder<2>,
    value_str: <str as Encode>::Encoder,
}

impl Encoder<KeyPart> for KeyPartEncoder {
    fn encode(&mut self, value: &KeyPart) {
        self.key.encode(value.key.as_str());
        // Manually encode Option<SmolStr> as variant + str
        self.value_variant.encode(&(value.value.is_some() as u8));
        if let Some(ref v) = value.value {
            self.value_str.reserve(NonZeroUsize::MIN);
            self.value_str.encode(v.as_str());
        }
    }
}

impl Buffer for KeyPartEncoder {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.key.collect_into(out);
        self.value_variant.collect_into(out);
        self.value_str.collect_into(out);
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.key.reserve(additional);
        self.value_variant.reserve(additional);
        // Don't reserve for value_str - we don't know how many are Some
    }
}

#[derive(Default)]
pub struct KeyPartDecoder<'de> {
    key: <&'de str as Decode<'de>>::Decoder,
    value_variant: VariantDecoder<'de, 2, false>,
    value_str: <&'de str as Decode<'de>>::Decoder,
}

impl<'de> View<'de> for KeyPartDecoder<'de> {
    fn populate(&mut self, input: &mut &'de [u8], length: usize) -> Result<()> {
        self.key.populate(input, length)?;
        self.value_variant.populate(input, length)?;
        // Get the count of Some values from variant decoder
        let some_count = self.value_variant.length(1);
        self.value_str.populate(input, some_count)?;
        Ok(())
    }
}

impl<'de> Decoder<'de, KeyPart> for KeyPartDecoder<'de> {
    fn decode(&mut self) -> KeyPart {
        let key_str: &str = self.key.decode();
        let value = if self.value_variant.decode() != 0 {
            let value_str: &str = self.value_str.decode();
            Some(SmolStr::new(value_str))
        } else {
            None
        };
        KeyPart {
            key: SmolStr::new(key_str),
            value,
        }
    }
}

impl KeyPart {
    pub fn new<K: AsRef<str>, V: AsRef<str>>(key: K, value: Option<V>) -> Self {
        KeyPart {
            key: SmolStr::new(key),
            value: value.map(SmolStr::new),
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
}

#[derive(Debug)]
pub struct KeyParts<T: Sized> {
    subject: T,
    parts: Vec<KeyPart>,
}

impl<T> KeyParts<T> {
    pub fn new(subject: T) -> Self {
        KeyParts {
            subject,
            parts: Vec::new(),
        }
    }

    pub fn push(&mut self, part: KeyPart) {
        self.parts.push(part)
    }

    pub fn append(&mut self, parts: &mut Vec<KeyPart>) {
        self.parts.append(parts)
    }

    pub fn into_cache_key(self) -> (T, CacheKey) {
        (
            self.subject,
            CacheKey {
                version: 0,
                prefix: SmolStr::default(),
                parts: self.parts,
            },
        )
    }
}
