use crate::{
    extractors::auth::AuthUser,
    repositories::{
        self,
        submissions::SubmissionHistory,
        users::{QuestionState, Role, Username},
    },
    server::AppState,
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::{num::NonZero, sync::Arc};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(Serialize, ToSchema, Copy, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QuestionSubmissionState {
    state: QuestionState,
    remaining_attempts: Option<u32>,
}

#[derive(Deserialize, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionStateParams {
    username: Option<Username>,
}

#[axum::debug_handler]
#[utoipa::path(
    get, path = "/state",
    tag = "testing",
    description = "Get the current state of the current user's submissions",
    responses(
        (status = OK, body = Vec<QuestionSubmissionState>, content_type = "application/json"),
        (status = 403, description = "User does not have permission to view the submissions for this user"),
    ),
)]
pub async fn get_submissions_state(
    AuthUser { user, .. }: AuthUser,
    Query(SubmissionStateParams { username }): Query<SubmissionStateParams>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<QuestionSubmissionState>>, StatusCode> {
    let username = if let Some(ref username) = username {
        if user.role == Role::Host {
            username
        } else {
            return Err(StatusCode::FORBIDDEN);
        }
    } else {
        &user.username
    };
    let max_attempts = state.config.max_submissions.map(NonZero::get);

    let sql = state.db.read().await;

    let mut states = vec![
        QuestionSubmissionState {
            state: QuestionState::NotAttempted,
            remaining_attempts: max_attempts,
        };
        state.config.packet.problems.len()
    ];

    match repositories::submissions::get_latest_submissions(&sql.db, username).await {
        Ok(submissions) => {
            for s in submissions {
                states[s.question_index as usize].state = if s.success {
                    QuestionState::Pass
                } else {
                    QuestionState::Fail
                }
            }
        }
        Err(err) => {
            tracing::error!("Error while getting submissions: {}", err);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    match repositories::submissions::count_tests(&sql.db, username).await {
        Ok(counts) => {
            for c in counts {
                if states[c.question_index as usize].state == QuestionState::NotAttempted {
                    states[c.question_index as usize].state = if c.count > 0 {
                        QuestionState::InProgress
                    } else {
                        QuestionState::NotAttempted
                    };
                }
            }
        }
        Err(err) => {
            tracing::error!("Error while getting attempts: {}", err);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    match repositories::submissions::get_attempts(&sql.db, username).await {
        Ok(attempts) => {
            for a in attempts {
                states[a.question_index as usize].remaining_attempts =
                    max_attempts.map(|x| x - a.attempts as u32);
            }
        }
        Err(err) => {
            tracing::error!("Error while getting attempts: {}", err);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    Ok(Json(states))
}

#[derive(Deserialize, IntoParams)]
pub struct SubmissionsParams {
    username: Option<Username>,
    question_index: usize,
}

#[axum::debug_handler]
#[utoipa::path(
    get, path = "/submissions", tag = "testing",
    params(SubmissionsParams),
    responses(
        (status = OK, body = Vec<SubmissionHistory>, content_type = "application/json"),
        (status = 403, description = "User does not have permission to view the submissions for this user"),
    )
)]
pub async fn get_submissions(
    AuthUser { user, .. }: AuthUser,
    params: Query<SubmissionsParams>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<SubmissionHistory>>, StatusCode> {
    let username = params.username.as_ref().unwrap_or(&user.username);
    if !(user.role == Role::Host || user.username == *username) {
        return Err(StatusCode::FORBIDDEN);
    }

    let sql = state.db.read().await;
    let subs =
        match repositories::submissions::get_submissions(&sql.db, username, params.question_index)
            .await
        {
            Ok(subs) => subs,
            Err(err) => {
                tracing::error!("Error getting subs for user: {}", err);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

    Ok(Json(subs))
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(get_submissions_state))
        .routes(routes!(get_submissions))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
