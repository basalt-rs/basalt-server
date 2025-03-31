use std::{sync::Arc, time::Duration};

use axum::{extract::State, http::StatusCode, Json};
use bedrock::{Game, PointsSettings};
use tracing::trace;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{extractors::auth::AuthUser, server::AppState};

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum UpdateClockRequest {
    PauseUpdate { is_paused: bool },
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct ClockStatusResponse {
    is_paused: bool,
    time_left_seconds: u64,
}

#[axum::debug_handler]
#[utoipa::path(
    patch,
    path="/", tag="clock", request_body=UpdateClockRequest,
    responses(
        (status=OK, body=ClockStatusResponse, description="Game is paused"),
        (status=UNAUTHORIZED, description="Insufficient permissions"),
    )
)]
async fn patch_clock(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(update): Json<UpdateClockRequest>,
) -> Result<Json<ClockStatusResponse>, StatusCode> {
    trace!(user.user.username, "attempt to pause server");

    let time_limit = match state.config.game {
        // TODO: When time_limit is made public, update this
        Game::Points(PointsSettings { .. }) => Duration::from_secs(60 * 75),
        // TODO: When other modes are supported, provide correct values
        _ => Duration::from_secs(60 * 75),
    };

    let mut clock = state.clock.write().await;
    match update {
        UpdateClockRequest::PauseUpdate { is_paused: true } => {
            clock.pause();
        }
        UpdateClockRequest::PauseUpdate { is_paused: false } => {
            clock.unpause();
        }
    }

    match clock.current_time() {
        Ok(current_time) => Ok(Json(ClockStatusResponse {
            is_paused: current_time.paused,
            time_left_seconds: current_time.time_left(time_limit).as_secs(),
        })),
        Err(_) => Ok(Json(ClockStatusResponse {
            is_paused: false,
            time_left_seconds: Duration::from_secs(0).as_secs(),
        })),
    }
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path="/", tag="clock",
    responses(
        (status=OK, body=ClockStatusResponse, description="Information about the clock"),
    )
)]
async fn get_clock(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClockStatusResponse>, StatusCode> {
    let time_limit = match state.config.game {
        // TODO: When time_limit is made public, update this
        Game::Points(PointsSettings { .. }) => Duration::from_secs(60 * 75),
        // TODO: When other modes are supported, provide correct values
        _ => Duration::from_secs(60 * 75),
    };
    let clock = state.clock.read().await;
    match clock.current_time() {
        Ok(current_time) => Ok(Json(ClockStatusResponse {
            is_paused: current_time.paused,
            time_left_seconds: current_time.time_left(time_limit).as_secs(),
        })),
        Err(_) => Ok(Json(ClockStatusResponse {
            is_paused: false,
            time_left_seconds: Duration::from_secs(0).as_secs(),
        })),
    }
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(get_clock))
        .routes(routes!(patch_clock))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
