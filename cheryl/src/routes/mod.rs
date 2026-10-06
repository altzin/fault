use crate::{routes, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub mod bookmarks;
mod health; // Changed from items

#[derive(OpenApi)]
#[openapi(
    paths(
        health::health_handler,
        bookmarks::create_bookmark,
        bookmarks::list_bookmarks
    ),
    components(
        schemas(bookmarks::Bookmark, bookmarks::CreateBookmark)
    ),
    tags(
        (name = "Bookmarks", description = "Bookmark management APIs")
    )
)]
pub struct ApiDoc;

pub fn create_router(is_prod: bool) -> Router<AppState> {
    let mut router = Router::new()
        .route("/healthz", get(health::health_handler))
        .route("/bookmarks", post(bookmarks::create_bookmark))
        .route("/bookmarks", get(bookmarks::list_bookmarks));

    if !is_prod {
        let swagger =
            SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", routes::ApiDoc::openapi());
        router = router.merge(swagger);
    }
    router
}
