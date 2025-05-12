use axum::Router;
use bedrock::Config;
use clock::ClockInfo;
use dashmap::DashSet;
use rand::{distributions::Alphanumeric, Rng};
use std::{path::PathBuf, sync::Arc};
use teams::TeamManagement;
use tokio::sync::RwLock;
use websocket::WebSocketManager;

pub mod clock;
pub mod teams;
pub mod websocket;

use crate::{services, storage::SqliteLayer};

pub struct AppState {
    pub db: RwLock<SqliteLayer>,
    pub web_dir: Option<PathBuf>,
    pub websocket: WebSocketManager,
    pub team_manager: TeamManagement,
    pub active_tests: DashSet<(websocket::ConnectionKind, usize)>,
    pub active_submissions: DashSet<(websocket::ConnectionKind, usize)>,
    pub config: Config,
    pub clock: RwLock<ClockInfo>,
}

impl AppState {
    pub fn new(db: SqliteLayer, config: Config, web_dir: Option<PathBuf>) -> Self {
        Self {
            db: RwLock::new(db),
            web_dir,
            websocket: Default::default(),
            team_manager: TeamManagement::from_config(&config),
            active_tests: Default::default(),
            active_submissions: Default::default(),
            config,
            clock: Default::default(),
        }
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
    announcements,
    auth,
    clock,
    competition,
    questions,
    teams,
    testing,
    leaderboard,
    ws,
}
