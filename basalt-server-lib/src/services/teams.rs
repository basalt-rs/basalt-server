use std::sync::Arc;

use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use tracing::{error, info, trace};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    extractors::auth::HostUser,
    repositories::{
        self,
        submissions::get_user_score,
        users::{GetUserError, User, UserId, Username},
    },
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
        (status=CONFLICT, description="Team with provided username already exists"),
        (status=INTERNAL_SERVER_ERROR),
    )
)]
async fn add_team(
    State(state): State<Arc<AppState>>,
    HostUser(creator): HostUser,
    Json(new): Json<NewTeam>,
) -> Result<Json<User>, StatusCode> {
    let sql = state.db.read().await;
    info!(creator = %creator.username, new = %new.username, "Creating new user");
    let user = repositories::users::create_user(
        &sql.db,
        new.username,
        new.display_name.as_deref(),
        new.password,
        repositories::users::Role::Competitor,
    )
    .await
    .map_err(|e| match e {
        repositories::users::CreateUserError::Confict => {
            info!("User not created due to username conflict");
            StatusCode::CONFLICT
        }
        repositories::users::CreateUserError::Other(_) => {
            error!("Error creating user: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    Ok(Json(user))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum DisplayNamePatch {
    Remove,      // "remove"
    Set(String), // { "set": "New Name" }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct PatchTeam {
    username: Option<Username>,
    display_name: Option<DisplayNamePatch>,
    password: Option<String>,
}

#[axum::debug_handler]
#[utoipa::path(
    patch,
    path="/{id}", tag="teams",
    request_body = PatchTeam,
    responses(
        (status=OK, body=User, description="Team was succesfully updated"),
        (status=NOT_FOUND, description="User with ID not found"),
        (status=CONFLICT, description="Team with provided username already exists"),
        (status=INTERNAL_SERVER_ERROR),
    )
)]
async fn patch_team(
    State(state): State<Arc<AppState>>,
    HostUser(host): HostUser,
    Path(user_id): Path<UserId>,
    Json(patch): Json<PatchTeam>,
) -> Result<Json<User>, StatusCode> {
    let sql = state.db.read().await;
    info!(host = %host.username, %user_id, ?patch, "Patching user");
    let mut user = repositories::users::get_user_by_id(&sql.db, user_id)
        .await
        .map_err(|e| match e {
            GetUserError::QueryError(_) => {
                error!("Error creating user: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            }
            GetUserError::UserNotFound { .. } => {
                info!("User not found");
                StatusCode::NOT_FOUND
            }
        })?;

    if let Some(username) = patch.username {
        user.username = username;
    }

    if let Some(display_name) = patch.display_name {
        match display_name {
            DisplayNamePatch::Remove => user.display_name = None,
            DisplayNamePatch::Set(name) => user.display_name = Some(name),
        }
    }

    if let Some(password) = patch.password {
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .expect("Failed to hash password")
            .to_string();
        user.password_hash = password_hash.into();
    }

    let new = repositories::users::update_user(&sql.db, user)
        .await
        .map_err(|e| {
            error!("Error updating user: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(new))
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(get_teams))
        .routes(routes!(add_team))
        .routes(routes!(patch_team))
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
                .find(|t| t.team_info.team == user1.id)
                .unwrap()
                .score,
            expected_score
        );
        drop(f);
    }
}
