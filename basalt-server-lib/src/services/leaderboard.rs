use crate::{
    repositories::{
        self,
        users::{QuestionState, Username},
    },
    server::AppState,
};
use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TeamProgression {
    username: Username,
    score: f64,
    submission_states: Vec<QuestionState>,
}

#[axum::debug_handler]
#[utoipa::path(
    get, path = "/",
    tag = "leaderboard",
    description = "Gets all team's submission states and total number of points",
    responses(
        (status = OK, body = Vec<TeamProgression>, content_type = "application/json"),
        (status = 403, description = "User does not have permission to view the leaderboard"),
    ),
)]
pub async fn get_leaderboard_info(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<TeamProgression>>, StatusCode> {
    let competitors: Vec<Username> = state
        .config
        .accounts
        .competitors
        .iter()
        .map(|user| (user.name.clone().into()))
        .collect();

    let sql = state.db.read().await;

    let mut leaderboard_info = Vec::with_capacity(competitors.len());

    for username in &competitors {
        // Get list size and sets values to not-attempted by default
        let mut submission_states =
            vec![QuestionState::NotAttempted; state.config.packet.problems.len()];

        let submissions =
            match repositories::submissions::get_latest_submissions(&sql.db, username).await {
                Ok(submissions) => submissions,
                Err(err) => {
                    tracing::error!("Error while getting submissions: {}", err);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            };

        for s in submissions {
            submission_states[s.question_index as usize] = if s.success {
                QuestionState::Pass
            } else {
                QuestionState::Fail
            };
        }

        match repositories::submissions::count_tests(&sql.db, username).await {
            Ok(counts) => {
                for c in counts {
                    if submission_states[c.question_index as usize] == QuestionState::NotAttempted {
                        submission_states[c.question_index as usize] = if c.count > 0 {
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

        let score = match repositories::submissions::get_user_score(&sql.db, username).await {
            Ok(score) => score,
            Err(err) => {
                tracing::error!("Error while getting score: {}", err);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

        leaderboard_info.push(TeamProgression {
            username: username.clone(),
            score,
            submission_states,
        });
    }

    Ok(Json(leaderboard_info))
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(get_leaderboard_info))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
