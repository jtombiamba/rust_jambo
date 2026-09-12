use lapin::types::{AMQPValue, FieldTable};
use opentelemetry::propagation::{Extractor, Injector};
use opentelemetry::Context;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Writes OpenTelemetry propagation fields (e.g. `traceparent`, `tracestate`)
/// into a lapin `FieldTable` used as RabbitMQ message headers.
pub struct AmqpInjector<'a>(pub &'a mut FieldTable);

impl Injector for AmqpInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0
            .insert(key.into(), AMQPValue::LongString(value.into()));
    }
}

/// Reads OpenTelemetry propagation fields from a lapin `FieldTable` of message headers.
#[allow(dead_code)]
pub struct AmqpExtractor<'a>(pub &'a FieldTable);

impl Extractor for AmqpExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.inner().get(key).and_then(|value| match value {
            AMQPValue::LongString(s) => std::str::from_utf8(s.as_bytes()).ok(),
            _ => None,
        })
    }

    fn keys(&self) -> Vec<&str> {
        self.0.inner().keys().map(|k| k.as_str()).collect()
    }
}

/// Inject the current tracing span's OpenTelemetry context into AMQP message headers.
///
/// Uses the *current tracing span's* context (via [`OpenTelemetrySpanExt::context`])
/// rather than the ambient [`Context::current`], because the tracing-opentelemetry layer
/// is not configured with context activation.
pub fn inject_headers() -> FieldTable {
    let parent_cx = tracing::Span::current().context();
    let mut headers = FieldTable::default();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&parent_cx, &mut AmqpInjector(&mut headers));
    });
    headers
}

/// Extract the OpenTelemetry parent context from AMQP message headers, if present.
#[allow(dead_code)]
pub fn extract_context(headers: &Option<FieldTable>) -> Context {
    match headers {
        Some(headers) => opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.extract(&AmqpExtractor(headers))
        }),
        None => Context::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::propagation::TextMapPropagator;
    use opentelemetry::trace::TraceContextExt;
    use opentelemetry_sdk::propagation::TraceContextPropagator;

    const TRACEPARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    #[test]
    fn extractor_ignores_non_string_values() {
        let mut headers = FieldTable::default();
        headers.insert("traceparent".into(), AMQPValue::Boolean(true));
        headers.insert("other".into(), AMQPValue::LongString("hello".into()));

        let extractor = AmqpExtractor(&headers);
        assert_eq!(extractor.get("traceparent"), None);
        assert_eq!(extractor.get("other"), Some("hello"));
        assert_eq!(extractor.keys().len(), 2);
    }

    #[test]
    fn extract_context_returns_default_when_no_headers() {
        let ctx = extract_context(&None);
        assert!(!ctx.has_active_span());
    }

    #[test]
    fn inject_then_extract_round_trips_trace_context() {
        let propagator = TraceContextPropagator::new();

        let mut source = FieldTable::default();
        source.insert(
            "traceparent".into(),
            AMQPValue::LongString(TRACEPARENT.into()),
        );
        let cx = propagator.extract(&AmqpExtractor(&source));
        assert!(cx.has_active_span());

        let mut headers = FieldTable::default();
        propagator.inject_context(&cx, &mut AmqpInjector(&mut headers));

        let traceparent = headers
            .inner()
            .get("traceparent")
            .and_then(|v| match v {
                AMQPValue::LongString(s) => std::str::from_utf8(s.as_bytes()).ok(),
                _ => None,
            })
            .expect("traceparent should be present");
        assert_eq!(traceparent, TRACEPARENT);
    }

    #[test]
    fn inject_headers_produces_empty_headers_without_active_tracing_span() {
        // With no active tracing span, inject_headers() degrades gracefully.
        let headers = inject_headers();
        assert!(headers.inner().is_empty());
    }
}
