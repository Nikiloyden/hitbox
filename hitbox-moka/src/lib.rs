#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod backend;
mod builder;
pub mod metrics;

pub use backend::MokaBackend;
pub use builder::{ByteCapacity, EntryCapacity, MokaBackendBuilder, NoCapacity};
pub use moka::policy::EvictionPolicy;
