#![doc = include_str!("../README.md")]

pub mod backend;

#[doc(inline)]
pub use crate::backend::Tarantool;
