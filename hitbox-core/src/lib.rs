#![warn(missing_docs)]
//! # hitbox-core
//!
//! Core traits and types for the Hitbox asynchronous caching framework.
//!
//! This crate provides the foundational abstractions that make Hitbox
//! **protocol-agnostic** and **extensible**. It defines the core traits
//! that protocol-specific implementations (like `hitbox-http`) and
//! backend implementations (like `hitbox-redis`, `hitbox-moka`) must implement.
//!
//! ## Architecture
//!
//! Hitbox is built around a **Finite State Machine (FSM)** that orchestrates
//! cache operations. This crate provides the traits that the FSM uses to:
//!
//! - **Decide** what to cache ([`Predicate`])
//! - **Generate** cache keys ([`Extractor`])
//! - **Bridge** protocol types with cache ([`CacheableRequest`], [`CacheableResponse`])
//! - **Call** upstream services ([`Upstream`])
//! - **Execute** background tasks ([`Offload`])
//!
//! ## Feature Flags
//!
//! - `rkyv_format` - Enable rkyv zero-copy serialization support
//!

pub mod cacheable;
pub mod context;
pub mod extractor;
pub mod key;
pub mod label;
pub mod offload;
pub mod policy;
pub mod predicate;
pub mod request;
pub mod response;
pub mod upstream;
pub mod value;

pub use cacheable::Cacheable;
pub use context::{
    BoxContext, CacheContext, CacheStatus, Context, ReadMode, ResponseSource, finalize_context,
};
pub use extractor::Extractor;
pub use key::{CacheKey, KeyPart, KeyParts};
pub use label::BackendLabel;
pub use offload::{DisabledOffload, Offload};
pub use policy::{CachePolicy, EntityPolicyConfig};
pub use predicate::{And, Neutral, Not, Or, Predicate, PredicateExt, PredicateResult};
pub use request::{CacheablePolicyData, CacheableRequest, RequestCachePolicy};
pub use response::{CacheState, CacheableResponse, ResponseCachePolicy};
#[doc(hidden)]
pub use smallbox::space::S4;
#[doc(hidden)]
pub use smol_str::SmolStr;
pub use upstream::Upstream;
pub use value::CacheValue;

/// Raw byte data type used for serialized cache values.
/// Using `Bytes` provides efficient zero-copy cloning via reference counting.
pub type Raw = bytes::Bytes;
