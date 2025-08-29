use std::{path::PathBuf, sync::Arc};

use axum::Router;
use bedrock::Config;
use clock::ClockInfo;
use dashmap::DashSet;
use rand::{distributions::Alphanumeric, Rng};
use teams::TeamManagement;
use tokio::{
    sync::{mpsc::UnboundedSender, RwLock},
    task::JoinSet,
};
use websocket::WebSocketManager;

pub mod clock;
pub mod hooks;
pub mod teams;
pub mod tester;
pub mod websocket;

use crate::{
    repositories::{self, users::Role},
    server::{hooks::events::ServerEvent, tester::Tester},
    services,
    storage::SqliteLayer,
};

type Dispatchers = Vec<UnboundedSender<(ServerEvent, Arc<AppState>)>>;

pub struct AppState {
    pub db: SqliteLayer,
    pub web_dir: Option<PathBuf>,
    pub websocket: WebSocketManager,
    pub team_manager: TeamManagement,
    pub active_tests: DashSet<(websocket::ConnectionKind, usize)>,
    pub active_submissions: DashSet<(websocket::ConnectionKind, usize)>,
    pub tester: Tester,
    pub config: Config,
    pub clock: RwLock<ClockInfo>,
    pub dispatchers: Dispatchers,
}

impl AppState {
    pub fn new(db: SqliteLayer, config: Config, web_dir: Option<PathBuf>) -> Self {
        Self {
            db,
            web_dir,
            websocket: Default::default(),
            team_manager: Default::default(),
            active_tests: Default::default(),
            active_submissions: Default::default(),
            dispatchers: Default::default(),
            tester: Tester::new(&config),
            config,
            clock: Default::default(),
        }
    }

    pub fn init_hooks(&mut self) -> JoinSet<()> {
        let mut jset = JoinSet::<()>::new();

        #[cfg(feature = "scripting")]
        {
            let (mut hook_handler, hooks_tx) =
                crate::server::hooks::handlers::EventHookHandler::create();
            self.dispatchers.push(hooks_tx);
            jset.spawn(async move { hook_handler.start().await });
        }

        #[cfg(feature = "webhooks")]
        {
            let (mut webhook_handler, webhooks_tx) =
                crate::server::hooks::webhooks::EventWebhookHandler::create();
            self.dispatchers.push(webhooks_tx);
            jset.spawn(async move { webhook_handler.start().await });
        }

        jset
    }

    pub async fn init(&mut self) -> anyhow::Result<()> {
        // init teams
        let users = repositories::users::get_users_with_role(&*self.db, Role::Competitor).await?;
        self.team_manager
            .insert_many(users.into_iter().map(|u| u.id));

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
