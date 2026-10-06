use axum::http::StatusCode;

#[utoipa::path(
    get,
    path = "/healthz",
    responses(
        (status = 200, description = "Service is healthy", body = String)
    ),
    tag = "Health"
)]
pub async fn health_handler() -> StatusCode {
    StatusCode::OK
}
