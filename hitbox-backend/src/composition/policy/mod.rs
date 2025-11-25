//! Policy traits and implementations for controlling composition backend behavior.
//!
//! This module defines the strategy patterns for read, write, refill, and delete operations
//! across multiple cache layers. Policies encapsulate the execution logic, allowing
//! different strategies to be implemented and composed.
//!
//! # Available Policies
//!
//! ## Read Policies
//! - [`SequentialReadPolicy`] - Try L1 first, then L2 on miss (default)
//! - [`RaceReadPolicy`] - Race L1 and L2, return first hit
//! - [`ParallelReadPolicy`] - Query both in parallel, prefer L1
//!
//! ## Write Policies
//! - [`SequentialWritePolicy`] - Write to L1, then L2 (write-through)
//! - [`OptimisticParallelWritePolicy`] - Write to both in parallel, succeed if ≥1 succeeds
//!
//! ## Refill Policies
//! - [`AlwaysRefill`] - Always populate L1 after L2 hit (default)
//! - [`NeverRefill`] - Never populate L1 after L2 hit
//!
//! ## Delete Policies (Future)
//! - `SequentialDeletePolicy` - Delete from L1, then L2
//! - `ParallelDeletePolicy` - Delete from both in parallel

pub mod builder;
pub mod read;
pub mod refill;
pub mod write;

// Re-export policy builder
pub use builder::CompositionPolicy;

// Re-export read policies
pub use read::{ParallelReadPolicy, RaceReadPolicy, ReadPolicy, SequentialReadPolicy};

// Re-export refill policies
pub use refill::{AlwaysRefill, NeverRefill, RefillPolicy};

// Re-export write policies
pub use write::{OptimisticParallelWritePolicy, SequentialWritePolicy, WritePolicy};
