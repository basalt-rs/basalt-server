use axum::routing::post;
use std::sync::Arc;

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
    #[utoipa::path(post, path = "/auth/login", responses((status = OK, body = LoginSuccessResponse)))]
    pub async fn post(State(_): State<Arc<AppState>>, Json(_): Json<LoginBody>) -> Response {
        todo!()
    }
}

pub fn auth_service(initial_state: Arc<AppState>) -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/login", post(login::post))
        .with_state(initial_state)
}
