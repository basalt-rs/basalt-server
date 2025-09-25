use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use bedrock::{packet::Test, scoring::Scorable, Config};
use dashmap::DashMap;
use erudite::{
    error::CompileError,
    runner::{TestResult, TestResultState, TestRunner},
    BorrowedFileContent, Rules, TestContext,
};
use tokio::sync::oneshot;
use tracing::{debug, error};

use crate::{
    repositories::{self, submissions::SubmissionId, users::UserId},
    server::AppState,
    services::ws::WebSocketSend,
};

#[derive(Debug, Copy, Clone, Default)]
pub struct TestData {
    pub visible: bool,
}

impl From<&Test> for TestData {
    fn from(value: &Test) -> Self {
        Self {
            visible: value.visible,
        }
    }
}

pub struct ContextExtended {
    context: Arc<TestContext<usize, TestData>>,
    dest: PathBuf,
}

pub struct Tester {
    // language raw name : test context
    contexts: HashMap<String, ContextExtended>,
    abort_handles: DashMap<SubmissionId, oneshot::Sender<()>>,
}

impl Tester {
    pub fn new(config: &Config) -> Self {
        let start = Instant::now();
        let contexts: HashMap<_, _> = config
            .languages
            .iter()
            .map(|l| {
                let compile_rules = Rules::new()
                    .add_read_only("/tmp")
                    .add_read_only("/usr")
                    .add_read_only("/etc")
                    .add_read_only("/dev")
                    .add_read_only("/bin");
                let run_rules = Rules::new()
                    .add_read_only("/usr")
                    .add_read_only("/etc")
                    .add_read_only("/dev")
                    .add_read_only("/bin");

                let groups = config
                    .packet
                    .problems
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| {
                        p.languages
                            .as_ref()
                            .is_none_or(|pl| pl.contains(l.raw_name()))
                    })
                    .map(move |(i, p)| {
                        (
                            i,
                            p.tests
                                .iter()
                                .map(|t| (&t.input, &*t.output, TestData::from(t))),
                        )
                    });

                let mut c = TestContext::builder()
                    .run_command(["bash", "-c", l.run_command()])
                    .run_timeout(config.test_runner.timeout)
                    .trim_output(config.test_runner.trim_output)
                    .test_groups(groups)
                    .compile_rules(compile_rules)
                    .run_rules(run_rules);

                if let Some(compile_command) = l.build_command() {
                    c = c.compile_command(["bash", "-c", compile_command])
                };

                (
                    l.raw_name().to_string(),
                    ContextExtended {
                        context: Arc::new(c.build()),
                        dest: PathBuf::from(l.source_file()),
                    },
                )
            })
            .collect();
        debug!(in = ?start.elapsed(), count = contexts.len(), "Generated all test contexts");

        Self {
            contexts,
            abort_handles: Default::default(),
        }
    }

    pub fn add_abort_handle(&self, test: SubmissionId, handle: oneshot::Sender<()>) {
        self.abort_handles.insert(test, handle);
    }

    /// Returns whether the test existed and was successfully aborted
    pub fn abort(&self, test: SubmissionId) -> bool {
        self.abort_handles
            .remove(&test)
            .is_some_and(|(_, x)| x.send(()).is_ok())
    }

    pub fn runner(
        &self,
        language: &str,
        problem_index: usize,
    ) -> Option<(TestRunner<'_, usize, TestData>, &Path)> {
        let ctx = self.contexts.get(language)?;
        Some((
            Arc::clone(&ctx.context).test_runner(&problem_index)?,
            &ctx.dest,
        ))
    }
}

enum TestWsSend {
    Error,
    Complete,
    Cancelled,
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
                TestWsSend::Cancelled => Some(WebSocketSend::TestsCancelled { id }),
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
                    TestWsSend::Cancelled => {
                        send = Some(WebSocketSend::TestsCancelled { id });
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

pub fn run_test(
    state: Arc<AppState>,
    language: String,
    question_index: usize,
    code: String,
    test_only: bool,
    submitter: UserId,
) -> Option<SubmissionId> {
    // NOTE: It's not great that we construct a test runner and then throw it await, but we can't
    // move the test runner into the new task, so it's fine (constructing one is really cheap).
    let runner = state.tester.runner(&language, question_index);
    if runner.is_none() {
        // This should be prevented by the UI
        error!(
            language,
            question_index, "Missing runner for attempted test"
        );
        return None;
    };

    let (abort_tx, abort_rx) = oneshot::channel();
    let id = SubmissionId::new();
    state.tester.add_abort_handle(id, abort_tx);
    tokio::spawn(async move {
        let language = language.as_ref();
        let code = code.as_ref();
        let (runner, source_file) = state
            .tester
            .runner(language, question_index)
            .expect("checked above");

        let compiled = runner
            .file(BorrowedFileContent::string(code), source_file)
            .filter_tests(if test_only {
                |t| t.data().visible
            } else {
                |_| true
            })
            .compile()
            .await;

        let result_tx = spawn_ws_sender(Arc::clone(&state), id, submitter);

        // NOTE: This exists since the `?` operator calls `.into()` and `()` implements `From` for
        // any `T`, but we want all errors to be handled, not just disappeared.
        struct Unit;
        impl From<()> for Unit {
            fn from((): ()) -> Self {
                Self
            }
        }

        let result = async {
            let other_completions =
                repositories::submissions::count_other_submissions(&state.db, question_index)
                    .await
                    .map_err(|error| error!(?error, "Error counting other submissions"))?;

            let previous_attempts = repositories::submissions::count_previous_submissions(
                &state.db,
                &submitter,
                question_index,
            )
            .await
            .map_err(|error| error!(?error, "Error counting previous submissions"))?;

            let compiled = match compiled {
                Err(CompileError::CompileFail(compile_result)) => {
                    repositories::submissions::create_failed_submission_history(
                        &state.db,
                        repositories::submissions::NewSubmissionHistory {
                            id,
                            submitter,
                            code,
                            question_index,
                            language,
                            compile_result: Some(&compile_result),
                            test_only,
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
                    submitter,
                    code,
                    question_index,
                    language,
                    compile_result: compiled.compile_result(),
                    test_only,
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
                question_index,
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
        };

        tokio::select! {
            result = result => {
                match result {
                    Ok(_) => {
                        let _ = result_tx.send(TestWsSend::Complete);
                    }
                    Err(_) => {
                        let _ = result_tx.send(TestWsSend::Error);
                    }
                }
            }
            _ = abort_rx => {
                let _ = result_tx.send(TestWsSend::Cancelled);
            }
        };

        // ensure that we don't try to use the sender after we've sent Complete or Error
        drop(result_tx);
    });

    Some(id)
}
