use std::sync::Arc;

use axum::{extract::State, Json};
use tracing::trace;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::server::{teams::TeamFull, AppState};

#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all(serialize = "camelCase", deserialize = "camelCase"))]
struct TeamsListResponse(Vec<TeamFull>);

#[axum::debug_handler]
#[utoipa::path(
    get,
    path="/", tag="teams",
    responses(
        (status=OK, body=TeamsListResponse, description="Information about teams"),
    )
)]
async fn get_teams(State(state): State<Arc<AppState>>) -> Json<TeamsListResponse> {
    trace!("user getting teams info");
    let teams = state.team_manager.list();
    Json(TeamsListResponse(teams))
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(get_teams))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
