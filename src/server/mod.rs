use std::sync::Arc;

use axum::Router;
use bedrock::Config;
use dashmap::DashMap;
use tokio::sync::RwLock;

use crate::{
    services::{self, ws},
    storage::SqliteLayer,
};

pub struct AppState {
    pub db: RwLock<SqliteLayer>,
    pub active_connections: DashMap<ws::ConnectionKind, ws::ConnectedClient>,
    pub config: Config,
}

impl AppState {
    pub fn new(db: SqliteLayer, config: Config) -> Self {
        Self {
            db: RwLock::new(db),
            active_connections: Default::default(),
            config,
        }
    }
}

pub fn router(initial_state: Arc<AppState>) -> axum::Router {
    Router::new()
        .nest("/auth", services::auth::auth_service())
        .nest("/ws", services::ws::ws_service())
        .with_state(initial_state)
}

#[cfg(debug_assertions)]
pub fn doc_router(initial_state: Arc<AppState>) -> utoipa_axum::router::OpenApiRouter {
    utoipa_axum::router::OpenApiRouter::new()
        .nest("/auth", services::auth::auth_router())
        .nest("/ws", services::ws::ws_router())
        .with_state(initial_state)
}
