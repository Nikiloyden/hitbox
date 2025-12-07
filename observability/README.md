# Hitbox Observability Stack

Local development setup for tracing, metrics, and dashboards.

## Quick Start

```bash
# Start the observability stack
docker compose -f observability/docker-compose.yml up -d

# Run the observability example (includes tracing + metrics)
cargo run -p hitbox-examples --example observability --features observability

# Make some requests
curl http://localhost:3002/
curl http://localhost:3002/greet/world
curl http://localhost:3002/health

# View metrics
curl http://localhost:3002/metrics

# View traces in Jaeger
open http://localhost:16686

# View dashboards in Grafana
open http://localhost:3000  # Login: admin/admin
```

## Services

| Service | URL | Description |
|---------|-----|-------------|
| Jaeger UI | http://localhost:16686 | Distributed tracing visualization |
| Prometheus | http://localhost:9090 | Metrics storage and querying |
| Grafana | http://localhost:3000 | Dashboards (admin/admin) |

## Example Endpoints

The observability example runs on port 3002:

| Endpoint | TTL | Description |
|----------|-----|-------------|
| `GET /` | 60s | Root handler with long cache TTL |
| `GET /greet/{name}` | 10s | Greeting with path-based cache key |
| `GET /health` | disabled | Health check (caching disabled) |
| `GET /metrics` | - | Prometheus metrics endpoint |

## OTLP Endpoints

Your application should send traces to:

| Protocol | Endpoint |
|----------|----------|
| gRPC | `localhost:4317` |
| HTTP | `localhost:4318` |

## Hitbox Metrics

The following metrics are exposed:

| Metric | Type | Description |
|--------|------|-------------|
| `hitbox_cache_miss_total` | Counter | Cache misses |
| `hitbox_backend_read_total` | Counter | Backend read operations |
| `hitbox_backend_write_total` | Counter | Backend write operations |
| `hitbox_backend_write_bytes_total` | Counter | Bytes written to cache |
| `hitbox_backend_read_duration_seconds` | Histogram | Read latency |
| `hitbox_backend_write_duration_seconds` | Histogram | Write latency |
| `hitbox_upstream_duration_seconds` | Histogram | Upstream call latency |

## With Redis Backend

To include Redis for testing:

```bash
docker compose -f observability/docker-compose.yml --profile with-redis up -d
```

Redis will be available at `localhost:6379`.

## Stopping

```bash
docker compose -f observability/docker-compose.yml down

# To also remove volumes:
docker compose -f observability/docker-compose.yml down -v
```

## Prometheus Configuration

The default configuration scrapes the observability example at `host.docker.internal:3002/metrics`.

To configure additional targets, edit `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'hitbox-observability'
    static_configs:
      - targets: ['host.docker.internal:3002']
    metrics_path: '/metrics'
```

## Grafana Dashboards

A pre-configured Hitbox dashboard is included at `grafana/provisioning/dashboards/json/hitbox.json`.

Additional dashboards can be added to `grafana/provisioning/dashboards/json/`.
They will be automatically loaded on startup.
