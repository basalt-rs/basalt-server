use crate::server::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::OnceCell;
use tower_http::services::ServeFile;
use tracing::error;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

static INFO: OnceCell<StaticCompetitionInfo> = OnceCell::const_new();
static RAW_INFO: OnceCell<StaticCompetitionInfo> = OnceCell::const_new();

#[derive(Serialize, ToSchema)]
pub struct StaticCompetitionInfo {
    /// The title of this competition
    title: String,
    /// The preamble of this competition, if specified in the configuration
    ///
    /// This may be either markdown or HTML
    preamble: Option<String>,
    /// The names of the problems in this competition
    problems: Vec<String>,
    /// The version of `basalt-server` that the server is running
    #[schema(value_type = String)]
    version: semver::Version,
    /// Total time limit for this competition
    time_limit_secs: u64,
    /// All languages supported by this competition
    languages: Vec<String>,
    /// Whether the server hosts the competition packet PDF
    packet: bool,
}

impl StaticCompetitionInfo {
    pub fn new_with_preamble(state: &AppState, preamble: Option<String>) -> Self {
        Self {
            title: state.config.packet.title.clone(),
            preamble,
            problems: state
                .config
                .packet
                .problems
                .iter()
                .map(|p| p.title.clone())
                .collect(),
            version: semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap(),
            time_limit_secs: match &state.config.game {
                bedrock::Game::Points(points_settings) => points_settings.time_limit.as_secs(),
                bedrock::Game::Race(race_settings) => {
                    race_settings.time_limit.map(|x| x.as_secs()).unwrap_or(0)
                }
            },
            languages: state
                .config
                .languages
                .iter()
                .map(|l| l.name().to_string())
                .collect(),
            packet: state.packet.is_some(),
        }
    }

    pub fn new_raw(state: &AppState) -> Self {
        Self::new_with_preamble(
            state,
            state
                .config
                .packet
                .preamble
                .as_ref()
                .map(|x| x.raw().to_string()),
        )
    }

    pub fn new(state: &AppState) -> Option<Self> {
        Some(Self::new_with_preamble(
            state,
            state
                .config
                .packet
                .preamble
                .as_ref()
                .map(|x| x.html())
                .transpose()
                .map_err(|err| {
                    error!("Error compiling preamble: {:?}", err);
                })
                .ok()?,
        ))
    }
}

#[derive(Serialize, ToSchema)]
pub struct CompetitionInfo {
    statik: &'static StaticCompetitionInfo,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = Date)]
    server_time: OffsetDateTime,
}

impl CompetitionInfo {
    async fn new(state: &AppState, raw_markdown: bool) -> Option<Self> {
        let statik = if raw_markdown {
            RAW_INFO
                .get_or_init(|| async { StaticCompetitionInfo::new_raw(state) })
                .await
        } else {
            INFO.get_or_try_init(|| async { StaticCompetitionInfo::new(state).ok_or(()) })
                .await
                .ok()?
        };

        Some(CompetitionInfo {
            statik,
            server_time: OffsetDateTime::now_utc(),
        })
    }
}

#[derive(Deserialize, IntoParams)]
pub struct InfoQuery {
    #[serde(default)]
    #[param(default = false)]
    raw_markdown: bool,
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    tag = "competition", path = "/",
    params(InfoQuery),
    responses((status = OK, body = CompetitionInfo, content_type = "application/json"))
)]
pub async fn get_info(
    State(state): State<Arc<AppState>>,
    Query(query): Query<InfoQuery>,
) -> Result<Json<CompetitionInfo>, StatusCode> {
    if let Some(info) = CompetitionInfo::new(&state, query.raw_markdown).await {
        Ok(Json(info))
    } else {
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

pub fn router(state: Arc<AppState>) -> OpenApiRouter<Arc<AppState>> {
    let mut router = OpenApiRouter::new().routes(routes!(get_info));

    if let Some(packet) = &state.packet {
        router = router.nest_service("/packet", ServeFile::new(packet))
    }

    router
}

pub fn service(state: Arc<AppState>) -> axum::Router<Arc<AppState>> {
    router(state).split_for_parts().0
}
