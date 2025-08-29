use crate::{
    define_id_type,
    repositories::{
        self,
        submissions::SubmissionHistory,
        users::{QuestionState, Role, User, UserId},
    },
    server::{tester::TestData, websocket::ConnectionKind, AppState},
    services::ws::WebSocketSend,
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use erudite::{
    error::CompileError,
    runner::{TestResult, TestResultState},
    BorrowedFileContent,
};
use serde::{Deserialize, Serialize};
use std::{
    num::NonZero,
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::{debug, error, trace, warn};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(Debug, Serialize, ToSchema, Copy, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QuestionSubmissionState {
    state: QuestionState,
    remaining_attempts: Option<u32>,
}

#[derive(Deserialize, ToSchema, Clone, IntoParams)]
pub struct SubmissionStateParams {
    user_id: Option<UserId>,
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
    user: User,
    Query(SubmissionStateParams { user_id }): Query<SubmissionStateParams>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<QuestionSubmissionState>>, StatusCode> {
    let user_id = if let Some(ref user_id) = user_id {
        if user.role == Role::Host {
            repositories::users::get_user_by_id(&state.db, user_id)
                .await
                .map_err(|e| match e {
                    repositories::users::GetUserError::QueryError(e) => {
                        error!("Error getting user: {:?}", e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    }
                    repositories::users::GetUserError::UserNotFound { .. } => StatusCode::NOT_FOUND,
                })?;
            user_id
        } else {
            return Err(StatusCode::FORBIDDEN);
        }
    } else {
        &user.id
    };
    let max_attempts = state.config.max_submissions.map(NonZero::get);

    let mut states = vec![
        QuestionSubmissionState {
            state: QuestionState::NotAttempted,
            remaining_attempts: max_attempts,
        };
        state.config.packet.problems.len()
    ];

    match repositories::submissions::get_latest_submissions(&state.db, user_id).await {
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

    match repositories::submissions::count_tests(&state.db, user_id).await {
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
            tracing::error!("Error while counting tests: {:?}", err);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    match repositories::submissions::get_attempts(&state.db, user_id).await {
        Ok(attempts) => {
            for a in attempts {
                states[a.question_index as usize].remaining_attempts =
                    max_attempts.map(|x| x - a.attempts as u32);
            }
        }
        Err(err) => {
            tracing::error!("Error while getting attempts: {:?}", err);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    Ok(Json(states))
}

#[derive(Deserialize, IntoParams)]
pub struct SubmissionsParams {
    user_id: Option<UserId>,
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
    user: User,
    params: Query<SubmissionsParams>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<SubmissionHistory>>, StatusCode> {
    let user_id = params.user_id.as_ref().unwrap_or(&user.id);
    if !(user.role == Role::Host || user.id == *user_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    let subs =
        match repositories::submissions::get_submissions(&state.db, user_id, params.question_index)
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct RunTestsBody {
    question_index: usize,
    language: String,
    solution: String,
}

define_id_type!(TestId);

#[axum::debug_handler]
#[utoipa::path(
    post, path = "/run-tests", tag = "testing",
    request_body = RunTestsBody,
    responses(
        (status = OK),
        (status = 403, description = ""),
    )
)]
pub async fn run_tests(
    user: User,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RunTestsBody>,
) -> Result<Json<TestId>, StatusCode> {
    tracing::debug!(?body, "run_tests");
    // NOTE: It's not great that we construct a test runner and then throw it await, but we can't
    // move the test runner into the new task, so it's fine (constructing one is really cheap).
    let runner = state.tester.runner(&body.language, body.question_index);
    if runner.is_none() {
        // This should be prevented by the UI
        error!(
            language = body.language,
            question_index = body.question_index,
            "Missing runner for attempted test"
        );
        return Err(StatusCode::BAD_REQUEST);
    };

    let test_id = TestId::new();

    tokio::spawn(async move {
        let (runner, source_file) = state
            .tester
            .runner(&body.language, body.question_index)
            .expect("checked above");

        let compiled = runner
            .file(BorrowedFileContent::string(&body.solution), source_file)
            .compile()
            .await;

        let compiled = match compiled {
            Err(CompileError::CompileFail(compile_result)) => {
                let res = repositories::submissions::create_failed_submission_history(
                    &state.db,
                    repositories::submissions::NewSubmissionHistory {
                        submitter: &user.id,
                        code: &body.solution,
                        question_index: body.question_index,
                        language: &body.language,
                        compile_result: Some(&compile_result),
                    },
                )
                .await;

                // TODO: alert user
                if let Err(error) = res {
                    error!(?error, "Error creating failed submission history");
                }
                return;
            }
            Err(error) => {
                // TODO: alert user
                error!(?error, "Error spawning compile command");
                return;
            }
            Ok(compiled) => compiled,
        };

        let res = repositories::submissions::create_submission_history(
            &state.db,
            repositories::submissions::NewSubmissionHistory {
                submitter: &user.id,
                code: &body.solution,
                question_index: body.question_index,
                language: &body.language,
                compile_result: compiled.compile_result(),
            },
        )
        .await;

        let submission = match res {
            Ok(h) => h,
            Err(error) => {
                // TODO: alert the client somehow (scopeguard-type thing?)
                error!(?error, "Error adding submission to database");
                return;
            }
        };

        let mut handle = compiled.run();

        let test_count = handle.test_count();
        let result_tx = {
            let (result_tx, mut result_rx) =
                tokio::sync::mpsc::channel::<TestResult<TestData>>(test_count);
            let state = Arc::clone(&state);
            let user_id = user.id;
            tokio::spawn(async move {
                // it's fairly likely that all tests will finish within one debounce, so let's
                // allocate all of them
                let mut results = Vec::with_capacity(test_count);
                while let Some(r) = result_rx.recv().await {
                    trace!("Got an item");
                    tokio::time::sleep(Duration::from_millis(100)).await; // debounce
                    trace!("Waiting for websocket connection");
                    let Some(websocket_sender) = state
                        .websocket
                        .wait_for_connection(user_id, Duration::from_secs(5))
                        .await
                    else {
                        debug!("No WS connection after timeout of 5s");
                        // if no connection after five seconds, we can just quit assume that the
                        // websocket is disconnected and the client will request the results later
                        return;
                    };

                    results.push((&r).into());
                    while let Ok(ref v) = result_rx.try_recv() {
                        results.push(v.into());
                    }

                    if websocket_sender
                        .send(WebSocketSend::TestResults {
                            id: test_id,
                            results: results.clone(),
                        })
                        .is_err()
                    {
                        debug!("Websocket closed while trying to send test results");
                        return; // we can't do anything else
                    }
                    results.clear();
                }
            });

            result_tx
        };

        let start = Instant::now();
        let mut success = true;
        loop {
            let result = match handle.wait_next().await {
                Ok(None) => break,          // we're done (no more tests)
                Ok(Some(result)) => result, // we have a result
                Err(_) => todo!(),          // there was an error spawning the test
            };
            tracing::info!(?result, "test result!");

            if result.state() != TestResultState::Pass {
                success = false;
            }

            let res = repositories::submissions::create_submission_test_history(
                &state.db,
                &submission.id,
                result.index(),
                (&result).into(),
            )
            .await;

            let test_hist = match res {
                Ok(h) => h,
                Err(error) => {
                    // TODO: alert the client somehow (scopeguard-type thing?)
                    error!(?error, "Error adding submission test to database");
                    // TODO: complain about this error
                    if let Err(error) = submission.fail(&state.db).await {
                        error!(?error, "Error updating submission to failed in database");
                    }
                    return;
                }
            };

            // if the result_rx is dropped, we don't really care
            let _ = result_tx.send(result).await;
        }

        let elapsed = start.elapsed();
        let res = submission.finish(&state.db, 0.0, success, elapsed).await;

        let submission = match res {
            Ok(h) => h,
            Err(error) => {
                // TODO: alert the client somehow (scopeguard-type thing?)
                error!(?error, "Error updating submission in database");
                return;
            }
        };
    });

    Ok(Json(test_id))
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(get_submissions_state))
        .routes(routes!(get_submissions))
        .routes(routes!(run_tests))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
