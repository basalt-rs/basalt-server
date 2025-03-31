use std::{sync::Arc, time::Duration};

use axum::{extract::State, http::StatusCode, Json};
use bedrock::{Game, PointsSettings};
use tracing::{debug, trace};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    extractors::auth::AuthUser,
    repositories::{
        self,
        users::{Role, User, UserLogin},
    },
    server::AppState,
};

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum UpdateClockRequest {
    PauseUpdate { is_paused: bool },
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct UpdateClockResponse {
    is_paused: bool,
    time_left_seconds: u64,
}

#[axum::debug_handler]
#[utoipa::path(
    patch,
    path="/", tag="auth", request_body=UpdateClockRequest,
    responses(
        (status=OK, body=UpdateClockResponse, description="Game is paused"),
        (status=UNAUTHORIZED, description="Insufficient permissions"),
    )
)]
async fn update_clock(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(update): Json<UpdateClockRequest>,
) -> Result<Json<UpdateClockResponse>, StatusCode> {
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
        Ok(current_time) => Ok(Json(UpdateClockResponse {
            is_paused: current_time.paused,
            time_left_seconds: current_time.time_left(time_limit).as_secs(),
        })),
        Err(_) => Ok(Json(UpdateClockResponse {
            is_paused: false,
            time_left_seconds: Duration::from_secs(0).as_secs(),
        })),
    }
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path="/logout", tag="auth",
    responses(
        (status=OK, description="User has been logged out"),
        (status=401, description="User was not logged in"),
    )
)]
async fn logout(State(state): State<Arc<AppState>>, user: AuthUser) -> Result<(), StatusCode> {
    debug!(user.user.username, "logout");
    let db = state.db.read().await;

    repositories::session::close_session(&db, &user.session_id)
        .await
        .unwrap();

    Ok(())
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path="/me", tag="auth",
    description="Get information about the current user",
    responses(
        (status=OK, body=User, description="User is signed in"),
        (status=401, description="Auth token is expired"),
    )
)]
async fn me(State(_state): State<Arc<AppState>>, user: AuthUser) -> Result<Json<User>, StatusCode> {
    Ok(Json(user.user))
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(login))
        .routes(routes!(logout))
        .routes(routes!(me))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
