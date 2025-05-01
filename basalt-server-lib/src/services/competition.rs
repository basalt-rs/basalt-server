use crate::server::AppState;
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{AppendHeaders, IntoResponse},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::OnceCell;
use tracing::{debug, error};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

static PDF: OnceCell<Box<[u8]>> = OnceCell::const_new();
static INFO: OnceCell<CompetitionInfo> = OnceCell::const_new();
static RAW_INFO: OnceCell<CompetitionInfo> = OnceCell::const_new();

#[derive(Serialize, ToSchema)]
pub struct CompetitionInfo {
    title: String,
    preamble: Option<String>,
    problems: Vec<String>,
    #[schema(value_type = String)]
    version: semver::Version,
    time_limit_secs: u64,
    languages: Vec<String>,
}

impl CompetitionInfo {
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
    pub fn new(state: &AppState) -> Result<Self, StatusCode> {
        Ok(Self::new_with_preamble(
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
                    StatusCode::INTERNAL_SERVER_ERROR
                })?,
        ))
    }
}

#[derive(Deserialize, IntoParams)]
pub struct InfoQuery {
    #[serde(default)]
    #[param(default = false)]
    raw_markdown: bool,
}

#[axum::debug_handler]
#[utoipa::path(get, tag = "competition", path = "/", params(InfoQuery), responses((status = OK, body = CompetitionInfo, content_type = "application/json")))]
pub async fn get_info(
    State(state): State<Arc<AppState>>,
    Query(query): Query<InfoQuery>,
) -> Result<Json<&'static CompetitionInfo>, StatusCode> {
    if query.raw_markdown {
        let info = RAW_INFO
            .get_or_init(|| async { CompetitionInfo::new_raw(&state) })
            .await;

        return Ok(Json(info));
    }

    // NOTE: we can't use get_or_init because we need this to give an error
    let info = match INFO.get() {
        Some(info) => info,
        None => {
            let info = CompetitionInfo::new(&state)?;
            // if this fails, another thread set the cell, so it's fine
            let _ = INFO.set(info);
            INFO.get().unwrap()
        }
    };
    Ok(Json(info))
}

#[axum::debug_handler]
#[utoipa::path(get, tag = "competition", path = "/packet", responses((status = OK, body = Vec<u8>, content_type = "application/pdf")))]
pub async fn download_packet(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, StatusCode> {
    PDF.get_or_try_init(|| async {
        debug!("Rendering packet PDF");
        state.config.render_pdf(None).map(Vec::into_boxed_slice)
    })
    .await
    .map(|x| {
        (
            AppendHeaders([
                (header::CONTENT_TYPE, "application/pdf"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"competition.pdf\"",
                ),
            ]),
            x.as_ref(),
        )
    })
    .map_err(|err| {
        error!("Error while rendering packet PDF: {:?}", err);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(download_packet))
        .routes(routes!(get_info))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
