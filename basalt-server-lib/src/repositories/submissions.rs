use std::fmt::Display;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::{Executor, Sqlite, SqliteExecutor};
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
    pub score: f64,
    pub success: bool,
}

pub struct NewSubmissionHistory<'a> {
    pub submitter: &'a Username,
    pub compile_fail: bool,
    pub code: &'a str,
    pub question_index: usize,
    pub score: f64,
    pub success: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
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
    new: NewSubmissionHistory<'a>,
) -> anyhow::Result<SubmissionHistory> {
    let id = SubmissionId::new();
    let question_index = new.question_index as i64;
    sqlx::query_as!(SubmissionHistory,
            "INSERT INTO submission_history (id, submitter, compile_fail, code, question_index, score, success) VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id, submitter, time, compile_fail, code, question_index, score, success",
            id,
            new.submitter,
            new.compile_fail,
            new.code,
            question_index,
            new.score,
            new.success,
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

pub async fn count_other_submissions<'a>(
    db: impl Executor<'_, Database = Sqlite>,
    question_index: usize,
) -> anyhow::Result<u32> {
    let question_index = question_index as i64;
    let attempts = sqlx::query_scalar!(
        "SELECT COUNT(submitter) FROM submission_history WHERE question_index = ? AND success = TRUE AND time < CURRENT_TIMESTAMP",
        question_index
    )
    .fetch_one(db)
    .await
    .context("Failed to create submission test history")?;

    Ok(attempts as _)
}

pub async fn count_previous_submissions<'a>(
    db: impl Executor<'_, Database = Sqlite>,
    submitter: &Username,
    question_index: usize,
) -> anyhow::Result<u32> {
    let question_index = question_index as i64;
    let attempts = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM submission_history WHERE submitter = ? AND question_index = ?",
        submitter,
        question_index
    )
    .fetch_one(db)
    .await
    .context("Failed to create submission test history")?;

    Ok(attempts as _)
}

pub async fn get_user_score(
    db: impl SqliteExecutor<'_>,
    username: &Username,
) -> anyhow::Result<f64> {
    sqlx::query_scalar!(
        r#"
            SELECT SUM(h.score)
            FROM submission_history h
            JOIN (
                SELECT question_index, MAX(time) AS latest
                FROM submission_history
                GROUP BY question_index
            ) t ON h.question_index = t.question_index AND h.time = t.latest
            WHERE h.submitter = ?;
        "#,
        username
    )
    .fetch_one(db)
    .await
    .context("while querying the user's score")
    .map(Option::unwrap_or_default)
}

pub async fn get_latest_submissions(
    db: impl SqliteExecutor<'_>,
    username: &Username,
) -> anyhow::Result<Vec<SubmissionHistory>> {
    sqlx::query_as!(
        SubmissionHistory,
        r#"
            SELECT h.*
            FROM submission_history h
            JOIN (
                SELECT question_index, MAX(time) AS latest
                FROM submission_history
                GROUP BY question_index
            ) t ON h.question_index = t.question_index AND h.time = t.latest
            WHERE h.submitter = ?;
        "#,
        username
    )
    .fetch_all(db)
    .await
    .context("while querying the user's question states")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        repositories::users::Role,
        testing::{mock_db, users_repositories::dummy_user},
    };

    use super::*;

    #[tokio::test]
    async fn create_submission() {
        let (f, sql_layer) = mock_db().await;
        let sql = sql_layer.read().await;
        let user = dummy_user(&sql.db, "dummy_user", "foobar", Role::Competitor).await;
        let history = create_submission_history(
            &sql.db,
            NewSubmissionHistory {
                submitter: &user.username,
                compile_fail: true,
                code: "this is some code",
                question_index: 42,
                score: 42.,
                success: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(history.submitter, user.username);
        assert!(history.compile_fail);
        assert_eq!(history.code.as_str(), "this is some code");
        assert_eq!(history.question_index, 42);
        assert_eq!(history.score, 42.);
        assert!(!history.success);
        drop(f)
    }

    #[tokio::test]
    async fn create_submission_test() {
        let (f, sql_layer) = mock_db().await;
        let sql = sql_layer.read().await;
        let user = dummy_user(&sql.db, "dummy_user", "foobar", Role::Competitor).await;
        let history = create_submission_history(
            &sql.db,
            NewSubmissionHistory {
                submitter: &user.username,
                compile_fail: true,
                code: "this is some code",
                question_index: 42,
                score: 42.,
                success: false,
            },
        )
        .await
        .unwrap();

        let test = create_submission_test_history(
            &sql.db,
            &history.id,
            NewSubmissionTestHistory {
                test_index: 42,
                result: TestResult::Timeout,
                stdout: Some("stdout".into()),
                stderr: Some("stderr".into()),
                exit_status: 1,
            },
        )
        .await
        .unwrap();

        assert_eq!(test.test_index, 42);
        assert_eq!(test.result, TestResult::Timeout);
        assert_eq!(test.stdout, Some("stdout".into()));
        assert_eq!(test.stderr, Some("stderr".into()));
        assert_eq!(test.exit_status, 1);
        drop(f)
    }

    #[tokio::test]
    async fn other_submissions() {
        let (f, sql_layer) = mock_db().await;
        let sql = sql_layer.read().await;

        for i in 0..5 {
            let user = dummy_user(
                &sql.db,
                &format!("submitter-{}", i),
                "foobar",
                Role::Competitor,
            )
            .await;
            let history = create_submission_history(
                &sql.db,
                NewSubmissionHistory {
                    submitter: &user.username,
                    compile_fail: false,
                    code: "",
                    question_index: 1,
                    score: 10.,
                    success: true,
                },
            )
            .await
            .unwrap();

            for i in 0..5 {
                create_submission_test_history(
                    &sql.db,
                    &history.id,
                    NewSubmissionTestHistory {
                        test_index: i,
                        result: TestResult::Pass,
                        stdout: None,
                        stderr: None,
                        exit_status: 0,
                    },
                )
                .await
                .unwrap();
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        let n = count_other_submissions(&sql.db, 1).await.unwrap();
        assert_eq!(n, 5);

        drop(f)
    }

    #[tokio::test]
    async fn previous_submissions() {
        let (f, sql_layer) = mock_db().await;
        let sql = sql_layer.read().await;

        let user = dummy_user(&sql.db, "dummy_user", "foobar", Role::Competitor).await;
        for _ in 0..5 {
            create_submission_history(
                &sql.db,
                NewSubmissionHistory {
                    submitter: &user.username,
                    compile_fail: true,
                    code: "",
                    question_index: 1,
                    score: 10.,
                    success: false,
                },
            )
            .await
            .unwrap();
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        let n = count_previous_submissions(&sql.db, &user.username, 1)
            .await
            .unwrap();
        assert_eq!(n, 5);

        drop(f)
    }

    #[tokio::test]
    async fn user_score() {
        let (f, sql_layer) = mock_db().await;
        let sql = sql_layer.read().await;

        let user = dummy_user(&sql.db, "dummy_user", "foobar", Role::Competitor).await;
        for i in 0..5 {
            create_submission_history(
                &sql.db,
                NewSubmissionHistory {
                    submitter: &user.username,
                    compile_fail: false,
                    code: "",
                    question_index: i,
                    score: 42.,
                    success: true,
                },
            )
            .await
            .unwrap();
        }

        let n = get_user_score(&sql.db, &user.username).await.unwrap();
        assert_eq!(n, 42. * 5.);

        drop(f)
    }

    #[tokio::test]
    async fn latest_submissions() {
        let (f, sql_layer) = mock_db().await;
        let sql = sql_layer.read().await;

        let user = dummy_user(&sql.db, "dummy_user", "foobar", Role::Competitor).await;
        for i in 0..5 {
            create_submission_history(
                &sql.db,
                NewSubmissionHistory {
                    submitter: &user.username,
                    compile_fail: false,
                    code: "not-latest",
                    question_index: i,
                    score: 42.,
                    success: true,
                },
            )
            .await
            .unwrap();
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        for i in 0..5 {
            create_submission_history(
                &sql.db,
                NewSubmissionHistory {
                    submitter: &user.username,
                    compile_fail: false,
                    code: "latest",
                    question_index: i,
                    score: 42.,
                    success: true,
                },
            )
            .await
            .unwrap();
        }

        let submissions = get_latest_submissions(&sql.db, &user.username)
            .await
            .unwrap();

        for s in submissions {
            assert_eq!(s.code, "latest");
        }

        drop(f)
    }
}
