use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use tracing::{error, info, trace};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    extractors::auth::{AuthUser, HostUser},
    repositories::{self, submissions::get_user_score, users::User},
    server::{teams::TeamWithScore, AppState},
};

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all(serialize = "camelCase", deserialize = "camelCase"))]
struct TeamsListResponse(Vec<TeamWithScore>);

#[axum::debug_handler]
#[utoipa::path(
    get,
    path="/", tag="teams",
    responses(
        (status=OK, body=TeamsListResponse, description="Information about teams"),
        (status=INTERNAL_SERVER_ERROR, description=""),
    )
)]
async fn get_teams(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TeamsListResponse>, StatusCode> {
    trace!("user getting teams info");
    let teams = state.team_manager.list();
    let mut joinset = JoinSet::new();
    for t in teams {
        let state = Arc::clone(&state);
        joinset.spawn(async move {
            let sql = state.db.read().await;
            get_user_score(&sql.db, &t.team)
                .await
                .map(|score| TeamWithScore {
                    team_info: t,
                    score,
                })
        });
    }
    joinset
        .join_all()
        .await
        .into_iter()
        .collect::<anyhow::Result<Vec<TeamWithScore>>>()
        .map_err(|e| {
            error!("Failed to retrieve scores for teams: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
        .map(TeamsListResponse)
        .map(Json)
        .map(Ok)?
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct NewTeam {
    username: String,
    display_name: Option<String>,
    password: String,
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path="/", tag="teams",
    request_body = NewTeam,
    responses(
        (status=OK, body=User, description="Team was created successfully"),
        (status=INTERNAL_SERVER_ERROR),
    )
)]
async fn add_team(
    State(state): State<Arc<AppState>>,
    HostUser(AuthUser { user: creator, .. }): HostUser,
    Json(new): Json<NewTeam>,
) -> Result<Json<User>, StatusCode> {
    let sql = state.db.read().await;
    info!(%creator.username, %new.username, "Creating new user");
    let user = repositories::users::create_user(
        &sql.db,
        new.username,
        new.display_name.as_deref(),
        new.password,
        repositories::users::Role::Competitor,
    )
    .await
    .map_err(|e| {
        error!("Error creating user: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(user))
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(get_teams))
        .routes(routes!(add_team))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}

#[cfg(test)]
mod tests {
    use bedrock::Config;

    use crate::{
        repositories::users::get_user_by_username,
        testing::{mock_db, SAMPLE_1},
    };

    use super::*;
    #[tokio::test]
    async fn get_teams_works() {
        let (f, sql) = mock_db().await;

        let expected_score = 3.0;

        let cfg = Config::from_str(SAMPLE_1, "Single.toml".into()).unwrap();
        sql.ingest(&cfg).await.unwrap();

        let user1 = get_user_by_username(&sql, "team1".into()).await.unwrap();

        crate::testing::submissions_repositories::dummy_submission(
            &sql.db,
            &user1,
            expected_score / 2.0,
        )
        .await;
        crate::testing::submissions_repositories::dummy_submission(
            &sql.db,
            &user1,
            expected_score / 2.0,
        )
        .await;

        let appstate = AppState::new(sql, cfg, None);

        let teams = get_teams(State(Arc::new(appstate))).await.unwrap().0 .0;
        assert_eq!(
            teams
                .into_iter()
                .find(|t| t.team_info.team == user1.username)
                .unwrap()
                .score,
            expected_score
        );
        drop(f);
    }
}
