use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use axum::{extract::State, http::StatusCode, Json};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    extractors::auth::{create_jwt, JWTUser},
    repositories::{
        self,
        users::{Role, UserLogin},
    },
    server::AppState,
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
    let db = state.db.read().await;

    let login = UserLogin {
        username: login.username,
        // TODO: hash password
        password_hash: login.password.into(),
    };

    let Ok(user) = repositories::users::login_user(&db, login).await else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let session_id = repositories::session::create_session(&db, &user)
        .await
        .unwrap();
    let role = user.role;

    let expire = Duration::from_secs(60 * 60 * 24 * 30) // 30 days
        + SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();

    let token = create_jwt(&JWTUser {
        user,
        session_id,
        exp: expire.as_secs(),
    });

    // Send the authorized token
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
async fn logout(State(state): State<Arc<AppState>>, user: JWTUser) -> Result<(), StatusCode> {
    let db = state.db.read().await;

    repositories::session::close_session(&db, &user.session_id)
        .await
        .unwrap();

    Ok(())
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path="/validate", tag="auth",
    description="Validate that the auth token provided is still valid",
    responses(
        (status=OK, description="Auth token is still valid"),
        (status=401, description="Auth token is expired"),
    )
)]
async fn validate(State(_state): State<Arc<AppState>>, _user: JWTUser) -> Result<(), StatusCode> {
    Ok(())
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(login))
        .routes(routes!(logout))
        .routes(routes!(validate))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
