use std::{borrow::Cow, net::SocketAddr, sync::Arc};

use anyhow::{bail, Context};
use bedrock::packet::Test;
use erudite::{RunOutput, TestCase, TestOutput};
use leucite::Rules;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, trace};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::server::AppState;
pub mod connect;

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub enum ConnectionKind {
    User { username: String },
    Leaderboard { addr: SocketAddr },
}

#[derive(Debug, Clone)]
pub struct ConnectedClient {
    pub send: mpsc::UnboundedSender<WebSocketSend>,
}

/// A message that is recieved from the websocket
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WebSocketRecv<'a> {
    RunTest {
        id: usize,
        language: Cow<'a, str>,
        solution: Cow<'a, str>,
        problem: usize,
    },
}

/// A message that is sent from the server onto the websocket
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WebSocketSend {
    Broadcast {
        message: String,
    },
    TestResults {
        id: usize,
        results: Vec<(TestOutput, Test)>,
        percent: usize,
    },
}

impl WebSocketRecv<'_> {
    #[tracing::instrument(skip(state))]
    async fn handle(self, who: &ConnectionKind, state: &Arc<AppState>) -> anyhow::Result<()> {
        match self {
            WebSocketRecv::RunTest {
                id,
                language,
                solution,
                problem,
            } => {
                let Some(language) = state.config.languages.get_by_str(&language) else {
                    // TODO: how to handle an known language?
                    debug!(language = &*language, "Unknown language");
                    return Ok(());
                };

                let build_rules = Rules::new()
                    .add_read_only("/usr")
                    .add_read_only("/etc")
                    .add_read_only("/dev")
                    .add_read_only("/bin");
                let run_rules = Rules::new()
                    .add_read_only("/usr")
                    .add_read_only("/etc")
                    .add_read_only("/dev")
                    .add_read_only("/bin");

                let mut runner = erudite::Runner::new();
                let problem = &*state.config.packet.problems[problem];
                runner
                    .create_file(language.source_file(), &*solution)
                    .tests(
                        problem
                            .tests
                            .iter()
                            .map(|t| TestCase::new(&t.input, &t.output)),
                    )
                    .timeout(state.config.test_runner.timeout)
                    .trim_output(state.config.test_runner.trim_output)
                    .compile_rules(build_rules)
                    .run_rules(run_rules)
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
                        state
                            .active_connections
                            .get(who)
                            .expect("added before call to this function")
                            .send
                            .send(WebSocketSend::TestResults {
                                id,
                                results,
                                percent,
                            })
                            .context("sending broadcast message")?;
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn ws_router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(connect::handler))
}

pub fn ws_service() -> axum::Router<Arc<AppState>> {
    ws_router().split_for_parts().0
}
