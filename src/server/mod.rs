use std::sync::Arc;

use tokio::sync::RwLock;

use crate::{services, storage::SqliteLayer};

pub struct AppState {
    pub db: RwLock<SqliteLayer>,
}

pub fn router(initial_state: Arc<AppState>) -> axum::routing::IntoMakeService<axum::Router> {
    axum::Router::new()
        .nest("/auth", services::auth::auth_service(initial_state.clone()))
        .with_state(initial_state)
        .into_make_service()
}
