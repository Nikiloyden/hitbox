//! Cache operation context.
//!
//! Re-exports [`Context`] and [`ReadMode`] from `hitbox-core`.
//!
//! In composition backends, context is wrapped in an internal `CompositionContext`
//! that tracks which cache layer (L1/L2) provided the data. This enables proper
//! refill behavior when data is found in L2 but missing from L1.

pub use hitbox_core::{Context, ReadMode};
