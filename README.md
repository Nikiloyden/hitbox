# Hitbox

[![Build status](https://github.com/hit-box/hitbox/actions/workflows/CI.yml/badge.svg)](https://github.com/hit-box/hitbox/actions?query=workflow)
[![Coverage Status](https://codecov.io/gh/hit-box/hitbox/branch/main/graph/badge.svg?token=tgAm8OBLkY)](https://codecov.io/gh/hit-box/hitbox)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Highly customizable async caching framework for Rust designed for high-performance applications.

Protocol-agnostic async core + first-class HTTP support via hitbox-http. Pluggable backends from in-memory to distributed solutions such as Redis. Built on tower, works with any tokio-based service.

- [Features](#features)
- [Motivation](#motivation)
- [Quick Start](#quick-start)
- [Project Structure](#project-structure)
- [License](#license)

## Features

### Core Features
- [Stale Cache](#stale-cache) Serve stale data within grace period while revalidating in background
- [Dogpile Prevention](#dogpile-prevention) Configurable concurrency limit with broadcast to prevent redundant upstream calls
- [Pluggable Backends](#pluggable-backends) Choose Moka, Redis, FeOxDB, or implement the Backend trait for your own storage
- [Composable Backends](#composable-backends) Combine backends into L1/L2/L3 tiers for optimal performance
- [Serialization](#serialization) Choose between bincode, rkyv, JSON, or RON formats
- [Compression](#compression) Reduce storage size with zstd or gzip compression
- [Observability](#observability) Track cache status, latency, backend I/O, and offload tasks
- [Predicate and Extractor Traits](#predicate-and-extractor-traits) Protocol-agnostic traits to control caching and generate cache keys

### HTTP Caching Features
- [HTTP Predicates](#http-predicates) Control caching with rules based on any part of request or response, including body
- [HTTP Extractors](#http-extractors) Automatically generate cache keys from request components
- [Framework Integration](#framework-integration) Works with Axum, Reqwest, and any tower-based framework or reqwest-middleware client
- [YAML Configuration](#yaml-configuration) Define entire caching setup in a configuration file

---

## Stale Cache

Stale cache enables serving outdated data while refreshing in the background. Each cache entry has a TTL (fresh period) and an optional stale window (grace period). During TTL, data is fresh. After TTL but within the stale window, data is stale but servable. Three policies control stale behavior: `Return` serves stale data immediately without revalidation, `Revalidate` blocks until fresh data is fetched, and `OffloadRevalidate` serves stale data immediately while refreshing in the background (Stale-While-Revalidate pattern). The OffloadManager handles background revalidation with task deduplication, configurable timeouts (none, cancel, or warn), and metrics tracking.

### OffloadManager Configuration

| Option | Description | Default |
|--------|-------------|---------|
| `max_concurrent_tasks` | Limit parallel background tasks | Unlimited |
| `timeout_policy` | `None`, `Cancel(duration)`, or `Warn(duration)` | `None` |
| `deduplicate` | Prevent duplicate revalidation for same key | `true` |

**Code example**

```rust
// Configure how background tasks are executed
let offload_config = OffloadConfig::builder()
    .max_concurrent_tasks(10)
    .timeout(Duration::from_secs(30))
    .deduplicate(true)
    .build();

let manager = OffloadManager::new(offload_config);

// Configure stale policy via PolicyConfig
let policy = PolicyConfig::builder()
    .ttl(Duration::from_secs(60))
    .stale(Duration::from_secs(300))
    .stale_policy(StalePolicy::OffloadRevalidate)
    .build();

// Add manager to Cache via .offload()
let cache = Cache::builder()
    .backend(backend)
    .config(policy)
    .offload(manager)
    .build();
```

## Dogpile Prevention

When a cache entry expires or is missing, multiple simultaneous requests can trigger redundant upstream calls - this is the "dogpile" or "thundering herd" problem.

Hitbox uses a configurable concurrency limit per cache key:
- First N requests (where N = concurrency limit) proceed to upstream
- Additional requests subscribe to a broadcast channel and wait
- When any request completes, it broadcasts the result to all waiters
- Waiters receive the response without calling upstream

With `concurrency: 1`, only one request fetches from upstream while others wait. But if that single request is slow, all waiting requests become slow too. Setting `concurrency: 2` or higher allows parallel fetches - the first to complete broadcasts to all waiters, reducing the impact of slow upstream responses.

**Code example**

```rust
// Create concurrency manager for dogpile prevention
let concurrency_manager = BroadcastConcurrencyManager::<Response>::new();

// Configure policy with concurrency limit
let policy = PolicyConfig::builder()
    .ttl(Duration::from_secs(60))
    .stale(Duration::from_secs(300))
    .concurrency(1)  // Only one request fetches, others wait
    .build();

// Add concurrency manager to Cache
let cache = Cache::builder()
    .backend(backend)
    .config(policy)
    .concurrency_manager(concurrency_manager)
    .build();
```

## Pluggable Backends

Backends store cached data. Each backend implements the `Backend` trait with `read`, `write`, and `remove` operations. All backends support configurable serialization format (Bincode, JSON, RON, Rkyv), key format (Bitcode, UrlEncoded), compression (Gzip, Zstd), and custom naming for metrics. Implement the `Backend` trait to add your own storage.

| Backend | Type | Configuration |
|---------|------|---------------|
| Moka | In-memory | `max_capacity` |
| Redis | Distributed | `server` (connection string) |
| FeOxDB | Embedded | `path` or `in_memory()` |

**Code example**

```rust
// Moka (in-memory)
let moka = MokaBackend::builder(10_000)
    .value_format(BincodeFormat)
    .compressor(ZstdCompressor::default())
    .build();

// Redis (distributed)
let redis = RedisBackend::builder()
    .server("redis://127.0.0.1/")
    .value_format(BincodeFormat)
    .build()?;

// FeOxDB (embedded persistent)
let feoxdb = FeOxDbBackend::builder()
    .path("/tmp/cache".into())
    .build()?;

// ...

// Use backend with Cache
let cache = Cache::builder()
    .backend(moka)
    .config(policy)
    .build();
```

## Composable Backends

Compose multiple backends into tiered cache hierarchies, similar to CPU cache levels. Each composition has configurable read, write, and refill policies. Read policies control how layers are queried: Sequential (L1 first, then L2), Race (first hit wins), or Parallel (both queried, prefer fresher). Write policies control how data propagates: Sequential (L1 then L2), OptimisticParallel (both in parallel, succeed if any succeeds), or Race (first success wins). Refill policies control L1 population on L2 hits: Always or Never. Compositions can be nested for L1/L2/L3 hierarchies.

| Policy Type | Options | Default |
|-------------|---------|---------|
| Read | Sequential, Race, Parallel | Sequential |
| Write | Sequential, OptimisticParallel, Race | OptimisticParallel |
| Refill | Always, Never | Never |

**Code example**

```rust
// L1 (Moka) + L2 (Redis) composition
let offload = OffloadManager::with_defaults();
let backend = moka.compose(redis, offload);

// With custom policies
let composition_policy = CompositionPolicy::new()
    .read(RaceReadPolicy::new())
    .write(SequentialWritePolicy::new())
    .refill(RefillPolicy::Always);

let backend = moka.compose_with(redis, offload, composition_policy);

// ...

// Use composed backend with Cache
let cache = Cache::builder()
    .backend(backend)
    .config(policy)
    .build();
```

## Serialization

Hitbox supports multiple serialization formats via the `Format` trait. Bincode offers the best write throughput and excellent read performance. Rkyv provides zero-copy deserialization for maximum read speed but slower writes. RON balances performance with human-readable output. JSON is slowest but useful for debugging. Choose based on your read/write ratio and debugging needs.

## Compression

Reduce cache storage size with optional compression via the `Compressor` trait. Zstd provides excellent compression ratio with fast decompression (levels -7 to 22, default 3). Gzip offers wide compatibility (levels 0-9, default 6). Both require feature flags (`zstd`, `gzip`). Skip compression for small payloads where overhead exceeds savings.

## Observability

Track cache performance with built-in metrics (via `metrics` crate):

- **Cache status** — hit, miss, and stale counters per backend
- **Latency histograms** — request duration and upstream call timing
- **Backend I/O** — reads, writes, bytes transferred, and errors per backend
- **Offload tasks** — spawned, completed, deduplicated, active, and timed out

Integrates with any `metrics`-compatible exporter (Prometheus, StatsD, etc.).

## Predicate and Extractor Traits

Hitbox provides two protocol-agnostic traits for extending caching to any protocol (GraphQL, gRPC, PostgreSQL wire protocol, etc.):

- **Predicate** controls what gets cached. Implement `check` to return `Cacheable` or `NonCacheable` for your request/response type.
- **Extractor** generates cache keys. Implement `get` to extract key components from requests. Multiple extractors can be chained, each contributing parts to the final key.

Hitbox provides a complete HTTP implementation in `hitbox-http`—use it as a reference when implementing your own protocol support.

## HTTP Predicates

HTTP Predicates control what gets cached. Request predicates filter incoming requests by method, path, headers, query parameters, and body. Response predicates filter upstream responses by status code, headers, and body. Both support operations like equality, existence, list matching, containment, and regex. Body predicates additionally support size limits and JQ expressions for JSON filtering. Combine predicates with AND (chaining), OR, and NOT logic.

**Code example**

```rust
use http::{Method, StatusCode, header::CACHE_CONTROL};
use hitbox_http::predicates::{
    request::Method as RequestMethod,
    response::StatusCode as ResponseStatusCode,
    header::{Header as RequestHeader, Operation as HeaderOperation},
};

// Request predicate: cache GET requests to /api/authors/{id}, skip if Cache-Control: no-cache
RequestMethod::new(Method::GET)
    .unwrap()
    .path("/api/authors/{id}".to_string())
    .and(
        RequestHeader::new(HeaderOperation::Contains(
            CACHE_CONTROL,
            "no-cache".to_string(),
        )).not()
    )

// Response predicate: only cache successful responses
ResponseStatusCode::new(StatusCode::OK)
```

## HTTP Extractors

HTTP Extractors build cache keys from request components. They extract values from method, path parameters (using `{param}` patterns), headers, query parameters, and body content. Headers and query parameters support exact name matching or prefix-based selection, with optional regex extraction for partial values. Body extraction supports full-body hashing, JQ expressions for JSON (with a custom `hash` function), and regex with named capture groups. Extracted values can be transformed using hash (SHA256), lowercase, or uppercase. Extractors chain together, combining all extracted parts into a single cache key.

**Code example**

```rust
use hitbox_http::extractors::{
    Method as MethodExtractor,
    Path as PathExtractor,
    query::QueryExtractor,
    header::HeaderExtractor,
};

// Extract method, path params, query params, and headers for cache key
MethodExtractor::new()
    .path("/v1/authors/{author_id}/books/{book_id}")
    .query("page".to_string())
    .header("Accept-Language".to_string())
```

A request to `/v1/authors/123/books/456?page=1` with `Accept-Language: en` produces a cache key with `method`, `author_id`, `book_id`, `page`, and `Accept-Language` components.

## Framework Integration

Hitbox integrates with both server-side frameworks and HTTP clients.

**Servers**

| Framework | Support |
|-----------|---------|
| Axum | Full support via tower layer |

Since Hitbox is built on tower, it integrates as a standard layer without framework-specific code. Any tower-compatible framework can use Hitbox.

**Clients**

| Client | Support |
|--------|---------|
| Reqwest | Full support via reqwest-middleware |

Cache responses from external APIs using `hitbox-reqwest`. Works with `reqwest-middleware` for seamless integration.

## YAML Configuration

Define your entire caching setup in a configuration file:

```yaml
backend:
  type: Moka
  max_capacity: 10000
  key:
    format: Bitcode
  value:
    format: Bincode
    compression:
      type: Zstd
      level: 3

request:
  - Method: GET
  - Path: "/api/authors/{author_id}"

response:
  - Status: Success

extractors:
  - Method:
  - Path: "/api/authors/{author_id}"
  - Query: page

policy:
  Enabled:
    ttl: 60
    stale: 300
    policy:
      stale: OffloadRevalidate
    concurrency: 1
```

Change caching rules at runtime - no recompilation needed.

---

## Motivation

Every real-world system brings a combination of shared challenges and unique constraints. We tried using existing caching frameworks for our services, but each time they failed to fully match our requirements. As a result, we repeatedly ended up building custom caching mechanisms from scratch instead of relying on ready-made solutions.

Hitbox was created to break this cycle.

We think of Hitbox not as a library, but as a platform for caching, designed from day one to be easily extensible without enforcing a single backend, protocol, or caching strategy. New storage backends, new protocols such as GraphQL or even the PostgreSQL protocol, as well as new serialization or compression strategies, are expected use cases - not special exceptions. You simply implement the required traits and go for it. Hitbox is built to be hacked, extended, bent, and reshaped.

A key principle of Hitbox is that every new integration automatically inherits the full set of advanced optimizations we built through real-world experience: dogpile-effect prevention, composable multi-layer caching (L1/L2/L3), offload caching, and more. Instead of re-implementing these mechanisms for every project, they come for free with the platform.

At the same time, Hitbox is not just an abstract foundation. It already provides a production-ready HTTP caching implementation based on [tower::Service](https://docs.rs/tower/latest/tower/trait.Service.html), covering the most common use case out of the box while also serving as a reference implementation for building additional integrations.

---

## Quick Start

### Installation

```toml
[dependencies]
hitbox = { version = "0.1", features = ["moka"] }
hitbox-tower = "0.1"
hitbox-configuration = "0.1"
```

### Basic Usage

```rust
use std::time::Duration;
use axum::{Router, routing::get, extract::Path};
use hitbox::policy::PolicyConfig;
use hitbox_configuration::Endpoint;
use hitbox_moka::MokaBackend;
use hitbox_tower::Cache;

// Handlers
async fn get_users() -> &'static str { "users list" }
async fn get_user(Path(id): Path<String>) -> String { format!("user {id}") }

// Create backend
let backend = MokaBackend::builder(10_000).build();

// Users list - long TTL (60s)
let users_config = Endpoint::builder()
    .policy(
        PolicyConfig::builder()
            .ttl(Duration::from_secs(60))
            .stale(Duration::from_secs(30))
            .build()
    )
    .build();

// Single user - short TTL (10s)
let user_config = Endpoint::builder()
    .policy(
        PolicyConfig::builder()
            .ttl(Duration::from_secs(10))
            .stale(Duration::from_secs(5))
            .build()
    )
    .build();

// Build cache layers
let users_cache = Cache::builder()
    .backend(backend.clone())
    .config(users_config)
    .build();

let user_cache = Cache::builder()
    .backend(backend)
    .config(user_config)
    .build();

// Router with per-route cache layers
let app = Router::new()
    .route("/api/users", get(get_users).layer(users_cache))
    .route("/api/users/{id}", get(get_user).layer(user_cache));
```

### What's Next

- [Predicates](#predicates) - Control what gets cached
- [Stale Cache](#stale-cache) - Configure TTL and background revalidation
- [Composable Backends](#composable-backends) - Add Redis as L2
- [Examples](./examples) - More complete examples

## Project Structure

| Crate | Description |
|-------|-------------|
| `hitbox` | Main crate with policy configuration, stale cache, and feature flags for backends |
| `hitbox-core` | Core traits (`Predicate`, `Extractor`) and types for protocol-agnostic caching |
| `hitbox-backend` | `Backend` trait and utilities for implementing storage backends |
| `hitbox-http` | HTTP-specific predicates and extractors for request/response caching |
| `hitbox-tower` | Tower middleware integration (`Cache` layer) for server-side caching |
| `hitbox-configuration` | YAML/file-based configuration support |
| `hitbox-moka` | In-memory backend using [Moka](https://github.com/moka-rs/moka) |
| `hitbox-redis` | Distributed backend using Redis |
| `hitbox-feoxdb` | Embedded persistent backend using FeOxDB |
| `hitbox-reqwest` | Client-side caching for [reqwest](https://github.com/seanmonstar/reqwest) via reqwest-middleware |

## License

This project is licensed under the [MIT license](LICENSE).
