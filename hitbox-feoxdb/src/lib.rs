#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod backend;
mod error;

pub use backend::{FeOxDbBackend, FeOxDbBackendBuilder};
pub use error::FeOxDbError;
