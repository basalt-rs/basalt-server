use axum::{routing::post, Router};
use std::sync::Arc;
use utoipa::openapi::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::server::AppState;

mod login {
    use std::sync::Arc;

    use axum::{extract::State, response::Response, Json};

    use crate::server::AppState;

    #[derive(serde::Deserialize, utoipa::ToSchema)]
    #[allow(dead_code)]
    pub struct LoginBody {
        username: String,
        password: String,
    }
    #[derive(serde::Serialize, utoipa::ToSchema)]
    pub struct LoginSuccessResponse {
        session_token: String,
    }
    #[axum::debug_handler]
    #[utoipa::path(post, path = "/auth/login", responses((status = OK, body = LoginSuccessResponse, content_type = "application/json")))]
    pub async fn post(State(_): State<Arc<AppState>>, Json(_): Json<LoginBody>) -> Response {
        todo!()
    }
}

pub fn auth_router(initial_state: Arc<AppState>) -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(login::post))
        .route("/login", post(login::post))
        .with_state(initial_state)
}

pub fn auth_service(initial_state: Arc<AppState>) -> axum::Router<Arc<AppState>> {
    auth_router(initial_state).split_for_parts().0
}
