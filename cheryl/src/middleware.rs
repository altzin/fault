use axum::Router;
use axum::http::StatusCode;
use axum::{
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::Response,
};
use opentelemetry::KeyValue;
use std::time::Duration;
use std::time::Instant;
use tower_http::trace::TraceLayer;
use tower_http::{
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
    timeout::TimeoutLayer, // <-- Use the standard TraceLayer
};

// Assuming you have an AppState struct that holds your metrics
use crate::AppState;

pub trait RouterExt {
    fn with_base_middleware(self) -> Self;
}

impl RouterExt for Router<AppState> {
    // <-- Add State generic if chaining stateful middleware
    fn with_base_middleware(self) -> Self {
        self.layer(TraceLayer::new_for_http()) // <-- Replaces OtelAxumLayer
            .layer(CorsLayer::permissive())
            .layer(RequestBodyLimitLayer::new(1024 * 1024))
            .layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                Duration::from_secs(10),
            ))
    }
}

// Fix the State extractor by giving it your AppState type
pub async fn track_metrics(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().to_string();

    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|mp| mp.as_str().to_owned())
        .unwrap_or_else(|| req.uri().path().to_owned());

    let response = next.run(req).await;

    let latency = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    let labels = [
        KeyValue::new("http.method", method),
        KeyValue::new("http.route", path),
        KeyValue::new("http.status_code", status),
    ];

    // Access metrics through your state
    state.metrics.http_requests_total.add(1, &labels);
    state
        .metrics
        .http_request_duration_seconds
        .record(latency, &labels);

    response
}
