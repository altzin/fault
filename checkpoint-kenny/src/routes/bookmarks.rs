use axum::{Json, extract::{State, Query}, http::StatusCode};
use opentelemetry::KeyValue;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{Instrument, info_span};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{error::AppError, state::AppState};

#[derive(Deserialize, ToSchema)]
pub struct CreateBookmark {
    pub url: String,
    pub title: Option<String>,
    pub raw_html: Option<String>, // The frontend sends the heavy HTML payload here
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct Bookmark {
    pub id: Uuid,
    pub url: String,
    pub title: Option<String>,
    pub status: String, // e.g., "pending", "processed", "failed"
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct Pagination {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/bookmarks",
    request_body = CreateBookmark,
    responses(
        (status = 201, description = "Bookmark created successfully", body = Bookmark)
    ),
    tag = "Bookmarks"
)]
#[tracing::instrument(
    name = "http.post.create_bookmark",
    skip(state, payload), 
    fields(bookmark.url = %payload.url)
)]
pub async fn create_bookmark(
    State(state): State<AppState>,
    Json(payload): Json<CreateBookmark>,
) -> Result<(StatusCode, Json<Bookmark>), AppError> {

    // Default to 'pending' so Woodhouse knows it needs processing
    let bookmark = sqlx::query_as!(
        Bookmark,
        r#"
        INSERT INTO bookmarks (id, url, title, raw_html, status)
        VALUES ($1, $2, $3, $4, 'pending')
        RETURNING id, url, title, status
        "#,
        Uuid::new_v4(),
        payload.url,
        payload.title,
        payload.raw_html
    )
    .fetch_one(&state.db)
    .instrument(info_span!("db.query.insert_bookmark"))
    .await?;

    state.metrics.bookmarks_created.add(1, &[KeyValue::new("status", "success")]);

    // TODO: Publish bookmark.id to NATS here

    Ok((StatusCode::CREATED, Json(bookmark)))
}

#[utoipa::path(
    get,
    path = "/bookmarks",
    params(Pagination),
    responses(
        (status = 200, description = "List of bookmarks retrieved", body = [Bookmark])
    ),
    tag = "Bookmarks"
)]
#[tracing::instrument(
    name = "http.get.list_bookmarks",
    skip(pool, pagination),
    fields(
        db.limit = pagination.limit.unwrap_or(10),
        db.offset = pagination.offset.unwrap_or(0)
    )
)]
pub async fn list_bookmarks(
    State(pool): State<PgPool>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<Vec<Bookmark>>, AppError> {
    
    let limit = pagination.limit.unwrap_or(10).clamp(1, 100);
    let offset = pagination.offset.unwrap_or(0).max(0);

    let bookmarks = sqlx::query_as!(
        Bookmark,
        r#"
        SELECT id, url, title, status
        FROM bookmarks
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#,
        limit,
        offset
    )
    .fetch_all(&pool)
    .instrument(info_span!("db.query.select_bookmarks"))
    .await?;

    Ok(Json(bookmarks))
}
