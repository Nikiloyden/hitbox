#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]
//! In-memory cache backend for the Hitbox caching framework.
//!
//! This crate provides [`MokaBackend`], a high-performance in-memory cache backend
//! powered by [Moka](https://github.com/moka-rs/moka). It offers automatic entry
//! expiration based on TTL values stored in each cache entry.
//!
//! # Overview
//!
//! - **High performance**: Lock-free concurrent access using Moka's async cache
//! - **Automatic expiration**: Entries expire based on their individual TTL values
//! - **Memory-bounded**: Configurable maximum capacity with LRU-like eviction
//! - **Zero network overhead**: All operations are in-process
//!
//! # Quickstart
//!
//! ```
//! use hitbox_moka::MokaBackend;
//!
//! // Create a backend with capacity for 10,000 entries
//! let backend = MokaBackend::builder(10_000).build();
//! ```
//!
//! # Memory Management
//!
//! The `max_capacity` parameter controls the maximum number of entries the cache
//! can hold. When the cache reaches capacity, the least recently used entries are
//! evicted to make room for new ones.
//!
//! Additionally, entries are automatically removed when their TTL expires. The
//! expiration is handled by Moka's internal eviction mechanism, which checks
//! expiration times during cache operations.
//!
//! # Configuration
//!
//! | Option | Default | Description |
//! |--------|---------|-------------|
//! | `max_capacity` | Required | Maximum number of entries |
//! | `key_format` | [`Bitcode`] | Cache key serialization format |
//! | `value_format` | [`JsonFormat`] | Value serialization format |
//! | `compressor` | [`PassthroughCompressor`] | Compression strategy |
//! | `label` | `"moka"` | Backend label for multi-tier composition |
//!
//! ## Serialization Formats
//!
//! The `value_format` option controls how cached data is serialized. Available formats
//! are provided by [`hitbox_backend::format`]:
//!
//! | Format | Speed | Size | Human-readable | Use case |
//! |--------|-------|------|----------------|----------|
//! | [`JsonFormat`] | Slow | Large | Yes | Debugging, interoperability |
//! | [`BincodeFormat`] | Fast | Compact | No | General purpose (recommended) |
//! | [`RonFormat`] | Medium | Medium | Yes | Config files, debugging |
//!
//! ## Compression Strategies
//!
//! The `compressor` option controls whether cached data is compressed. Available
//! compressors are provided by [`hitbox_backend`]:
//!
//! | Compressor | Ratio | Speed | Feature flag |
//! |------------|-------|-------|--------------|
//! | [`PassthroughCompressor`] | None | Fastest | — |
//! | [`GzipCompressor`] | Good | Medium | `gzip` |
//! | [`ZstdCompressor`] | Best | Fast | `zstd` |
//!
//! For in-memory caches, compression is typically **not recommended** since memory
//! access is fast and compression adds CPU overhead. Consider compression when:
//!
//! - Cached values are large (>10KB)
//! - Memory is constrained
//! - Composing with network backends (compress once, reuse across tiers)
//!
//! [`Bitcode`]: hitbox_backend::CacheKeyFormat::Bitcode
//! [`JsonFormat`]: hitbox_backend::format::JsonFormat
//! [`BincodeFormat`]: hitbox_backend::format::BincodeFormat
//! [`RonFormat`]: hitbox_backend::format::RonFormat
//! [`PassthroughCompressor`]: hitbox_backend::PassthroughCompressor
//! [`GzipCompressor`]: hitbox_backend::GzipCompressor
//! [`ZstdCompressor`]: hitbox_backend::ZstdCompressor
//! [`hitbox_backend::format`]: hitbox_backend::format
//!
//! # When to Use This Backend
//!
//! Use `MokaBackend` when you need:
//!
//! - **Single-instance caching**: Data doesn't need to be shared across processes
//! - **Low latency**: Sub-microsecond read/write operations
//! - **Automatic memory management**: LRU eviction prevents unbounded growth
//!
//! Consider other backends when you need:
//!
//! - **Distributed caching**: Use [`hitbox-redis`] instead
//! - **Persistence**: Use [`hitbox-feoxdb`] instead
//!
//! # Multi-Tier Composition
//!
//! `MokaBackend` works well as an L1 cache in front of slower backends:
//!
//! ```ignore
//! use hitbox_backend::composition::Compose;
//!
//! // Fast local cache (L1) backed by Redis (L2)
//! let l1 = MokaBackend::builder(10_000).build();
//! let l2 = RedisBackend::builder().server("redis://localhost/").build()?;
//!
//! let backend = l1.compose(l2, offload_manager);
//! ```
//!
//! [`hitbox-redis`]: https://docs.rs/hitbox-redis
//! [`hitbox-feoxdb`]: https://docs.rs/hitbox-feoxdb

mod backend;
mod builder;

pub use backend::MokaBackend;
pub use builder::MokaBackendBuilder;
