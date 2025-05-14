use std::sync::OnceLock;

use opentelemetry::global::{self, BoxedTracer};
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry::trace::{TraceContextExt as _, TracerProvider as _};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry};

pub fn get_tracer() -> &'static BoxedTracer {
    static TRACER: OnceLock<BoxedTracer> = OnceLock::new();
    TRACER.get_or_init(|| global::tracer("resource-server"))
}

pub fn get_current_activity_id() -> Option<String> {
    // Get the current `tracing` span
    let span = Span::current();

    // Get OpenTelemetry context from the tracing span
    let otel_ctx = span.context();
    let otel_span = otel_ctx.span();

    // Extract SpanContext which holds the trace_id
    let span_ctx = otel_span.span_context();

    if span_ctx.is_valid() {
        Some(span_ctx.trace_id().to_string())
    } else {
        None
    }
}

fn init_tracer(level: &str) {
    // Create a new OpenTelemetry trace pipeline that prints to stdout
    let sdk_provider = SdkTracerProvider::builder()
        .with_simple_exporter(opentelemetry_stdout::SpanExporter::default())
        .build();

    let tracer = sdk_provider.tracer("resource-server");
    global::set_tracer_provider(sdk_provider);

    // Create a tracing layer with the configured tracer
    let telemetry = tracing_opentelemetry::layer()
        .with_tracer(tracer);

    // Use the tracing subscriber `Registry`, or any other subscriber
    // that impls `LookupSpan`
    Registry::default()
        .with(EnvFilter::new(level))
        .with(tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            // .with_thread_names(true)
        )
        .with(telemetry)
        .init();
}

pub fn init(level: &str) {
    static INITIALIZED: OnceLock<bool> = OnceLock::new();

    if *INITIALIZED.get_or_init(|| {
        init_tracer(level);
        true
    }) {
        return;
    }
}