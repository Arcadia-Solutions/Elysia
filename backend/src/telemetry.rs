use std::sync::{LazyLock, OnceLock};

use opentelemetry::KeyValue;
use opentelemetry::metrics::Counter;
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    Resource, logs::SdkLoggerProvider, metrics::SdkMeterProvider, trace::SdkTracerProvider,
};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::TelemetryConfig;

/// Name reported to the telemetry collector.
const SERVICE_NAME: &str = "elysia";

static PROVIDERS: OnceLock<(SdkTracerProvider, SdkMeterProvider, SdkLoggerProvider)> =
    OnceLock::new();

/// Stdout logging, plus OTLP export of traces, metrics and logs when
/// `telemetry.otlp_endpoint` is set. `RUST_LOG` overrides the default `info` level.
pub fn init(config: &TelemetryConfig) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer());

    let Some(endpoint) = &config.otlp_endpoint else {
        registry.init();
        return;
    };
    let resource = Resource::builder().with_service_name(SERVICE_NAME).build();

    let span_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .expect("failed to create OTLP span exporter");
    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(resource.clone())
        .with_batch_exporter(span_exporter)
        .build();

    let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .expect("failed to create OTLP metric exporter");
    let meter_provider = SdkMeterProvider::builder()
        .with_resource(resource.clone())
        .with_periodic_exporter(metric_exporter)
        .build();
    opentelemetry::global::set_meter_provider(meter_provider.clone());

    let log_exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .expect("failed to create OTLP log exporter");
    let logger_provider = SdkLoggerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(log_exporter)
        .build();

    registry
        .with(tracing_opentelemetry::layer().with_tracer(tracer_provider.tracer(SERVICE_NAME)))
        .with(
            opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(
                &logger_provider,
            ),
        )
        .init();

    let _ = PROVIDERS.set((tracer_provider, meter_provider, logger_provider));
}

/// Flush pending telemetry before exit.
pub fn shutdown() {
    if let Some((tracer_provider, meter_provider, logger_provider)) = PROVIDERS.get() {
        let _ = tracer_provider.shutdown();
        let _ = meter_provider.shutdown();
        let _ = logger_provider.shutdown();
    }
}

/// Application metrics. The global meter is a no-op until [`init`] installs
/// the OTLP provider, so recording is free when telemetry is disabled.
struct Instruments {
    http_errors: Counter<u64>,
    uploads: Counter<u64>,
    upload_source_bytes: Counter<u64>,
    upload_stored_bytes: Counter<u64>,
}

static INSTRUMENTS: LazyLock<Instruments> = LazyLock::new(|| {
    let meter = opentelemetry::global::meter(SERVICE_NAME);
    Instruments {
        http_errors: meter
            .u64_counter("http.errors")
            .with_description("HTTP error responses")
            .build(),
        uploads: meter
            .u64_counter("elysia.uploads")
            .with_description("Newly stored images")
            .build(),
        upload_source_bytes: meter
            .u64_counter("elysia.upload.source_bytes")
            .with_description("Bytes received for newly stored images")
            .with_unit("By")
            .build(),
        upload_stored_bytes: meter
            .u64_counter("elysia.upload.stored_bytes")
            .with_description("Bytes written to disk for newly stored images, after processing")
            .with_unit("By")
            .build(),
    }
});

/// Count an HTTP error response.
pub fn record_http_error(error: &str, status_code: u16) {
    INSTRUMENTS.http_errors.add(
        1,
        &[
            KeyValue::new("error", error.to_string()),
            KeyValue::new("status_code", i64::from(status_code)),
        ],
    );
}

/// Count a newly stored image, its received size and its stored size.
pub fn record_upload(format: &'static str, source_bytes: u64, stored_bytes: u64) {
    let attributes = [KeyValue::new("format", format)];
    INSTRUMENTS.uploads.add(1, &attributes);
    INSTRUMENTS
        .upload_source_bytes
        .add(source_bytes, &attributes);
    INSTRUMENTS
        .upload_stored_bytes
        .add(stored_bytes, &attributes);
}
