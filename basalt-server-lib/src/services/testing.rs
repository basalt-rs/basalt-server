use crate::{
    repositories::{
        self,
        submissions::{SubmissionHistory, SubmissionId},
        users::{QuestionState, Role, User, UserId},
    },
    server::{tester::TestData, AppState},
    services::ws::WebSocketSend,
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use bedrock::scoring::Scorable;
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
use tokio::sync::oneshot;
use tracing::{debug, error};
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
    test_only: bool,
    language: String,
    solution: String,
}

enum TestWsSend {
    Error,
    Complete,
    Result(TestResult<TestData>),
}

fn spawn_ws_sender(
    state: Arc<AppState>,
    id: SubmissionId,
    user_id: UserId,
) -> tokio::sync::mpsc::UnboundedSender<TestWsSend> {
    let (result_tx, mut result_rx) = tokio::sync::mpsc::unbounded_channel::<TestWsSend>();
    tokio::spawn(async move {
        let mut results = Vec::new();
        while let Some(r) = result_rx.recv().await {
            tokio::time::sleep(Duration::from_millis(100)).await; // debounce
            let Some(websocket_sender) = state
                .websocket
                .wait_for_connection(user_id, Duration::from_secs(30))
                .await
            else {
                debug!("No WS connection after timeout of 30s");
                continue;
            };

            let get_test_results = || async {
                match repositories::submissions::get_test_results(&state.db, id).await {
                    Ok(v) => v.into_iter().map(Into::into).collect(),
                    Err(error) => {
                        error!(?error, "Error getting test results");
                        vec![]
                    }
                }
            };

            let mut send = match r {
                TestWsSend::Error => Some(WebSocketSend::TestsError { id }),
                TestWsSend::Complete => Some(WebSocketSend::TestsComplete {
                    id,
                    results: get_test_results().await,
                }),
                TestWsSend::Result(ref r) => {
                    results.push(r.into());
                    None
                }
            };
            while let Ok(ref v) = result_rx.try_recv() {
                match v {
                    TestWsSend::Error => {
                        send = Some(WebSocketSend::TestsError { id });
                    }
                    TestWsSend::Complete => {
                        send = Some(WebSocketSend::TestsComplete {
                            id,
                            results: get_test_results().await,
                        });
                    }
                    TestWsSend::Result(ref r) => results.push(r.into()),
                }
            }

            if let Some(send) = send {
                if websocket_sender.send(send).is_err() {
                    debug!("Websocket closed while trying to send test finish");
                }
                return;
            } else if websocket_sender
                .send(WebSocketSend::TestResults {
                    id,
                    results: results.clone(),
                })
                .is_err()
            {
                debug!("Websocket closed while trying to send test results");
                // hold onto the results for the next cycle and send them then
            } else {
                results.clear();
            }
        }
    });

    result_tx
}

#[axum::debug_handler]
#[utoipa::path(
    post, path = "/run-tests", tag = "testing",
    request_body = RunTestsBody,
    responses(
        (status = OK),
        (status = 403, description = ""),
    )
)]
// TODO: Test spawn error
// TODO: Better API
// TODO: Abortion
pub async fn run_tests(
    user: User,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RunTestsBody>,
) -> Result<Json<SubmissionId>, StatusCode> {
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

    let (abort_tx, abort_rx) = oneshot::channel();
    let id = SubmissionId::new();
    state.tester.add_abort_handle(id, abort_tx);
    tokio::spawn(async move {
        let (runner, source_file) = state
            .tester
            .runner(&body.language, body.question_index)
            .expect("checked above");

        let compiled = runner
            .file(BorrowedFileContent::string(&body.solution), source_file)
            .filter_tests(if body.test_only {
                |t| t.data().visible
            } else {
                |_| true
            })
            .compile()
            .await;

        let result_tx = spawn_ws_sender(Arc::clone(&state), id, user.id);

        // NOTE: This exists since the `?` operator calls `.into()` and `()` implements `From` for
        // any `T`, but we want all errors to be handled, not just disappeared.
        struct Unit;
        impl From<()> for Unit {
            fn from((): ()) -> Self {
                Self
            }
        }

        let result: Result<(), Unit> = async {
            let other_completions =
                repositories::submissions::count_other_submissions(&state.db, body.question_index)
                    .await
                    .map_err(|error| error!(?error, "Error counting other submissions"))?;

            let previous_attempts = repositories::submissions::count_previous_submissions(
                &state.db,
                &user.id,
                body.question_index,
            )
            .await
            .map_err(|error| error!(?error, "Error counting previous submissions"))?;

            let compiled = match compiled {
                Err(CompileError::CompileFail(compile_result)) => {
                    repositories::submissions::create_failed_submission_history(
                        &state.db,
                        repositories::submissions::NewSubmissionHistory {
                            id,
                            submitter: &user.id,
                            code: &body.solution,
                            question_index: body.question_index,
                            language: &body.language,
                            compile_result: Some(&compile_result),
                            test_only: body.test_only,
                        },
                    )
                    .await
                    .map_err(|error| error!(?error, "Error creating failed submission history"))?;

                    return Err(Unit);
                }
                Err(error) => {
                    error!(?error, "Error spawning compile command");
                    return Err(Unit);
                }
                Ok(compiled) => compiled,
            };

            let submission = repositories::submissions::create_submission_history(
                &state.db,
                repositories::submissions::NewSubmissionHistory {
                    id,
                    submitter: &user.id,
                    code: &body.solution,
                    question_index: body.question_index,
                    language: &body.language,
                    compile_result: compiled.compile_result(),
                    test_only: body.test_only,
                },
            )
            .await
            .map_err(|error| error!(?error, "Error adding submission to database"))?;

            let mut handle = compiled.run();

            let start = Instant::now();
            let mut passed = 0;
            let mut failed = 0;
            while let Some(result) = handle
                .wait_next()
                .await
                .map_err(|error| error!(?error, "Error running test"))?
            {
                if result.state() == TestResultState::Pass {
                    passed += 1;
                } else {
                    failed += 1;
                }

                let res = repositories::submissions::create_test_results(
                    &state.db,
                    &submission.id,
                    result.index(),
                    (&result).into(),
                )
                .await;

                if let Err(error) = res {
                    error!(?error, "Error adding submission test to database");
                    if let Err(error) = submission.fail(&state.db).await {
                        error!(?error, "Error updating submission to failed in database");
                    }
                    return Err(Unit);
                };

                let _ = result_tx.send(TestWsSend::Result(result));
            }

            let elapsed = start.elapsed();
            let score = state.config.score(
                body.question_index,
                bedrock::scoring::EvaluationContext {
                    num_completions: other_completions,
                    num_attempts: previous_attempts,
                    passed_tests: passed,
                    failed_tests: failed,
                    number_tests: passed + failed,
                },
            );
            let score = match score {
                Ok(score) => score,
                Err(error) => {
                    error!(?error, "Error calculating score of submission");
                    if let Err(error) = submission.fail(&state.db).await {
                        error!(?error, "Error updating submission to failed in database");
                    }
                    return Err(Unit);
                }
            };

            submission
                .finish(&state.db, score, failed == 0, elapsed)
                .await
                .map_err(|error| error!(?error, "Error updating submission in database"))?;

            Ok::<(), Unit>(())
        }
        .await;

        match result {
            Ok(_) => {
                let _ = result_tx.send(TestWsSend::Complete);
            }
            Err(_) => {
                let _ = result_tx.send(TestWsSend::Error);
            }
        }
        // ensure that we don't try to use the sender after we've sent Complete or Error
        drop(result_tx);
    });

    Ok(Json(id))
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
