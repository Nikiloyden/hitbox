#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]
//! Distributed cache backend for the Hitbox caching framework using Redis.
//!
//! This crate provides [`RedisBackend`], a distributed cache backend powered by
//! [redis-rs](https://github.com/redis-rs/redis-rs). It uses a multiplexed connection
//! for efficient async operations across concurrent requests.
//!
//! # Overview
//!
//! - **Distributed caching**: Share cache across multiple processes and hosts
//! - **Multiplexed connection**: Efficient connection reuse via [`ConnectionManager`]
//! - **Automatic TTL**: Entries expire using native Redis TTL mechanism
//! - **Lazy connection**: Connection established on first operation, not at construction
//!
//! # Quickstart
//!
//! ```no_run
//! use hitbox_redis::RedisBackend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a backend with default settings (localhost:6379)
//! let backend = RedisBackend::new()?;
//!
//! // Or configure with a builder
//! let backend = RedisBackend::builder()
//!     .server("redis://redis.example.com:6379/0")
//!     .build()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Redis Storage Schema
//!
//! Each cache entry is stored as a Redis Hash with the following structure:
//!
//! ```text
//! Key: <serialized CacheKey>
//! Fields:
//!   "d" → cached data (serialized, optionally compressed)
//!   "s" → stale timestamp in milliseconds (optional)
//! TTL:  Set via EXPIRE command based on CacheValue::expire
//! ```
//!
//! # Configuration
//!
//! | Option | Default | Description |
//! |--------|---------|-------------|
//! | `server` | `redis://127.0.0.1/` | Redis connection URL |
//! | `key_format` | [`Bitcode`] | Cache key serialization format |
//! | `value_format` | [`BincodeFormat`] | Value serialization format |
//! | `compressor` | [`PassthroughCompressor`] | Compression strategy |
//! | `label` | `"redis"` | Backend label for multi-tier composition |
//!
//! ## Serialization Formats
//!
//! The `value_format` option controls how cached data is serialized. Available formats
//! are provided by [`hitbox_backend::format`]:
//!
//! | Format | Speed | Size | Human-readable | Use case |
//! |--------|-------|------|----------------|----------|
//! | [`BincodeFormat`] | Fast | Compact | No | Production (default, recommended) |
//! | [`JsonFormat`] | Slow | Large | Partial* | Debugging, interoperability |
//! | [`RonFormat`] | Medium | Medium | Yes | Config files, debugging |
//! | [`RkyvFormat`] | Fastest | Compact | No | Zero-copy, max performance |
//!
//! *\* JSON serializes binary data as byte arrays `[104, 101, ...]`, not readable strings.*
//!
//! **Note:** [`RkyvFormat`] requires enabling the `rkyv_format` feature on `hitbox-backend`.
//!
//! ## Key Formats
//!
//! The `key_format` option controls how [`CacheKey`] values are serialized for Redis:
//!
//! | Format | Size | Human-readable | Use case |
//! |--------|------|----------------|----------|
//! | [`Bitcode`] | Compact | No | Production (default, recommended) |
//! | [`UrlEncoded`] | Larger | Yes | Debugging, CDN/HTTP integration |
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
//! For Redis backends, compression is often **recommended** since it reduces:
//!
//! - Network bandwidth between application and Redis
//! - Redis memory usage
//! - Storage costs for Redis persistence (RDB/AOF)
//!
//! Consider compression when cached values exceed ~1KB.
//!
//! # When to Use This Backend
//!
//! Use `RedisBackend` when you need:
//!
//! - **Distributed caching**: Share cache across multiple application instances
//! - **Persistence**: Survive application restarts (with Redis RDB/AOF)
//! - **Large capacity**: Store more data than fits in process memory
//!
//! Consider other backends when you need:
//!
//! - **Lowest latency**: Use [`hitbox-moka`] for in-process caching
//! - **Both**: Use multi-tier composition (Moka L1 + Redis L2)
//!
//! # Multi-Tier Composition
//!
//! `RedisBackend` works well as an L2 cache behind a fast in-memory L1:
//!
//! ```ignore
//! use hitbox_backend::composition::Compose;
//! use hitbox_moka::MokaBackend;
//! use hitbox_redis::RedisBackend;
//!
//! // Fast local cache (L1) backed by Redis (L2)
//! let l1 = MokaBackend::builder(10_000).build();
//! let l2 = RedisBackend::builder()
//!     .server("redis://redis.example.com:6379/")
//!     .build()?;
//!
//! let backend = l1.compose(l2, offload_manager);
//! ```
//!
//! # Performance Characteristics
//!
//! | Operation | Latency | Redis Commands |
//! |-----------|---------|----------------|
//! | `read` | ~0.5-2ms | `HMGET` + `PTTL` (pipelined) |
//! | `write` | ~0.5-2ms | `HSET` + `EXPIRE` (pipelined) |
//! | `remove` | ~0.5-2ms | `DEL` |
//!
//! All operations use Redis pipelining to minimize round trips.
//!
//! [`Bitcode`]: hitbox_backend::CacheKeyFormat::Bitcode
//! [`UrlEncoded`]: hitbox_backend::CacheKeyFormat::UrlEncoded
//! [`BincodeFormat`]: hitbox_backend::format::BincodeFormat
//! [`JsonFormat`]: hitbox_backend::format::JsonFormat
//! [`RonFormat`]: hitbox_backend::format::RonFormat
//! [`RkyvFormat`]: hitbox_backend::format::RkyvFormat
//! [`PassthroughCompressor`]: hitbox_backend::PassthroughCompressor
//! [`GzipCompressor`]: hitbox_backend::GzipCompressor
//! [`ZstdCompressor`]: hitbox_backend::ZstdCompressor
//! [`hitbox_backend::format`]: hitbox_backend::format
//! [`hitbox-moka`]: https://docs.rs/hitbox-moka
//! [`CacheKey`]: hitbox::CacheKey
//! [`ConnectionManager`]: redis::aio::ConnectionManager

pub mod backend;
pub mod error;

#[doc(inline)]
pub use crate::backend::{RedisBackend, RedisBackendBuilder};
