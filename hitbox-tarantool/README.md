# hitbox-tarantool

Tarantool cache backend for the Hitbox caching framework.

This crate provides a [`Backend`] implementation for [Tarantool](https://www.tarantool.io/),
enabling distributed caching with Tarantool's in-memory database.

## Overview

- **Distributed caching** - Share cache across multiple application instances
- **High performance** - Tarantool's in-memory storage for fast access
- **Persistence** - Optional disk persistence for data durability

## Usage

```rust
use hitbox_tarantool::Tarantool;

// Create a Tarantool backend
let backend = Tarantool::new(/* connection config */);
```

[`Backend`]: hitbox_backend::Backend
