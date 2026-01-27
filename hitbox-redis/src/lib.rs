#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod backend;
pub mod error;

#[doc(inline)]
pub use crate::backend::{ConnectionMode, RedisBackend, RedisBackendBuilder, SingleConfig};

#[cfg(feature = "cluster")]
#[doc(inline)]
pub use crate::backend::ClusterConfig;
