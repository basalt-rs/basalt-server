use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use tracing::{debug, trace};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    extractors::auth::AuthUser,
    repositories::{
        self,
        users::{Role, User, UserLogin},
    },
    server::{teams::TeamWithScore, AppState},
    services::ws::{Broadcast, WebSocketSend},
};

#[derive(serde::Deserialize, utoipa::ToSchema)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct LoginResponse {
    token: String,
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
    trace!(login.username, "attempt to login to user");
    let sql = state.db.read().await;

    let login = UserLogin {
        username: login.username,
        password: login.password.into(),
    };

    let Ok(user) = repositories::users::login_user(&sql.db, &login).await else {
        debug!(login.username, "failed login attempt");
        return Err(StatusCode::UNAUTHORIZED);
    };

    let token = repositories::session::create_session(&sql.db, &user)
        .await
        .unwrap();
    let score = repositories::submissions::get_user_score(&sql.db, &user.username)
        .await
        .unwrap();
    drop(sql);

    state.team_manager.check_in(&user.username);

    state.team_manager.get_team(&user.username).map(|team| {
        state.broadcast(WebSocketSend::Broadcast {
            broadcast: Broadcast::TeamConnected(TeamWithScore {
                score,
                team_info: team,
            }),
        })
    });

    let role = user.role;
    debug!(login.username, "log in");

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
async fn logout(State(state): State<Arc<AppState>>, user: AuthUser) -> Result<(), StatusCode> {
    debug!(?user.user.username, "logout");

    let score = {
        let sql = state.db.read().await;

        repositories::session::close_session(&sql.db, &user.session_id)
            .await
            .unwrap();

        repositories::submissions::get_user_score(&sql.db, &user.user.username)
            .await
            .unwrap()
    };

    state.team_manager.disconnect(&user.user.username);

    state
        .team_manager
        .get_team(&user.user.username)
        .map(|team| {
            state.broadcast(crate::services::ws::WebSocketSend::Broadcast {
                broadcast: crate::services::ws::Broadcast::TeamDisconnected(TeamWithScore {
                    score,
                    team_info: team,
                }),
            })
        });

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
