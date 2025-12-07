//! Tracing utilities for testing FSM state transitions.
//!
//! This module provides utilities to capture and assert on tracing spans
//! during tests, replacing the old `DebugState` system.

use std::sync::{Arc, Mutex};

use tracing::Dispatch;
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

/// Captured span information for testing.
#[derive(Debug, Clone)]
pub struct CapturedSpan {
    /// Unique span ID
    pub id: u64,
    /// Parent span ID (if any)
    pub parent_id: Option<u64>,
    /// The span name (e.g., "fsm.PollCache")
    pub name: String,
    /// The span target (e.g., "hitbox::fsm::states")
    pub target: String,
    /// Captured field values as strings
    pub fields: Vec<(String, String)>,
}

/// A tracing layer that captures span information for testing.
pub struct SpanCaptureLayer {
    spans: Arc<Mutex<Vec<CapturedSpan>>>,
}

/// Visitor to capture span field values.
struct FieldVisitor {
    fields: Vec<(String, String)>,
}

impl FieldVisitor {
    fn new() -> Self {
        Self { fields: Vec::new() }
    }
}

impl tracing::field::Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields
            .push((field.name().to_string(), format!("{:?}", value)));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }
}

impl<S> Layer<S> for SpanCaptureLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let metadata = attrs.metadata();

        // Only capture FSM spans
        if !metadata.name().starts_with("fsm.") && !metadata.name().starts_with("hitbox.") {
            return;
        }

        let mut visitor = FieldVisitor::new();
        attrs.record(&mut visitor);

        // Get parent ID - either explicit parent or current span
        let parent_id = attrs
            .parent()
            .cloned()
            .or_else(|| {
                if attrs.is_contextual() {
                    ctx.current_span().id().cloned()
                } else {
                    None
                }
            })
            .map(|id| id.into_u64());

        let span = CapturedSpan {
            id: id.into_u64(),
            parent_id,
            name: metadata.name().to_string(),
            target: metadata.target().to_string(),
            fields: visitor.fields,
        };

        self.spans.lock().unwrap().push(span);
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        // Update fields when span.record() is called
        if let Some(span_ref) = ctx.span(id) {
            let metadata = span_ref.metadata();

            if !metadata.name().starts_with("fsm.") && !metadata.name().starts_with("hitbox.") {
                return;
            }

            let mut visitor = FieldVisitor::new();
            values.record(&mut visitor);

            // Find the span by ID and update its fields
            let span_id = id.into_u64();
            let mut spans = self.spans.lock().unwrap();
            if let Some(captured) = spans.iter_mut().find(|s| s.id == span_id) {
                for (key, value) in visitor.fields {
                    // Update existing field or add new one
                    if let Some(existing) = captured.fields.iter_mut().find(|(k, _)| k == &key) {
                        existing.1 = value;
                    } else {
                        captured.fields.push((key, value));
                    }
                }
            }
        }
    }

    fn on_event(&self, _event: &Event<'_>, _ctx: Context<'_, S>) {
        // We don't capture events, only spans
    }
}

/// Collector for captured spans.
#[derive(Clone)]
pub struct SpanCollector {
    spans: Arc<Mutex<Vec<CapturedSpan>>>,
    dispatch: Dispatch,
}

/// Create a new span collector with its associated dispatch.
///
/// Returns a `SpanCollector` that can be cloned and used to inspect captured spans.
/// The collector contains a `Dispatch` that should be used with spawned tasks.
pub fn create_span_collector() -> SpanCollector {
    let spans = Arc::new(Mutex::new(Vec::new()));
    let layer = SpanCaptureLayer {
        spans: spans.clone(),
    };
    let subscriber = Registry::default().with(layer);
    let dispatch = Dispatch::new(subscriber);
    SpanCollector { spans, dispatch }
}

impl SpanCollector {
    /// Get the dispatch for use with spawned tasks.
    pub fn dispatch(&self) -> &Dispatch {
        &self.dispatch
    }

    /// Get all captured spans.
    pub fn spans(&self) -> Vec<CapturedSpan> {
        self.spans.lock().unwrap().clone()
    }

    /// Get captured span names in order.
    pub fn span_names(&self) -> Vec<String> {
        self.spans
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.name.clone())
            .collect()
    }

    /// Get FSM state names (strips "fsm." prefix).
    pub fn fsm_states(&self) -> Vec<String> {
        self.spans
            .lock()
            .unwrap()
            .iter()
            .filter_map(|s| s.name.strip_prefix("fsm.").map(String::from))
            .collect()
    }

    /// Get FSM states grouped by request.
    ///
    /// Each request creates a root "hitbox.cache" span. This method groups all FSM states
    /// by their root parent span, returning states for each request in order.
    pub fn fsm_states_per_request(&self) -> Vec<Vec<CapturedSpan>> {
        let spans = self.spans.lock().unwrap();

        // Find all root spans (hitbox.cache)
        let root_spans: Vec<_> = spans.iter().filter(|s| s.name == "hitbox.cache").collect();

        // For each root span, collect its descendant FSM states
        root_spans
            .iter()
            .map(|root| {
                let mut request_spans = Vec::new();
                Self::collect_descendants(&spans, root.id, &mut request_spans);
                request_spans
            })
            .collect()
    }

    /// Helper to collect all FSM descendant spans of a given parent.
    fn collect_descendants(
        all_spans: &[CapturedSpan],
        parent_id: u64,
        result: &mut Vec<CapturedSpan>,
    ) {
        for span in all_spans {
            if span.parent_id == Some(parent_id) {
                if span.name.starts_with("fsm.") {
                    result.push(span.clone());
                }
                // Recursively collect descendants
                Self::collect_descendants(all_spans, span.id, result);
            }
        }
    }

    /// Check if a span with the given name was captured.
    pub fn has_span(&self, name: &str) -> bool {
        self.spans.lock().unwrap().iter().any(|s| s.name == name)
    }

    /// Get a span by name (returns the last occurrence).
    pub fn get_span(&self, name: &str) -> Option<CapturedSpan> {
        self.spans
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|s| s.name == name)
            .cloned()
    }

    /// Get field value from a span.
    pub fn get_field(&self, span_name: &str, field_name: &str) -> Option<String> {
        self.get_span(span_name).and_then(|s| {
            s.fields
                .iter()
                .find(|(k, _)| k == field_name)
                .map(|(_, v)| v.clone())
        })
    }

    /// Clear all captured spans.
    pub fn clear(&self) {
        self.spans.lock().unwrap().clear();
    }

    /// Assert that spans were captured in the given order.
    /// Only checks that the given spans appear in order, allows other spans in between.
    pub fn assert_span_sequence(&self, expected: &[&str]) {
        let names = self.span_names();
        let mut expected_iter = expected.iter();
        let mut current_expected = expected_iter.next();

        for name in &names {
            if let Some(exp) = current_expected
                && name == *exp
            {
                current_expected = expected_iter.next();
            }
        }

        if current_expected.is_some() {
            panic!("Expected span sequence {:?} but got {:?}", expected, names);
        }
    }

    /// Assert that all given spans were captured (in any order).
    pub fn assert_has_spans(&self, expected: &[&str]) {
        for name in expected {
            if !self.has_span(name) {
                panic!(
                    "Expected span '{}' not found. Captured spans: {:?}",
                    name,
                    self.span_names()
                );
            }
        }
    }
}

/// Run a closure with span capturing enabled.
///
/// Returns the result of the closure and a collector with captured spans.
///
/// # Example
///
/// ```ignore
/// let (result, collector) = with_span_capture(|| {
///     // sync code
/// });
///
/// collector.assert_has_spans(&["fsm.PollCache", "fsm.Response"]);
/// ```
pub fn with_span_capture<F, R>(f: F) -> (R, SpanCollector)
where
    F: FnOnce() -> R,
{
    let collector = create_span_collector();
    let result = tracing::dispatcher::with_default(collector.dispatch(), f);
    (result, collector)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::{Level, span};

    #[test]
    fn test_span_capture() {
        let ((), collector) = with_span_capture(|| {
            let span = span!(Level::TRACE, "fsm.TestState", cache.key = "test_key");
            let _enter = span.enter();
        });

        assert!(collector.has_span("fsm.TestState"));
        assert_eq!(
            collector.get_field("fsm.TestState", "cache.key"),
            Some("test_key".to_string())
        );
    }

    #[test]
    fn test_span_sequence() {
        let ((), collector) = with_span_capture(|| {
            {
                let span = span!(Level::TRACE, "fsm.First");
                let _enter = span.enter();
            }
            {
                let span = span!(Level::TRACE, "fsm.Second");
                let _enter = span.enter();
            }
            {
                let span = span!(Level::TRACE, "fsm.Third");
                let _enter = span.enter();
            }
        });

        collector.assert_span_sequence(&["fsm.First", "fsm.Second", "fsm.Third"]);
    }

    #[test]
    fn test_field_recording() {
        let ((), collector) = with_span_capture(|| {
            let span = span!(
                Level::TRACE,
                "fsm.TestState",
                cache.key = "key",
                cache.status = tracing::field::Empty
            );
            let _enter = span.enter();
            span.record("cache.status", "hit");
        });

        assert_eq!(
            collector.get_field("fsm.TestState", "cache.status"),
            Some("hit".to_string())
        );
    }
}
