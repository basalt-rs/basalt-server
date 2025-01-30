use std::sync::Arc;

use axum::Router;
use tokio::sync::RwLock;
use utoipa_axum::router::OpenApiRouter;

use crate::{services, storage::SqliteLayer};

pub struct AppState {
    pub db: RwLock<SqliteLayer>,
}

pub fn router(initial_state: Arc<AppState>) -> axum::routing::IntoMakeService<axum::Router> {
    Router::new()
        .nest("/auth", services::auth::auth_service())
        .with_state(initial_state)
        .into_make_service()
}

pub fn doc_router(initial_state: Arc<AppState>) -> OpenApiRouter {
    OpenApiRouter::new()
        .nest("/auth", services::auth::auth_router())
        .with_state(initial_state)
}
