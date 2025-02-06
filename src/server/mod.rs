use std::sync::Arc;

use axum::Router;
use bedrock::Config;
use dashmap::DashMap;
use tokio::sync::RwLock;

use crate::{
    services::{
        self,
        ws::{self, Broadcast, WebSocketSend},
    },
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

    pub fn broadcast(self: Arc<Self>, broadcast: Broadcast) -> anyhow::Result<()> {
        let mut to_remove = Vec::new();
        let broadcast = WebSocketSend::Broadcast { broadcast };
        for conn in &self.active_connections {
            if conn.send.send(broadcast.clone()).is_err() {
                // This _shouldn't_ happen, but it _could_
                tracing::warn!(key = ?conn.key(), "Socket discovered to be closed when sending broadcast.  Removing from active connections...");
                to_remove.push(conn.key().clone());
            }
        }
        to_remove.iter().for_each(|x| {
            self.active_connections.remove(x);
        });

        Ok(())
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
