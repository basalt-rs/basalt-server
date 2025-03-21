use std::fmt::Display;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::{Executor, Sqlite};
use time::OffsetDateTime;

use super::users::Username;

#[derive(Serialize, Deserialize, derive_more::From, derive_more::Into, sqlx::Type)]
#[sqlx(transparent)]
pub struct SubmissionId(String);

impl SubmissionId {
    fn new() -> Self {
        use rand::{distributions::Alphanumeric, Rng};
        let id = rand::thread_rng()
            .sample_iter(Alphanumeric)
            .take(20)
            .map(char::from)
            .collect::<String>();
        Self(id)
    }
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct SubmissionHistory {
    pub id: SubmissionId,
    pub submitter: Username,
    #[serde(with = "time::serde::rfc3339")]
    pub time: OffsetDateTime,
    pub compile_fail: bool,
    pub code: String,
    pub question_index: i64, // _really_ should be usize, but sqlx doesn't like that
    pub score: i64,
}

#[derive(Serialize, Deserialize)]
pub enum TestResult {
    Pass,
    Timeout,
    IncorrectOutput,
    Crash,
}

impl Display for TestResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestResult::Pass => write!(f, "pass"),
            TestResult::Timeout => write!(f, "timeout"),
            TestResult::IncorrectOutput => write!(f, "incorrect_output"),
            TestResult::Crash => write!(f, "crash"),
        }
    }
}

impl From<String> for TestResult {
    fn from(value: String) -> Self {
        match value.as_str() {
            "pass" => Self::Pass,
            "timeout" => Self::Timeout,
            "incorrect_output" => Self::IncorrectOutput,
            "crash" => Self::Crash,
            _ => unreachable!(),
        }
    }
}

/// History of tests that have been run on submissions
#[derive(Serialize, Deserialize)]
pub struct SubmissionTestHistory {
    pub submission: SubmissionId,
    pub test_index: i64, // _really_ should be usize, but sqlx doesn't like that
    pub result: TestResult,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_status: i64,
}

/// History of tests that have been run on submissions
#[derive(Serialize, Deserialize)]
pub struct NewSubmissionTestHistory {
    pub test_index: usize,
    pub result: TestResult,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_status: i64,
}

pub async fn create_submission_history<'a>(
    db: impl Executor<'_, Database = Sqlite>,
    submitter: &Username,
    compile_fail: bool,
    code: impl AsRef<str>,
    question_index: usize,
    score: u32,
) -> anyhow::Result<SubmissionHistory> {
    let code = code.as_ref();
    let id = SubmissionId::new();
    let question_index = question_index as i64;
    sqlx::query_as!(SubmissionHistory,
            "INSERT INTO submission_history (id, submitter, compile_fail, code, question_index, score) VALUES (?, ?, ?, ?, ?, ?) RETURNING id, submitter, time, compile_fail, code, question_index, score",
            id,
            submitter,
            compile_fail,
            code,
            question_index,
            score
        )
        .fetch_one(db)
        .await
        .context("Failed to create submission history")
}

pub async fn create_submission_test_history<'a>(
    db: impl Executor<'_, Database = Sqlite>,
    submission: &SubmissionId,
    new: NewSubmissionTestHistory,
) -> anyhow::Result<SubmissionTestHistory> {
    let test_index = new.test_index as i64;
    let result = new.result.to_string();
    sqlx::query_as!(SubmissionTestHistory,
            "INSERT INTO submission_test_history (submission, test_index, result, stdout, stderr, exit_status) VALUES (?, ?, ?, ?, ?, ?) RETURNING submission, test_index, result, stdout, stderr, exit_status",
            submission,
            test_index,
            result,
            new.stdout,
            new.stderr,
            new.exit_status,
        )
        .fetch_one(db)
        .await
        .context("Failed to create submission test history")
}

pub async fn count_previous_submissions<'a>(
    db: impl Executor<'_, Database = Sqlite>,
    submitter: &Username,
    question_index: usize,
) -> anyhow::Result<usize> {
    let question_index = question_index as i64;
    let count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM submission_history WHERE submitter = ? AND question_index = ?",
        submitter,
        question_index
    )
    .fetch_one(db)
    .await
    .context("Failed to create submission test history")?;
    Ok(count as usize)
}
