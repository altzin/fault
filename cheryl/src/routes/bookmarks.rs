use axum::{Json, extract::{State, Query}, http::StatusCode};
use opentelemetry::KeyValue;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{Instrument, info_span};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use chrono::{DateTime, Utc};

use serde_json::json;


use crate::{error::AppError, state::AppState};

#[derive(Deserialize, ToSchema)]
pub struct CreateBookmark {
    pub url: String,
    pub title: Option<String>,
    // We keep this in the struct so the API can receive it, 
    // but we won't insert it into the database.
    pub raw_html: Option<String>, 
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct Bookmark {
    pub id: Uuid,
    pub url: String,
    pub title: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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

    // Note: raw_html is removed from the query, and timestamps are returned
    let bookmark = sqlx::query_as!(
        Bookmark,
        r#"
        INSERT INTO bookmarks (id, url, title, status)
        VALUES ($1, $2, $3, 'pending')
        RETURNING id, url, title, status, created_at, updated_at
        "#,
        Uuid::new_v4(),
        payload.url,
        payload.title
    )
    .fetch_one(&state.db)
    .instrument(info_span!("db.query.insert_bookmark"))
    .await?;

    state.metrics.bookmarks_created.add(1, &[KeyValue::new("status", "success")]);


    if let Some(html) = payload.raw_html {
        if !html.is_empty() {
            // Get the bucket (assume it was created by Terraform)
            let store = state.js.get_object_store("raw_html").await
                .map_err(|e| {
                    tracing::error!(error = %e, "Failed to get Object Store");
                    AppError::Database(sqlx::Error::Io(std::io::ErrorKind::ConnectionRefused.into())) // Or map to a new AppError variant
                })?;

            // Convert HTML string into an async reader
            let mut reader = html.as_bytes();
            //TODO: Fix.
            let id_str = bookmark.id.to_string()+".html"; //seems like a small edit. is important
                store.put(id_str.as_str(), &mut reader).await
                    .map_err(|e| {
                        tracing::error!(error = %e, "Failed to upload to Object Store");
                        AppError::Database(sqlx::Error::Io(std::io::ErrorKind::Other.into())) 
                    })?;
        }
    }

    // 3. Publish to JetStream Queue
    let event = json!({
        "id": bookmark.id,
        "url": bookmark.url
    });

    state.js.publish("woodhouse.bookmark.pending", event.to_string().into()).await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to publish NATS event");
            AppError::Database(sqlx::Error::Io(std::io::ErrorKind::Other.into()))
        })?;

    // 4. Return success to user
    state.metrics.bookmarks_created.add(1, &[KeyValue::new("status", "success")]);

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
        SELECT id, url, title, status, created_at, updated_at
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
