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
    server::AppState,
};

#[derive(serde::Deserialize, utoipa::ToSchema)]
struct PauseRequest {
    username: String,
    password: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct PauseResponse {
    token: String,
    role: Role,
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path="/", tag="auth", request_body=PauseRequest,
    responses(
        (status=OK, body=PauseResponse, description="Game is paused"),
        (status=401, description="Insufficient permissions"),
    )
)]
async fn pause(
    State(state): State<Arc<AppState>>,
    Json(login): Json<PauseRequest>,
) -> Result<Json<PauseResponse>, StatusCode> {
    trace!(login.username, "attempt to login to user");
    let db = state.db.read().await;

    let login = UserLogin {
        username: login.username,
        password: login.password.into(),
    };

    let Ok(user) = repositories::users::login_user(&db, &login).await else {
        debug!(login.username, "failed login attempt");
        return Err(StatusCode::UNAUTHORIZED);
    };

    let token = repositories::session::create_session(&db, &user)
        .await
        .unwrap();
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
