use crate::{
    extractors::auth::AuthUser,
    repositories::{self, users::QuestionState},
    server::AppState,
};
use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(serde::Serialize, utoipa::ToSchema, Copy, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QuestionSubmissionState {
    state: QuestionState,
    remaining_attempts: u32,
}

#[axum::debug_handler]
#[utoipa::path(get, path = "/state", responses((status = OK, body = Vec<QuestionSubmissionState>, content_type = "application/json")))]
pub async fn get_submissions_state(
    AuthUser { user, .. }: AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<QuestionSubmissionState>>, StatusCode> {
    let sql = state.db.read().await;

    // TODO: add this to the config
    const MAX_ATTEMPTS: u32 = 5;

    let mut states = vec![
        QuestionSubmissionState {
            state: QuestionState::NotAttempted,
            remaining_attempts: MAX_ATTEMPTS
        };
        state.config.packet.problems.len()
    ];

    match repositories::submissions::get_latest_submissions(&sql.db, &user.username).await {
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

    match repositories::submissions::count_tests(&sql.db, &user.username).await {
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

    match repositories::submissions::get_attempts(&sql.db, &user.username).await {
        Ok(attempts) => {
            for a in attempts {
                states[a.question_index as usize].remaining_attempts =
                    MAX_ATTEMPTS - a.attempts as u32;
            }
        }
        Err(err) => {
            tracing::error!("Error while getting attempts: {}", err);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    Ok(Json(states))
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(get_submissions_state))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
