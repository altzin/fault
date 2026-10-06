use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

pub enum AppError {
    Database(sqlx::Error),
    // You can add more variants later: NotFound, Unauthorized, etc.
}

// 1. Tell Axum how to convert this error into an HTTP response
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Database(err) => {
                // Log the exact internal error for OpenObserve
                tracing::error!(error = %err, "Database error");

                // Never leak raw SQL errors to the client; return a generic 500
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
        };

        (status, error_message).into_response()
    }
}

// 2. Tell Rust how to automatically convert sqlx::Error into AppError
impl From<sqlx::Error> for AppError {
    fn from(inner: sqlx::Error) -> Self {
        AppError::Database(inner)
    }
}
