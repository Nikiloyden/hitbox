//! Metrics declaration and initialization.

use std::time::Duration;

#[cfg(feature = "metrics")]
use crate::context::CacheContext;

#[cfg(not(feature = "metrics"))]
use crate::context::CacheContext;

#[cfg(feature = "metrics")]
use lazy_static::lazy_static;

#[cfg(feature = "metrics")]
lazy_static! {
    // Cache status metrics

    /// Track number of cache hit events.
    pub static ref CACHE_HIT_COUNTER: &'static str = {
        metrics::describe_counter!(
            "hitbox_cache_hit_total",
            "Total number of cache hit events."
        );
        "hitbox_cache_hit_total"
    };
    /// Track number of cache miss events.
    pub static ref CACHE_MISS_COUNTER: &'static str = {
        metrics::describe_counter!(
            "hitbox_cache_miss_total",
            "Total number of cache miss events."
        );
        "hitbox_cache_miss_total"
    };
    /// Track number of cache stale events.
    pub static ref CACHE_STALE_COUNTER: &'static str = {
        metrics::describe_counter!(
            "hitbox_cache_stale_total",
            "Total number of cache stale events."
        );
        "hitbox_cache_stale_total"
    };

    // Latency metrics

    /// Histogram of cache request duration.
    pub static ref CACHE_REQUEST_DURATION: &'static str = {
        metrics::describe_histogram!(
            "hitbox_request_duration_seconds",
            metrics::Unit::Seconds,
            "Duration of cache requests in seconds."
        );
        "hitbox_request_duration_seconds"
    };
    /// Metric of upstream message handling timings.
    pub static ref CACHE_UPSTREAM_HANDLING_HISTOGRAM: &'static str = {
        metrics::describe_histogram!(
            "hitbox_upstream_duration_seconds",
            metrics::Unit::Seconds,
            "Duration of upstream requests in seconds."
        );
        "hitbox_upstream_duration_seconds"
    };

    // Offload manager metrics

    /// Track number of offload tasks spawned.
    pub static ref OFFLOAD_TASKS_SPAWNED: &'static str = {
        metrics::describe_counter!(
            "hitbox_offload_tasks_spawned_total",
            "Total number of offload tasks spawned."
        );
        "hitbox_offload_tasks_spawned_total"
    };
    /// Track number of offload tasks completed successfully.
    pub static ref OFFLOAD_TASKS_COMPLETED: &'static str = {
        metrics::describe_counter!(
            "hitbox_offload_tasks_completed_total",
            "Total number of offload tasks completed successfully."
        );
        "hitbox_offload_tasks_completed_total"
    };
    /// Track number of offload tasks that timed out.
    pub static ref OFFLOAD_TASKS_TIMEOUT: &'static str = {
        metrics::describe_counter!(
            "hitbox_offload_tasks_timeout_total",
            "Total number of offload tasks that timed out."
        );
        "hitbox_offload_tasks_timeout_total"
    };
    /// Track number of offload tasks deduplicated (skipped).
    pub static ref OFFLOAD_TASKS_DEDUPLICATED: &'static str = {
        metrics::describe_counter!(
            "hitbox_offload_tasks_deduplicated_total",
            "Total number of offload tasks deduplicated (skipped because already in flight)."
        );
        "hitbox_offload_tasks_deduplicated_total"
    };
    /// Gauge of currently active offload tasks.
    pub static ref OFFLOAD_TASKS_ACTIVE: &'static str = {
        metrics::describe_gauge!(
            "hitbox_offload_tasks_active",
            "Number of currently active offload tasks."
        );
        "hitbox_offload_tasks_active"
    };
    /// Histogram of offload task duration.
    pub static ref OFFLOAD_TASK_DURATION: &'static str = {
        metrics::describe_histogram!(
            "hitbox_offload_task_duration_seconds",
            metrics::Unit::Seconds,
            "Duration of offload tasks in seconds."
        );
        "hitbox_offload_task_duration_seconds"
    };
    /// Track number of offload revalidations completed.
    pub static ref OFFLOAD_REVALIDATION_COMPLETED: &'static str = {
        metrics::describe_counter!(
            "hitbox_offload_revalidation_completed_total",
            "Total number of offload revalidation tasks completed."
        );
        "hitbox_offload_revalidation_completed_total"
    };

    // Per-backend cache metrics

    /// Track cache backend reads per backend.
    pub static ref CACHE_BACKEND_READS: &'static str = {
        metrics::describe_counter!(
            "hitbox_backend_reads_total",
            "Total number of cache reads per backend."
        );
        "hitbox_backend_reads_total"
    };
    /// Track cache backend writes per backend.
    pub static ref CACHE_BACKEND_WRITES: &'static str = {
        metrics::describe_counter!(
            "hitbox_backend_writes_total",
            "Total number of cache writes per backend."
        );
        "hitbox_backend_writes_total"
    };
    /// Track bytes read per backend.
    pub static ref CACHE_BACKEND_BYTES_READ: &'static str = {
        metrics::describe_counter!(
            "hitbox_backend_bytes_read_total",
            "Total bytes read from cache per backend."
        );
        "hitbox_backend_bytes_read_total"
    };
    /// Track bytes written per backend.
    pub static ref CACHE_BACKEND_BYTES_WRITTEN: &'static str = {
        metrics::describe_counter!(
            "hitbox_backend_bytes_written_total",
            "Total bytes written to cache per backend."
        );
        "hitbox_backend_bytes_written_total"
    };
    /// Track read errors per backend.
    pub static ref CACHE_BACKEND_READ_ERRORS: &'static str = {
        metrics::describe_counter!(
            "hitbox_backend_read_errors_total",
            "Total number of cache read errors per backend."
        );
        "hitbox_backend_read_errors_total"
    };
    /// Track write errors per backend.
    pub static ref CACHE_BACKEND_WRITE_ERRORS: &'static str = {
        metrics::describe_counter!(
            "hitbox_backend_write_errors_total",
            "Total number of cache write errors per backend."
        );
        "hitbox_backend_write_errors_total"
    };
}

/// Record metrics from a CacheContext after a cache operation.
///
/// This helper extracts metrics from the context and records them
/// with appropriate labels per backend and status.
///
/// # Arguments
/// * `ctx` - The cache context containing operation results
/// * `duration` - Duration of the cache request
/// * `revalidate` - If true, this was a background revalidation request
///
/// When the `metrics` feature is disabled, this function is a no-op
/// and will be eliminated by the compiler.
#[cfg(feature = "metrics")]
#[inline]
pub fn record_context_metrics(ctx: &CacheContext, duration: Duration, revalidate: bool) {
    let status = ctx.status.as_str();
    let backend = ctx.source.as_str();

    // Record request duration with status and backend labels
    metrics::histogram!(
        *CACHE_REQUEST_DURATION,
        "status" => status,
        "backend" => backend.to_string()
    )
    .record(duration.as_secs_f64());

    // Record cache status counter
    let counter = match ctx.status {
        crate::context::CacheStatus::Hit => *CACHE_HIT_COUNTER,
        crate::context::CacheStatus::Miss => *CACHE_MISS_COUNTER,
        crate::context::CacheStatus::Stale => *CACHE_STALE_COUNTER,
    };
    metrics::counter!(counter, "backend" => backend.to_string()).increment(1);

    // Record revalidation completion
    if revalidate {
        metrics::counter!(*OFFLOAD_REVALIDATION_COMPLETED).increment(1);
    }
}

/// No-op version when metrics feature is disabled.
/// The compiler will eliminate this empty function call.
#[cfg(not(feature = "metrics"))]
#[inline]
pub fn record_context_metrics(_ctx: &CacheContext, _duration: Duration, _revalidate: bool) {}
