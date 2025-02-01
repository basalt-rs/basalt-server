use std::sync::Arc;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::server::AppState;

mod login {
    use std::sync::Arc;

    use axum::{extract::State, response::Response, Json};

    use crate::{repositories, server::AppState};

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
    #[utoipa::path(post, path = "/login", responses((status = OK, body = LoginSuccessResponse, content_type = "application/json")))]
    pub async fn post(State(state): State<Arc<AppState>>, Json(body): Json<LoginBody>) -> Response {
        let db = state.db.read().await;
        repositories::users::get_user_by_username(&db, body.username)
            .await
            .unwrap();
        todo!()
    }
}

pub fn auth_router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(login::post))
}

pub fn auth_service() -> axum::Router<Arc<AppState>> {
    auth_router().split_for_parts().0
}
