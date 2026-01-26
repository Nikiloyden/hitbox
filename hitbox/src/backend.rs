//! Backend traits and utilities for cache storage.
//!
//! This module re-exports types from `hitbox-backend` for implementing
//! custom cache storage backends:
//!
//! - `Backend` - Core trait for cache storage operations
//! - `CacheBackend` - Extended trait with response-aware operations
//! - `BackendError` - Error type for backend operations
//! - `DeleteStatus` - Result of cache entry deletion
//!
//! ## Built-in Backends
//!
//! | Backend | Crate | Use Case |
//! |---------|-------|----------|
//! | Moka | [`hitbox-moka`] | In-memory, single instance |
//! | Redis | [`hitbox-redis`] | Distributed, multi-instance |
//! | FeOxDB | [`hitbox-feoxdb`] | Embedded persistent storage |
//!
//! See [`hitbox-backend`] documentation for implementing custom backends.
//!
//! [`hitbox-backend`]: https://docs.rs/hitbox-backend
//! [`hitbox-moka`]: https://docs.rs/hitbox-moka
//! [`hitbox-redis`]: https://docs.rs/hitbox-redis
//! [`hitbox-feoxdb`]: https://docs.rs/hitbox-feoxdb

pub use hitbox_backend::{Backend, BackendError, CacheBackend, DeleteStatus};
