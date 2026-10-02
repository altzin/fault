use axum::extract::FromRef;
use opentelemetry::metrics::{Counter, Histogram, Meter};
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppMetrics {
    pub bookmarks_created: Counter<u64>,
    pub http_requests_total: Counter<u64>,
    pub http_request_duration_seconds: Histogram<f64>,
}

impl AppMetrics {
    pub fn new(meter: &Meter) -> Self {
        Self {
            bookmarks_created: meter
                .u64_counter("bookmarks.created")
                .with_description("Total number of bookmarks successfully created")
                .build(),
            http_requests_total: meter
                .u64_counter("http.requests.total")
                .with_description("Total number of HTTP requests")
                .build(),
            http_request_duration_seconds: meter
                .f64_histogram("http.request.duration.seconds")
                .with_description("HTTP request duration in seconds")
                .with_boundaries(vec![0.05, 0.1, 0.5, 1.0, 5.0])
                .build(), //make bucket smaller to save events
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub metrics: AppMetrics,
}

// Allows `State<PgPool>` in handlers
impl FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> Self {
        state.db.clone()
    }
}
