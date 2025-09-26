use erudite::runner::TestResult;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    repositories::{
        announcements::{Announcement, AnnouncementId},
        submissions::{SubmissionId, TestResultState, TestResults as DbTestResults},
        users::{QuestionState, UserId},
    },
    server::{teams::TeamWithScore, tester::TestData, websocket::ConnectionKind, AppState},
};

pub mod connect;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub struct TeamUpdate {
    pub id: UserId,
    pub name: String,
    pub display_name: Option<String>,
    pub new_score: f64,
    pub new_states: Vec<QuestionState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Broadcast {
    NewAnnouncement(Announcement),
    DeleteAnnouncement {
        id: AnnouncementId,
    },
    Announce {
        message: String,
    },
    GamePaused,
    TeamConnected(TeamWithScore),
    TeamDisconnected(TeamWithScore),
    GameUnpaused {
        time_left_in_seconds: u64,
    },
    TeamRename {
        id: UserId,
        name: String,
        display_name: Option<String>,
    },
    TeamUpdate {
        teams: Vec<TeamUpdate>,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResultSend {
    index: usize,
    state: TestResultState,
    stdout: String,
    stderr: String,
    exit_status: i32,
    // milliseconds
    time_taken: u64,
}

impl From<DbTestResults> for TestResultSend {
    fn from(value: DbTestResults) -> Self {
        Self {
            index: value.test_index as _,
            state: value.result,
            stdout: value.stdout,
            stderr: value.stderr,
            exit_status: value.exit_status as _,
            time_taken: value.time_taken.as_millis() as u64,
        }
    }
}

impl From<&TestResult<TestData>> for TestResultSend {
    fn from(value: &TestResult<TestData>) -> Self {
        Self {
            index: value.index(),
            state: value.state().into(),
            stdout: value.stdout().to_str_lossy().into_owned(),
            stderr: value.stderr().to_str_lossy().into_owned(),
            exit_status: value.exit_status(),
            time_taken: value.time_taken().as_millis() as u64,
        }
    }
}

/// A message that is sent from the server onto the websocket
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WebSocketSend {
    Broadcast {
        broadcast: Broadcast,
    },
    Error {
        id: Option<usize>,
        message: String,
    },
    /// An error occurred while running tests
    TestsError {
        id: SubmissionId,
    },
    /// Running tests were cancelled
    TestsCancelled {
        id: SubmissionId,
    },
    /// All tests have finished running
    TestsComplete {
        id: SubmissionId,
        results: Vec<TestResultSend>,
    },
    /// One of more tests has finished
    TestResults {
        id: SubmissionId,
        results: Vec<TestResultSend>,
    },
}

/// A message that is recieved from the websocket
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WebSocketRecv {}

impl WebSocketRecv {
    async fn handle(self, _who: &ConnectionKind, _state: Arc<AppState>) -> anyhow::Result<()> {
        Ok(())
    }
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(connect::connect_websocket))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
