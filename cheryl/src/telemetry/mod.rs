use std::collections::HashMap;
use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry_otlp::{
    LogExporter, MetricExporter, SpanExporter, WithExportConfig, WithHttpConfig,
};
use opentelemetry_sdk::{
    Resource, logs::SdkLoggerProvider, metrics::SdkMeterProvider, trace::SdkTracerProvider,
};
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::OtlpConfig;

pub struct TelemetryGuards {
    pub tracer_provider: Option<SdkTracerProvider>,
    pub logger_provider: Option<SdkLoggerProvider>,
    pub meter_provider: Option<SdkMeterProvider>,
}

impl TelemetryGuards {
    pub fn shutdown(self) {
        if let Some(tracer) = self.tracer_provider {
            let _ = tracer.shutdown();
        }
        if let Some(meter) = self.meter_provider {
            let _ = meter.shutdown();
        }
        if let Some(logger) = self.logger_provider {
            let _ = logger.shutdown();
        }
    }
}

// --- Component 1: Tracing Provider ---
fn init_tracer_provider(
    resource: &Resource,
    otlp_url: &str,
    headers: HashMap<String, String>,
) -> SdkTracerProvider {
    let span_exporter = SpanExporter::builder()
        .with_http()
        .with_endpoint(format!("{otlp_url}/v1/traces"))
        .with_headers(headers)
        .with_timeout(Duration::from_secs(3))
        .build()
        .expect("failed to create span exporter");

    SdkTracerProvider::builder()
        .with_resource(resource.clone())
        .with_batch_exporter(span_exporter)
        .build()
}

// --- Component 2: Correlated Logs Provider ---
fn init_logger_provider(
    resource: &Resource,
    otlp_url: &str,
    headers: HashMap<String, String>,
) -> SdkLoggerProvider {
    let log_exporter = LogExporter::builder()
        .with_http()
        .with_endpoint(format!("{otlp_url}/v1/logs"))
        .with_headers(headers)
        .with_timeout(Duration::from_secs(3))
        .build()
        .expect("failed to create log exporter");

    SdkLoggerProvider::builder()
        .with_resource(resource.clone())
        .with_batch_exporter(log_exporter)
        .build()
}

// --- Component 3: Metrics Provider ---
fn init_metrics_provider(
    resource: &Resource,
    otlp_url: &str,
    headers: HashMap<String, String>,
) -> SdkMeterProvider {
    let metric_exporter = MetricExporter::builder()
        .with_http()
        .with_endpoint(format!("{otlp_url}/v1/metrics"))
        .with_headers(headers)
        .with_timeout(Duration::from_secs(3))
        .build()
        .expect("failed to create metric exporter");

    let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(metric_exporter)
        .with_interval(Duration::from_secs(60)) //increase events sent to openobserve
        .build();

    SdkMeterProvider::builder()
        .with_resource(resource.clone())
        .with_reader(reader)
        .build()
}
pub fn init_telemetry(
    service_name: &str,
    is_prod: bool,
    otlp_config: Option<&OtlpConfig>,
) -> TelemetryGuards {
    // 1. Stdout Terminal Logging
    let env_filter_str = if is_prod {
        "axum_test=info,tower_http=info,warn"
    } else {
        "axum_test=debug,tower_http=debug,info"
    };

    let stdout_layer = tracing_subscriber::fmt::layer().with_filter(EnvFilter::new(env_filter_str));

    // Fast-path: local development without OTLP endpoint
    if !is_prod || otlp_config.is_none() {
        tracing_subscriber::registry().with(stdout_layer).init();

        return TelemetryGuards {
            tracer_provider: None,
            logger_provider: None,
            meter_provider: None,
        };
    }

    let otlp = otlp_config.unwrap();
    let otlp_base_url = otlp.base_url();
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), otlp.auth_header.to_string());

    // Shared identity resource across all 3 pillars
    let resource = Resource::builder()
        .with_attributes(vec![
            KeyValue::new("service.name", service_name.to_string()),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new(
                "deployment.environment",
                if is_prod { "production" } else { "development" },
            ),
        ])
        .build();

    // 2. Initialize all three providers
    let tracer_provider = init_tracer_provider(&resource, &otlp_base_url, headers.clone());
    let logger_provider = init_logger_provider(&resource, &otlp_base_url, headers.clone());
    let meter_provider = init_metrics_provider(&resource, &otlp_base_url, headers);

    // Register meter globally so application code can call opentelemetry::global::meter(...)
    opentelemetry::global::set_meter_provider(meter_provider.clone());

    // 3. Construct subscriber layers
    use opentelemetry::trace::TracerProvider as _;
    let tracer = tracer_provider.tracer("axum-test");
    let otel_trace_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let otel_log_layer =
        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider)
            // Add tower_http=info to allow middleware logs through to OpenObserve
            .with_filter(EnvFilter::new(
                "axum_test=info,tower_http=info,sqlx=warn,warn",
            ));

    // 4. Initialize global tracing registry
    tracing_subscriber::registry()
        .with(stdout_layer)
        .with(otel_trace_layer)
        .with(otel_log_layer)
        .init();

    TelemetryGuards {
        tracer_provider: Some(tracer_provider),
        logger_provider: Some(logger_provider),
        meter_provider: Some(meter_provider),
    }
}
