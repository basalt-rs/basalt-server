use std::{borrow::Cow, net::SocketAddr, sync::Arc};

use anyhow::{bail, Context};
use bedrock::{packet::Test, scoring::Scorable};
use erudite::{RunOutput, TestCase, TestFailReason, TestOutput};
use lazy_static::lazy_static;
use leucite::Rules;
use serde::{Deserialize, Serialize};
use sqlx::Acquire;
use tokio::sync::mpsc::{self, UnboundedSender};
use tracing::{debug, trace};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    extractors::auth::AuthUser,
    repositories::{
        self,
        submissions::{NewSubmissionHistory, NewSubmissionTestHistory, TestResult},
        users::{QuestionState, Username},
    },
    server::AppState,
};
pub mod connect;

#[derive(Clone, Eq, PartialEq, Hash, derive_more::Debug)]
pub enum ConnectionKind {
    User {
        user: AuthUser,
    },
    Leaderboard {
        id: String,
        #[debug(skip)]
        addr: SocketAddr,
    },
}

impl ConnectionKind {
    pub fn is_user(&self) -> bool {
        match self {
            ConnectionKind::User { .. } => true,
            ConnectionKind::Leaderboard { .. } => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectedClient {
    pub send: mpsc::UnboundedSender<WebSocketSend>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Broadcast {
    Announce { message: String },
}

/// A message that is sent from the server onto the websocket
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WebSocketSend {
    Broadcast {
        broadcast: Broadcast,
    },
    TeamUpdate {
        team: Username,
        new_score: f64,
        new_states: Vec<QuestionState>,
    },
    TestResults {
        id: usize,
        results: Vec<(TestOutput, Test)>,
        percent: usize,
    },
    Submit {
        id: usize,
        results: Vec<(TestOutput, Test)>,
        percent: usize,
    },
    Error {
        id: Option<usize>,
        message: String,
    },
}

/// A message that is recieved from the websocket
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WebSocketRecv<'a> {
    Broadcast {
        broadcast: Broadcast,
    },
    RunTest {
        id: usize,
        language: Cow<'a, str>,
        solution: Cow<'a, str>,
        problem: usize,
    },
    Submit {
        id: usize,
        language: Cow<'a, str>,
        solution: Cow<'a, str>,
        problem: usize,
    },
}

lazy_static! {
    static ref BUILD_RULES: Rules = Rules::new()
        .add_read_only("/usr")
        .add_read_only("/etc")
        .add_read_only("/dev")
        .add_read_only("/bin");
    static ref RUN_RULES: Rules = Rules::new()
        .add_read_only("/usr")
        .add_read_only("/etc")
        .add_read_only("/dev")
        .add_read_only("/bin");
}

impl WebSocketRecv<'_> {
    fn can_use(&self, who: &ConnectionKind) -> bool {
        match self {
            WebSocketRecv::Broadcast { .. } => true,
            WebSocketRecv::RunTest { .. } => who.is_user(),
            WebSocketRecv::Submit { .. } => who.is_user(),
        }
    }

    fn id(&self) -> Option<usize> {
        match self {
            WebSocketRecv::Broadcast { .. } => None,
            WebSocketRecv::RunTest { id, .. } => Some(*id),
            WebSocketRecv::Submit { id, .. } => Some(*id),
        }
    }

    fn error(
        &self,
        ws: &UnboundedSender<WebSocketSend>,
        message: impl Into<String>,
    ) -> anyhow::Result<()> {
        ws.send(WebSocketSend::Error {
            id: self.id(),
            message: message.into(),
        })
        .context("sending error message")
    }

    async fn run_test(
        &self,
        id: usize,
        language: &str,
        solution: &str,
        problem: usize,
        state: Arc<AppState>,
        who: &ConnectionKind,
    ) -> anyhow::Result<()> {
        let ws = &state
            .active_connections
            .get(who)
            .context("websocket not in active_connections")?
            .send;

        let Some(language) = state.config.languages.get_by_str(language) else {
            return self.error(ws, format!("Unknown language '{}'", language));
        };

        let key = (who.clone(), problem);
        if !state.active_tests.insert(key.clone()) {
            return self.error(ws, "Tests are already running");
        };

        scopeguard::defer! {
            state.active_tests.remove(&key);
        }

        let mut runner = erudite::Runner::new();
        let problem = &*state.config.packet.problems[problem];
        runner
            .create_file(language.source_file(), solution)
            .tests(
                problem
                    .tests
                    .iter()
                    .map(|t| TestCase::new(&t.input, &t.output)),
            )
            .timeout(state.config.test_runner.timeout)
            .trim_output(state.config.test_runner.trim_output)
            .compile_rules(BUILD_RULES.clone()) // TODO: Remove these clones
            .run_rules(RUN_RULES.clone())
            .run_command(language.run_command().split(" "));

        if let Some(cmd) = language.build_command() {
            runner.compile_command(cmd.split(" "));
        }

        let results = runner.run().await?;

        match results {
            RunOutput::CompileSpawnFail(s) => {
                bail!("Failed to spawn compile command: {:?}", s)
            }
            RunOutput::CompileFail(simple_output) => {
                debug!(?simple_output, "Failed to build");
            }
            RunOutput::RunSuccess(vec) => {
                trace!(?vec, "Raw test output");
                let success = vec
                    .iter()
                    .filter(|&r| matches!(r, TestOutput::Pass))
                    .count();

                let results = vec
                    .into_iter()
                    .zip(problem.tests.iter())
                    .filter(|(_, t)| t.visible)
                    .map(|(r, t)| (r, t.clone()))
                    .collect::<Vec<_>>();

                let percent = success * 100 / problem.tests.len();
                ws.send(WebSocketSend::TestResults {
                    id,
                    results,
                    percent,
                })
                .context("sending test results message")?;
            }
        }
        Ok(())
    }

    async fn run_submission(
        &self,
        id: usize,
        language: &str,
        solution: &str,
        problem_index: usize,
        state: Arc<AppState>,
        who: &ConnectionKind,
    ) -> anyhow::Result<()> {
        let ws = &state
            .active_connections
            .get(who)
            .context("websocket not in active_connections")?
            .send;

        let ConnectionKind::User {
            user: AuthUser { user, .. },
        } = who
        else {
            unreachable!("is_user called above")
        };

        let Some(language) = state.config.languages.get_by_str(language) else {
            return self.error(ws, format!("Unknown language '{}'", language));
        };

        let sql = state.db.read().await;
        let attempts = repositories::submissions::count_previous_submissions(
            &sql.db,
            &user.username,
            problem_index,
        )
        .await
        .context("getting previous submissions")?;
        if attempts >= 5 {
            // TODO: move this to the config
            return self.error(ws, "Only 5 submissions are allowed.");
        }
        drop(sql); // ensure we don't hold the lock while doing time-consuming things

        let key = (who.clone(), problem_index);
        if !state.active_submissions.insert(key.clone()) {
            return self.error(ws, "Submission is already running");
        };

        scopeguard::defer! {
            state.active_submissions.remove(&key);
        }

        let mut runner = erudite::Runner::new();
        let problem = &*state.config.packet.problems[problem_index];
        runner
            .create_file(language.source_file(), solution)
            .tests(
                problem
                    .tests
                    .iter()
                    .map(|t| TestCase::new(&t.input, &t.output)),
            )
            .timeout(state.config.test_runner.timeout)
            .trim_output(state.config.test_runner.trim_output)
            .compile_rules(BUILD_RULES.clone())
            .run_rules(RUN_RULES.clone())
            .run_command(language.run_command().split(" "));

        if let Some(cmd) = language.build_command() {
            runner.compile_command(cmd.split(" "));
        }

        let results = runner.run().await?;

        match results {
            RunOutput::CompileSpawnFail(s) => {
                let sql = state.db.read().await;
                repositories::submissions::create_submission_history(
                    &sql.db,
                    NewSubmissionHistory {
                        submitter: &user.username,
                        compile_fail: true,
                        code: solution,
                        question_index: problem_index,
                        score: 0.,
                        success: false,
                    },
                )
                .await
                .context("creating submission history")?;
                tracing::error!("Failed to spawn compile command: {:?}", s);
            }
            RunOutput::CompileFail(simple_output) => {
                let sql = state.db.read().await;
                repositories::submissions::create_submission_history(
                    &sql.db,
                    NewSubmissionHistory {
                        submitter: &user.username,
                        compile_fail: true,
                        code: solution,
                        question_index: problem_index,
                        score: 0.,
                        success: false,
                    },
                )
                .await
                .context("creating submission history")?;
                debug!(?simple_output, "Failed to build");
            }
            RunOutput::RunSuccess(vec) => {
                let sql = state.db.read().await;
                let other_completions =
                    repositories::submissions::count_other_submissions(&sql.db, problem_index)
                        .await
                        .context("getting other submissions")?;
                let mut txn = sql.db.begin().await.unwrap();
                let success = vec.iter().all(|x| matches!(x, TestOutput::Pass));
                let score = if success {
                    state
                        .config
                        .score(
                            problem_index,
                            bedrock::scoring::EvaluationContext {
                                num_completions: other_completions,
                                num_attempts: attempts,
                            },
                        )
                        .context("calculating score")?
                } else {
                    0.
                };
                let history = repositories::submissions::create_submission_history(
                    txn.acquire().await.unwrap(),
                    NewSubmissionHistory {
                        submitter: &user.username,
                        compile_fail: false,
                        code: solution,
                        question_index: problem_index,
                        score,
                        success,
                    },
                )
                .await
                .context("creating submission history")?;

                for (i, test) in vec.iter().enumerate() {
                    repositories::submissions::create_submission_test_history(
                        txn.acquire().await.unwrap(),
                        &history.id,
                        match test {
                            TestOutput::Pass => NewSubmissionTestHistory {
                                test_index: i,
                                result: TestResult::Pass,
                                stdout: None,
                                stderr: None,
                                exit_status: 0,
                            },
                            TestOutput::Fail(TestFailReason::Timeout) => NewSubmissionTestHistory {
                                test_index: i,
                                result: TestResult::Timeout,
                                stdout: None,
                                stderr: None,
                                exit_status: 1,
                            },
                            TestOutput::Fail(TestFailReason::IncorrectOutput(output)) => {
                                NewSubmissionTestHistory {
                                    test_index: i,
                                    result: TestResult::IncorrectOutput,
                                    stdout: output.stdout.str().map(String::from),
                                    stderr: output.stderr.str().map(String::from),
                                    exit_status: output.status.into(),
                                }
                            }
                            TestOutput::Fail(TestFailReason::Crash(output)) => {
                                NewSubmissionTestHistory {
                                    test_index: i,
                                    result: TestResult::Crash,
                                    stdout: output.stdout.str().map(String::from),
                                    stderr: output.stderr.str().map(String::from),
                                    exit_status: output.status.into(),
                                }
                            }
                        },
                    )
                    .await
                    .context("creating submission test history")?;
                }

                trace!(?vec, "Raw test output");
                let success = vec
                    .iter()
                    .filter(|&r| matches!(r, TestOutput::Pass))
                    .count();

                let results = vec
                    .into_iter()
                    .zip(problem.tests.iter())
                    .filter(|(_, t)| t.visible)
                    .map(|(r, t)| (r, t.clone()))
                    .collect::<Vec<_>>();

                let percent = success * 100 / problem.tests.len();
                txn.commit().await.context("committing transaction")?;
                ws.send(WebSocketSend::TestResults {
                    id,
                    results,
                    percent,
                })
                .context("sending test results message")?;

                let submissions =
                    repositories::submissions::get_latest_submissions(&sql.db, &user.username)
                        .await
                        .context("getting user submissions")?;

                let mut new_states =
                    vec![QuestionState::NotAttempted; state.config.packet.problems.len()];
                for s in submissions {
                    new_states[s.question_index as usize] = if s.success {
                        QuestionState::Pass
                    } else {
                        QuestionState::Fail
                    }
                }

                let new_score = repositories::submissions::get_user_score(&sql.db, &user.username)
                    .await
                    .context("getting user score")?;

                Arc::clone(&state).broadcast(WebSocketSend::TeamUpdate {
                    team: user.username.clone(),
                    new_score,
                    new_states,
                })?;
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip(state))]
    async fn handle(self, who: &ConnectionKind, state: Arc<AppState>) -> anyhow::Result<()> {
        {
            let ws = &state
                .active_connections
                .get(who)
                .context("websocket not in active_connections")?
                .send;

            if !self.can_use(who) {
                return self.error(ws, "Must be signed in to run tests");
            }
        }

        match self {
            WebSocketRecv::Broadcast { broadcast } => {
                state.broadcast(WebSocketSend::Broadcast { broadcast })?
            }
            WebSocketRecv::RunTest {
                id,
                ref language,
                ref solution,
                problem,
            } => {
                self.run_test(id, language, solution, problem, state, who)
                    .await?;
            }
            WebSocketRecv::Submit {
                id,
                ref language,
                ref solution,
                problem,
            } => {
                self.run_submission(id, language, solution, problem, state, who)
                    .await?;
            }
        }
        Ok(())
    }
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(connect::connect_websocket))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
