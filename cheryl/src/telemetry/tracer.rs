use opentelemetry::{KeyValue, global};
use opentelemetry_otlp::{WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::{
    Resource,
    trace::{Sampler, SdkTracerProvider},
};
use std::collections::HashMap;

use crate::config::OtlpConfig;

pub fn init_tracer(
    config: Option<&OtlpConfig>,
    service_name: &str,
    is_prod: bool,
) -> Result<SdkTracerProvider, Box<dyn std::error::Error>> {
    let resource = Resource::builder()
        .with_attribute(KeyValue::new("service.name", service_name.to_string()))
        .with_attribute(KeyValue::new(
            "deployment.environment",
            if is_prod { "production" } else { "development" },
        ))
        .build();

    let mut builder = SdkTracerProvider::builder().with_resource(resource);

    // In prod, sample a percentage (e.g. parent-based 10%) or 100% in dev
    let sampler = if is_prod {
        Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(0.1)))
    } else {
        Sampler::AlwaysOn
    };
    builder = builder.with_sampler(sampler);

    // Attach remote OTLP exporter only if credentials are present
    if let Some(cfg) = config {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), cfg.auth_header.to_string());

        let ep = cfg.base_url();

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(format!("{ep}/v1/traces"))
            .with_headers(headers)
            .build()?;

        builder = builder.with_batch_exporter(exporter);
        println!("Tracing: remote OpenObserve exporter active");
    } else {
        println!("Tracing: running in local mode (no remote exporter)");
    }

    let provider = builder.build();
    global::set_tracer_provider(provider.clone());

    Ok(provider)
}
