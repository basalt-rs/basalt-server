use std::{path::PathBuf, sync::Arc};

use axum::Router;
use bedrock::Config;
use clock::ClockInfo;
use dashmap::{DashMap, DashSet};
use rand::{distributions::Alphanumeric, Rng};
use teams::TeamManagement;
use tokio::sync::RwLock;

pub mod clock;
pub mod teams;

use crate::{
    services::{
        self,
        ws::{self, WebSocketSend},
    },
    storage::SqliteLayer,
};

pub struct AppState {
    pub db: RwLock<SqliteLayer>,
    pub web_dir: Option<PathBuf>,
    pub active_connections: DashMap<ws::ConnectionKind, ws::ConnectedClient>,
    pub team_manager: TeamManagement,
    pub active_tests: DashSet<(ws::ConnectionKind, usize)>,
    pub active_submissions: DashSet<(ws::ConnectionKind, usize)>,
    pub config: Config,
    pub clock: RwLock<ClockInfo>,
}

impl AppState {
    pub fn new(db: SqliteLayer, config: Config, web_dir: Option<PathBuf>) -> Self {
        Self {
            db: RwLock::new(db),
            web_dir,
            active_connections: Default::default(),
            team_manager: TeamManagement::from_config(&config),
            active_tests: Default::default(),
            active_submissions: Default::default(),
            config,
            clock: Default::default(),
        }
    }

    pub fn broadcast(self: Arc<Self>, broadcast: WebSocketSend) -> anyhow::Result<()> {
        let mut to_remove = Vec::new();
        for conn in &self.active_connections {
            if conn.send.send(broadcast.clone()).is_err() {
                // This _shouldn't_ happen, but it _could_
                tracing::warn!(key = ?conn.key(), "Socket discovered to be closed when sending broadcast. Removing from active connections...");
                to_remove.push(conn.key().clone());
            }
        }
        to_remove.iter().for_each(|x| {
            self.active_connections.remove(x);
        });

        Ok(())
    }
}

macro_rules! define_router {
    ($($route: ident),+$(,)?) => {
        pub fn router(initial_state: Arc<AppState>) -> axum::Router {
            let router = Router::new()
                $(.nest(concat!("/", stringify!($route)), services::$route::service()))+;

                let router = if let Some(path) = &initial_state.web_dir {
                    router.fallback_service(tower_http::services::ServeDir::new(path))
                } else {
                    router
                };

            router.with_state(initial_state)
                .layer(tower_http::cors::CorsLayer::permissive())
                .layer(
                    tower_http::trace::TraceLayer::new_for_http().make_span_with(
                        |request: &axum::http::Request<axum::body::Body>| {
                            tracing::trace_span!(
                                "request",
                                method = %request.method(),
                                uri = %request.uri(),
                                version = ?request.version(),
                                id = %rand::thread_rng()
                                    .sample_iter(Alphanumeric)
                                    .take(10)
                                    .map(char::from)
                                    .collect::<String>()
                            )
                        },
                    ),
                )
        }

        #[cfg(feature = "doc-gen")]
        pub fn doc_router(initial_state: Arc<AppState>) -> utoipa_axum::router::OpenApiRouter {
            utoipa_axum::router::OpenApiRouter::new()
                $(.nest(concat!("/", stringify!($route)), services::$route::router()))+
                .with_state(initial_state)
                .layer(tower_http::cors::CorsLayer::permissive())
                .layer(tower_http::trace::TraceLayer::new_for_http())
        }
    };
}


define_router! {
    auth,
    questions,
    competition,
    teams,
    ws,
    clock,
}
