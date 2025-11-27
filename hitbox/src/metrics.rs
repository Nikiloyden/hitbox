//! Metrics declaration and initialization.

#[cfg(feature = "metrics")]
use crate::context::{CacheContext, CacheStatus};

#[cfg(not(feature = "metrics"))]
use crate::context::CacheContext;

#[cfg(feature = "metrics")]
use lazy_static::lazy_static;

#[cfg(feature = "metrics")]
lazy_static! {
    /// Track number of cache hit events.
    pub static ref CACHE_HIT_COUNTER: &'static str = {
        metrics::describe_counter!(
            "cache_hit_count",
            "Total number of cache hit events by message and actor."
        );
        "cache_hit_count"
    };
    /// Track number of cache miss events.
    pub static ref CACHE_MISS_COUNTER: &'static str = {
        metrics::describe_counter!(
            "cache_miss_count",
            "Total number of cache miss events by message and actor."
        );
        "cache_miss_count"
    };
    /// Track number of cache stale events.
    pub static ref CACHE_STALE_COUNTER: &'static str = {
        metrics::describe_counter!(
            "cache_stale_count",
            "Total number of cache stale events by message and actor."
        );
        "cache_stale_count"
    };
    /// Metric of upstream message handling timings.
    pub static ref CACHE_UPSTREAM_HANDLING_HISTOGRAM: &'static str = {
        metrics::describe_histogram!(
            "cache_upstream_message_handling_duration_seconds",
            metrics::Unit::Seconds,
            "Cache upstream actor message handling latencies in seconds."
        );
        "cache_upstream_message_handling_duration_seconds"
    };

    // Offload manager metrics

    /// Track number of offload tasks spawned.
    pub static ref OFFLOAD_TASKS_SPAWNED: &'static str = {
        metrics::describe_counter!(
            "offload_tasks_spawned_total",
            "Total number of offload tasks spawned."
        );
        "offload_tasks_spawned_total"
    };
    /// Track number of offload tasks completed successfully.
    pub static ref OFFLOAD_TASKS_COMPLETED: &'static str = {
        metrics::describe_counter!(
            "offload_tasks_completed_total",
            "Total number of offload tasks completed successfully."
        );
        "offload_tasks_completed_total"
    };
    /// Track number of offload tasks that timed out.
    pub static ref OFFLOAD_TASKS_TIMEOUT: &'static str = {
        metrics::describe_counter!(
            "offload_tasks_timeout_total",
            "Total number of offload tasks that timed out."
        );
        "offload_tasks_timeout_total"
    };
    /// Track number of offload tasks deduplicated (skipped).
    pub static ref OFFLOAD_TASKS_DEDUPLICATED: &'static str = {
        metrics::describe_counter!(
            "offload_tasks_deduplicated_total",
            "Total number of offload tasks deduplicated (skipped because already in flight)."
        );
        "offload_tasks_deduplicated_total"
    };
    /// Gauge of currently active offload tasks.
    pub static ref OFFLOAD_TASKS_ACTIVE: &'static str = {
        metrics::describe_gauge!(
            "offload_tasks_active",
            "Number of currently active offload tasks."
        );
        "offload_tasks_active"
    };
    /// Histogram of offload task duration.
    pub static ref OFFLOAD_TASK_DURATION: &'static str = {
        metrics::describe_histogram!(
            "offload_task_duration_seconds",
            metrics::Unit::Seconds,
            "Duration of offload tasks in seconds."
        );
        "offload_task_duration_seconds"
    };
    /// Track number of offload revalidations completed.
    pub static ref OFFLOAD_REVALIDATION_COMPLETED: &'static str = {
        metrics::describe_counter!(
            "offload_revalidation_completed_total",
            "Total number of offload revalidation tasks completed."
        );
        "offload_revalidation_completed_total"
    };

    // Per-layer cache metrics

    /// Track cache backend reads per layer.
    pub static ref CACHE_LAYER_READS: &'static str = {
        metrics::describe_counter!(
            "cache_layer_reads_total",
            "Total number of cache reads per layer."
        );
        "cache_layer_reads_total"
    };
    /// Track cache backend writes per layer.
    pub static ref CACHE_LAYER_WRITES: &'static str = {
        metrics::describe_counter!(
            "cache_layer_writes_total",
            "Total number of cache writes per layer."
        );
        "cache_layer_writes_total"
    };
    /// Track bytes read per layer.
    pub static ref CACHE_LAYER_BYTES_READ: &'static str = {
        metrics::describe_counter!(
            "cache_layer_bytes_read_total",
            "Total bytes read from cache per layer."
        );
        "cache_layer_bytes_read_total"
    };
    /// Track bytes written per layer.
    pub static ref CACHE_LAYER_BYTES_WRITTEN: &'static str = {
        metrics::describe_counter!(
            "cache_layer_bytes_written_total",
            "Total bytes written to cache per layer."
        );
        "cache_layer_bytes_written_total"
    };
    /// Track read errors per layer.
    pub static ref CACHE_LAYER_READ_ERRORS: &'static str = {
        metrics::describe_counter!(
            "cache_layer_read_errors_total",
            "Total number of cache read errors per layer."
        );
        "cache_layer_read_errors_total"
    };
    /// Track write errors per layer.
    pub static ref CACHE_LAYER_WRITE_ERRORS: &'static str = {
        metrics::describe_counter!(
            "cache_layer_write_errors_total",
            "Total number of cache write errors per layer."
        );
        "cache_layer_write_errors_total"
    };
}

/// Record metrics from a CacheContext after a cache operation.
///
/// This helper extracts metrics from the context and records them
/// with appropriate labels per backend layer and operation type.
///
/// # Arguments
/// * `ctx` - The cache context containing operation results and metrics
/// * `operation` - Operation label (e.g., "request", "revalidate")
/// * `revalidate` - If true, also increments the revalidation completed counter
///
/// When the `metrics` feature is disabled, this function is a no-op
/// and will be eliminated by the compiler.
#[cfg(feature = "metrics")]
#[inline]
pub fn record_context_metrics(ctx: &CacheContext, operation: &str, revalidate: bool) {
    if revalidate {
        metrics::counter!(*OFFLOAD_REVALIDATION_COMPLETED).increment(1);
    }
    // Record cache status
    match ctx.status {
        CacheStatus::Hit => {
            metrics::counter!(*CACHE_HIT_COUNTER, "op" => operation.to_string()).increment(1);
        }
        CacheStatus::Miss => {
            metrics::counter!(*CACHE_MISS_COUNTER, "op" => operation.to_string()).increment(1);
        }
        CacheStatus::Stale => {
            metrics::counter!(*CACHE_STALE_COUNTER, "op" => operation.to_string()).increment(1);
        }
    }

    // Record per-layer metrics
    for (layer_name, layer_metrics) in &ctx.metrics.layers {
        let layer = layer_name.to_string();
        let op = operation.to_string();

        if layer_metrics.reads > 0 {
            metrics::counter!(
                *CACHE_LAYER_READS,
                "layer" => layer.clone(),
                "op" => op.clone()
            )
            .increment(layer_metrics.reads as u64);
        }

        if layer_metrics.writes > 0 {
            metrics::counter!(
                *CACHE_LAYER_WRITES,
                "layer" => layer.clone(),
                "op" => op.clone()
            )
            .increment(layer_metrics.writes as u64);
        }

        if layer_metrics.bytes_read > 0 {
            metrics::counter!(
                *CACHE_LAYER_BYTES_READ,
                "layer" => layer.clone(),
                "op" => op.clone()
            )
            .increment(layer_metrics.bytes_read);
        }

        if layer_metrics.bytes_written > 0 {
            metrics::counter!(
                *CACHE_LAYER_BYTES_WRITTEN,
                "layer" => layer.clone(),
                "op" => op.clone()
            )
            .increment(layer_metrics.bytes_written);
        }

        if layer_metrics.read_errors > 0 {
            metrics::counter!(
                *CACHE_LAYER_READ_ERRORS,
                "layer" => layer.clone(),
                "op" => op.clone()
            )
            .increment(layer_metrics.read_errors as u64);
        }

        if layer_metrics.write_errors > 0 {
            metrics::counter!(
                *CACHE_LAYER_WRITE_ERRORS,
                "layer" => layer,
                "op" => op
            )
            .increment(layer_metrics.write_errors as u64);
        }
    }
}

/// No-op version when metrics feature is disabled.
/// The compiler will eliminate this empty function call.
#[cfg(not(feature = "metrics"))]
#[inline]
pub fn record_context_metrics(_ctx: &CacheContext, _operation: &str, _revalidate: bool) {}
