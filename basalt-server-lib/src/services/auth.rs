use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use chrono::Local;
use tracing::{debug, error, trace};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    extractors::auth::UserWithSession,
    repositories::{
        self,
        session::SessionId,
        users::{Role, User, UserLogin},
    },
    server::{hooks::events::ServerEvent, teams::TeamWithScore, AppState},
    services::ws::{Broadcast, WebSocketSend},
};

#[derive(serde::Deserialize, utoipa::ToSchema)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct LoginResponse {
    token: SessionId,
    role: Role,
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path="/login", tag="auth", request_body=LoginRequest,
    responses(
        (status=OK, body=LoginResponse, description="Session cookie has been set"),
        (status=401, description="Incorrect credentials provided"),
    )
)]
async fn login(
    State(state): State<Arc<AppState>>,
    Json(login): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    trace!(%login.username, "attempt to login to user");
    let login = UserLogin {
        username: login.username,
        password: login.password.into(),
    };

    let Ok(user) = repositories::users::login_user(&state.db, &login).await else {
        debug!(%login.username, "failed login attempt");
        return Err(StatusCode::UNAUTHORIZED);
    };

    let token = repositories::session::create_session(&state.db, &user)
        .await
        .unwrap();
    let score = repositories::submissions::get_user_score(&state.db, &user.id)
        .await
        .unwrap();

    if state.team_manager.check_in(&user.id) {
        trace!("checking in user: {}", &user.username);
        if let Err(err) = (ServerEvent::OnCheckIn {
            id: user.id.clone(),
            time: Local::now().to_utc(),
        }
        .dispatch(state.clone()))
        {
            error!("error occurred dispatching event hook: {}", err.to_string());
        }
    }

    if let Some(team) = state.team_manager.get_team(&user.id) {
        let user = repositories::users::get_user_by_id(&state.db, &user.id)
            .await
            .map_err(|e| {
                error!("Error getting username: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        state.websocket.broadcast(WebSocketSend::Broadcast {
            broadcast: Broadcast::TeamConnected(TeamWithScore {
                score,
                id: user.id,
                name: user.username,
                display_name: user.display_name,
                team_info: team,
            }),
        });
    }

    let role = user.role;
    debug!(%login.username, "log in");

    Ok(Json(LoginResponse { token, role }))
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
async fn logout(
    State(state): State<Arc<AppState>>,
    UserWithSession(user, session_id): UserWithSession,
) -> Result<(), StatusCode> {
    debug!(?user.username, "logout");

    repositories::session::close_session(&state.db, &session_id)
        .await
        .unwrap();

    let score = repositories::submissions::get_user_score(&state.db, &user.id)
        .await
        .unwrap();

    state.team_manager.disconnect(&user.id);

    if let Some(team) = state.team_manager.get_team(&user.id) {
        let user = repositories::users::get_user_by_id(&state.db, &user.id)
            .await
            .map_err(|e| {
                error!("Error getting username: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        state
            .websocket
            .broadcast(crate::services::ws::WebSocketSend::Broadcast {
                broadcast: crate::services::ws::Broadcast::TeamDisconnected(TeamWithScore {
                    score,
                    id: user.id,
                    name: user.username,
                    display_name: user.display_name,
                    team_info: team,
                }),
            });
    }

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
async fn me(State(_state): State<Arc<AppState>>, user: User) -> Result<Json<User>, StatusCode> {
    Ok(Json(user))
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
