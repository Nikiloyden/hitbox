# hitbox-redis

Hitbox is an asynchronous caching framework supporting multiple backends and suitable for distributed and for single-machine applications.

hitbox-redis is Cache [Backend] implementation for Redis.

This crate uses [redis-rs] as base library for asynchronous interaction with redis nodes.
It uses one [MultiplexedConnection] for better connection utilisation.

## Example usage

```rust
use hitbox_redis::{RedisBackend, ConnectionMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Single-node Redis
    let backend = RedisBackend::builder()
        .connection(ConnectionMode::single("redis://127.0.0.1:6379/"))
        .build()?;

    // Or with cluster support (requires `cluster` feature)
    // let backend = RedisBackend::builder()
    //     .connection(ConnectionMode::cluster([
    //         "redis://node1:6379",
    //         "redis://node2:6379",
    //         "redis://node3:6379",
    //     ]))
    //     .build()?;

    Ok(())
}
```

[MultiplexedConnection]: https://docs.rs/redis/latest/redis/aio/struct.MultiplexedConnection.html
[Backend]: https://docs.rs/hitbox-backend/latest/hitbox_backend/trait.Backend.html
[redis-rs]: https://docs.rs/redis/
