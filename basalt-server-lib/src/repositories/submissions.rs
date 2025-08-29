use anyhow::Context;
use derive_more::Deref;
use erudite::runner::{CompileResult, TestResult};
use serde::{Deserialize, Serialize};
use sqlx::{Executor, Sqlite, SqliteExecutor};
use std::{borrow::Cow, time::Duration};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{define_id_type, repositories::util::WrappedDuration};

use super::users::UserId;

define_id_type!(SubmissionId);

macro_rules! impl_erudite_enum {
    (
        $(#[$($attr: tt)+])*
        pub enum $name: ident($erudite_ty: ty) {
            $variant0: ident = $pat0: pat,
            $($variant: ident = $pat: pat),+$(,)?
        }
    ) => {
        $(#[$($attr)+])*
        #[derive(Clone, Copy, PartialEq, Eq)]
        #[repr(i64)]
        pub enum $name {
            $variant0 = 0,
            $($variant),+
        }

        impl From<$name> for i64 {
            fn from(value: $name) -> Self {
                value as _
            }
        }

        impl From<i64> for $name {
            fn from(value: i64) -> Self {
                assert!(value >= 0);
                [Self::$variant0, $(Self::$variant),+][value as usize]
            }
        }

        impl From<$erudite_ty> for $name {
            fn from(value: $erudite_ty) -> Self {
                match value {
                    $pat0 => Self::$variant0,
                    $($pat => Self::$variant),+
                }
            }
        }

    };
    (
        $(#[$($attr: tt)+])*
        pub enum $name: ident {
            $variant0: ident,
            $($variant: ident),+$(,)?
        }
    ) => {
        $(#[$($attr)+])*
        #[derive(Clone, Copy, PartialEq, Eq)]
        #[repr(i64)]
        pub enum $name {
            $variant0 = 0,
            $($variant),+
        }

        impl From<$name> for i64 {
            fn from(value: $name) -> Self {
                value as _
            }
        }

        impl From<i64> for $name {
            fn from(value: i64) -> Self {
                assert!(value >= 0);
                [Self::$variant0, $(Self::$variant),+][value as usize]
            }
        }

    }
}

impl_erudite_enum! {
    #[derive(Debug, Serialize, Deserialize, sqlx::Type, ToSchema)]
    pub enum CompileResultState(Option<erudite::runner::CompileResultState>) {
        NoCompile = None,
        Success = Some(erudite::runner::CompileResultState::Success),
        RuntimeFail = Some(erudite::runner::CompileResultState::RuntimeFail),
        TimedOut = Some(erudite::runner::CompileResultState::TimedOut)
    }
}

impl_erudite_enum! {
    #[derive(Debug, Hash, Serialize, Deserialize, utoipa::ToSchema, sqlx::Type)]
    pub enum SubmissionState {
        Started,
        Finished,
        Cancelled,
        Failed,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct SubmissionHistory {
    pub id: SubmissionId,
    pub submitter: UserId,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = Date)]
    pub time: OffsetDateTime,
    pub code: String,
    pub question_index: i64,
    pub language: String,
    pub compile_result: CompileResultState,
    pub compile_stdout: String,
    pub compile_stderr: String,
    pub compile_exit_status: i64,
    // The remaining data will be updated after the tests have finished running
    pub state: SubmissionState,
    pub score: f64,
    pub success: bool, // effectively tests.all(state = Pass)
    // NOTE: This is stored as a `u64` cast as an `i64`.  Keep that in mind while doing operations on this data in queries.
    pub time_taken: WrappedDuration,
}

pub struct NewSubmissionHistory<'a> {
    pub submitter: &'a UserId,
    pub code: &'a str,
    pub question_index: usize,
    pub language: &'a str,
    pub compile_result: Option<&'a CompileResult>,
}

impl_erudite_enum! {
    #[derive(Debug, Serialize, Deserialize, sqlx::Type, ToSchema)]
    pub enum TestResultState(erudite::runner::TestResultState) {
        Pass = erudite::runner::TestResultState::Pass,
        RuntimeFail = erudite::runner::TestResultState::RuntimeFail,
        TimedOut = erudite::runner::TestResultState::TimedOut,
        IncorrectOutput = erudite::runner::TestResultState::IncorrectOutput
    }
}

/// History of tests that have been run on submissions
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SubmissionTestHistory {
    pub submission: SubmissionId,
    pub test_index: i64, // _really_ should be usize, but sqlx doesn't like that
    pub result: TestResultState,
    pub stdout: String,
    pub stderr: String,
    pub exit_status: i64,
    // NOTE: This is stored as a `u64` cast as an `i64`.  Keep that in mind while doing operations on this data in queries.
    pub time_taken: WrappedDuration,
}

/// History of tests that have been run on submissions
pub struct NewSubmissionTestHistory<'a> {
    result: TestResultState,
    stdout: Cow<'a, str>,
    stderr: Cow<'a, str>,
    exit_status: i32,
    time_taken: WrappedDuration,
}

impl<'a, T> From<&'a TestResult<T>> for NewSubmissionTestHistory<'a> {
    fn from(value: &'a TestResult<T>) -> Self {
        Self {
            result: value.state().into(),
            stdout: value.stdout().to_str_lossy(),
            stderr: value.stderr().to_str_lossy(),
            exit_status: value.exit_status(),
            time_taken: value.time_taken().into(),
        }
    }
}

/// A submission that has been added to the database, but hasn't been finished (call `.finish` to
/// finish)
#[derive(Clone, Debug, Deref)]
#[must_use]
pub struct PartialSubmissionHistory(SubmissionHistory);

impl PartialSubmissionHistory {
    pub async fn fail(
        self,
        db: impl Executor<'_, Database = Sqlite>,
    ) -> anyhow::Result<SubmissionHistory> {
        sqlx::query_as!(SubmissionHistory, r#"
            UPDATE submission_history
                SET state = ?
            WHERE id = ?
            RETURNING id, submitter, time, code, question_index, language, compile_result, compile_stdout, compile_stderr, compile_exit_status, state, score, success, time_taken"#,
            SubmissionState::Failed,
            self.id,
        )
        .fetch_one(db)
        .await
        .context("Failed to create submission history")
    }

    pub async fn cancel(
        self,
        db: impl Executor<'_, Database = Sqlite>,
    ) -> anyhow::Result<SubmissionHistory> {
        sqlx::query_as!(SubmissionHistory, r#"
            UPDATE submission_history
                SET state = ?
            WHERE id = ?
            RETURNING id, submitter, time, code, question_index, language, compile_result, compile_stdout, compile_stderr, compile_exit_status, state, score, success, time_taken"#,
            SubmissionState::Cancelled,
            self.id,
        )
        .fetch_one(db)
        .await
        .context("Failed to create submission history")
    }

    pub async fn finish(
        self,
        db: impl Executor<'_, Database = Sqlite>,
        score: f64,
        success: bool,
        time_taken: Duration,
    ) -> anyhow::Result<SubmissionHistory> {
        let time_taken = WrappedDuration::from(time_taken);
        sqlx::query_as!(SubmissionHistory, r#"
            UPDATE submission_history
                SET state = ?,
                score = ?,
                success = ?,
                time_taken = ?
            WHERE id = ?
            RETURNING id, submitter, time, code, question_index, language, compile_result, compile_stdout, compile_stderr, compile_exit_status, state, score, success, time_taken"#,
            SubmissionState::Finished,
            score,
            success,
            time_taken,
            self.id,
        )
        .fetch_one(db)
        .await
        .context("Failed to create submission history")
    }
}

pub async fn create_failed_submission_history<'a>(
    db: impl Executor<'_, Database = Sqlite>,
    new: NewSubmissionHistory<'a>,
) -> anyhow::Result<PartialSubmissionHistory> {
    let id = SubmissionId::new();
    let question_index = new.question_index as i64;
    let compile_result: CompileResultState = new.compile_result.map(|r| r.state()).into();
    let compile_stdout = new
        .compile_result
        .map(|r| r.stdout().to_str_lossy())
        .unwrap_or(Cow::Borrowed(""));
    let compile_stderr = new
        .compile_result
        .map(|r| r.stderr().to_str_lossy())
        .unwrap_or(Cow::Borrowed(""));
    let compile_exit_status = new
        .compile_result
        .map(|r| r.exit_status())
        .unwrap_or_default() as i64;

    let hist = sqlx::query_as!(SubmissionHistory, r#"
            INSERT INTO submission_history (id, submitter, code, question_index, language, compile_result, compile_stdout, compile_stderr, compile_exit_status, state)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id, submitter, time, code, question_index, language, compile_result, compile_stdout, compile_stderr, compile_exit_status, state, score, success, time_taken"#,
            id,
            new.submitter,
            new.code,
            question_index,
            new.language,
            compile_result, compile_stdout, compile_stderr, compile_exit_status,
            SubmissionState::Failed,
        )
        .fetch_one(db)
        .await
        .context("Failed to create submission history")?;

    Ok(PartialSubmissionHistory(hist))
}

pub async fn create_submission_history<'a>(
    db: impl Executor<'_, Database = Sqlite>,
    new: NewSubmissionHistory<'a>,
) -> anyhow::Result<PartialSubmissionHistory> {
    let id = SubmissionId::new();
    let question_index = new.question_index as i64;
    let compile_result: CompileResultState = new.compile_result.map(|r| r.state()).into();
    let compile_stdout = new
        .compile_result
        .map(|r| r.stdout().to_str_lossy())
        .unwrap_or(Cow::Borrowed(""));
    let compile_stderr = new
        .compile_result
        .map(|r| r.stderr().to_str_lossy())
        .unwrap_or(Cow::Borrowed(""));
    let compile_exit_status = new
        .compile_result
        .map(|r| r.exit_status())
        .unwrap_or_default() as i64;

    let hist = sqlx::query_as!(SubmissionHistory, r#"
            INSERT INTO submission_history (id, submitter, code, question_index, language, compile_result, compile_stdout, compile_stderr, compile_exit_status)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id, submitter, time, code, question_index, language, compile_result, compile_stdout, compile_stderr, compile_exit_status, state, score, success, time_taken"#,
            id,
            new.submitter,
            new.code,
            question_index,
            new.language,
            compile_result,
            compile_stdout, compile_stderr, compile_exit_status
        )
        .fetch_one(db)
        .await
        .context("Failed to create submission history")?;

    Ok(PartialSubmissionHistory(hist))
}

pub async fn create_submission_test_history<'a>(
    db: impl Executor<'_, Database = Sqlite>,
    submission: &SubmissionId,
    test_index: usize,
    new: NewSubmissionTestHistory<'a>,
) -> anyhow::Result<SubmissionTestHistory> {
    let test_index = test_index as i64;

    sqlx::query_as!(
        SubmissionTestHistory,
        r#"
            INSERT INTO test_results (submission, test_index, result, stdout, stderr, exit_status, time_taken)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            RETURNING submission, test_index, result, stdout, stderr, exit_status, time_taken"#,
        submission,
        test_index,
        new.result,
        new.stdout,
        new.stderr,
        new.exit_status,
        new.time_taken,
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
    submitter: &UserId,
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

pub async fn get_user_score(db: impl SqliteExecutor<'_>, user_id: &UserId) -> anyhow::Result<f64> {
    sqlx::query_scalar!(
        r#"
            SELECT SUM(h.score)
            FROM submission_history h
            JOIN (
                SELECT question_index, MAX(time) AS latest
                FROM submission_history
                WHERE submitter = ?
                GROUP BY question_index
            ) t ON h.question_index = t.question_index AND h.time = t.latest
            WHERE h.submitter = ?;
        "#,
        user_id,
        user_id,
    )
    .fetch_one(db)
    .await
    .context("while querying the user's score")
    .map(Option::unwrap_or_default)
}

pub async fn get_latest_submissions(
    db: impl SqliteExecutor<'_>,
    user_id: &UserId,
) -> anyhow::Result<Vec<SubmissionHistory>> {
    sqlx::query_as!(
        SubmissionHistory,
        r#"
            SELECT h.*
            FROM submission_history h
            JOIN (
                SELECT question_index, MAX(time) AS latest
                FROM submission_history
                WHERE submitter = ?
                GROUP BY question_index
            ) t ON h.question_index = t.question_index AND h.time = t.latest
            WHERE h.submitter = ?;
        "#,
        user_id,
        user_id,
    )
    .fetch_all(db)
    .await
    .context("while querying the user's question states")
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct Attempt {
    pub question_index: i64,
    pub attempts: i64,
}

pub async fn get_attempts(
    db: impl SqliteExecutor<'_>,
    user_id: &UserId,
) -> anyhow::Result<Vec<Attempt>> {
    sqlx::query_as!(
        Attempt,
        r#"
            SELECT question_index, count(id) as attempts
            FROM submission_history
            WHERE submitter = ?
            GROUP BY question_index;
        "#,
        user_id
    )
    .fetch_all(db)
    .await
    .context("while querying the user's score")
}

pub async fn add_test(
    db: impl SqliteExecutor<'_>,
    user_id: &UserId,
    question_index: usize,
) -> anyhow::Result<()> {
    let question_index = question_index as i64;
    let id = SubmissionId::new();
    sqlx::query!(
        r#"
        INSERT INTO test_runs (id, user_id, question_index)
        VALUES (?, ?, ?)
        "#,
        id,
        user_id,
        question_index,
    )
    .execute(db)
    .await
    .context("while adding the user's test")?;
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct TestCount {
    pub question_index: i64,
    pub count: i64,
}

pub async fn count_tests(
    db: impl SqliteExecutor<'_>,
    user_id: &UserId,
) -> anyhow::Result<Vec<TestCount>> {
    sqlx::query_as!(
        TestCount,
        r#"
            SELECT question_index, count(id) as count
            FROM test_runs
            WHERE user_id = ?
            GROUP BY question_index;
        "#,
        user_id,
    )
    .fetch_all(db)
    .await
    .context("while querying the user's test runs")
}

pub async fn get_submissions(
    db: impl SqliteExecutor<'_>,
    user_id: &UserId,
    question_index: usize,
) -> anyhow::Result<Vec<SubmissionHistory>> {
    let question_index = question_index as i64;

    sqlx::query_as!(
        SubmissionHistory,
        r#"
        SELECT * FROM submission_history
        WHERE submitter = ? AND question_index = ?
        ORDER BY time DESC;
        "#,
        user_id,
        question_index
    )
    .fetch_all(db)
    .await
    .context("getting user submissions")
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
        let (f, sql) = mock_db().await;
        let user = dummy_user(&sql, "dummy_user", "foobar", Role::Competitor).await;
        let history = create_submission_history(
            &sql,
            NewSubmissionHistory {
                submitter: &user.id,
                code: "this is some code",
                question_index: 42,
                language: "java",
                compile_result: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(history.submitter, user.id);
        assert_eq!(history.compile_result, CompileResultState::NoCompile);
        assert_eq!(history.code.as_str(), "this is some code");
        assert_eq!(history.question_index, 42);
        assert_eq!(history.score, 42.);
        assert!(!history.success);
        drop(f)
    }

    #[tokio::test]
    async fn create_submission_test() {
        let (f, sql) = mock_db().await;
        let user = dummy_user(&sql, "dummy_user", "foobar", Role::Competitor).await;
        let history = create_submission_history(
            &sql,
            NewSubmissionHistory {
                submitter: &user.id,
                code: "this is some code",
                question_index: 42,
                language: "java",
                compile_result: None,
            },
        )
        .await
        .unwrap();

        let test = create_submission_test_history(
            &sql,
            &history.id,
            42,
            NewSubmissionTestHistory {
                result: TestResultState::TimedOut,
                stdout: "stdout".into(),
                stderr: "stderr".into(),
                exit_status: 1,
                time_taken: Duration::from_secs(1).into(),
            },
        )
        .await
        .unwrap();

        assert_eq!(test.test_index, 42);
        assert_eq!(test.result, TestResultState::TimedOut);
        assert_eq!(test.stdout, "stdout");
        assert_eq!(test.stderr, "stderr");
        assert_eq!(test.exit_status, 1);
        drop(f)
    }

    #[tokio::test]
    async fn other_submissions() {
        let (f, sql) = mock_db().await;

        for i in 0..5 {
            let user = dummy_user(
                &sql,
                &format!("submitter-{}", i),
                "foobar",
                Role::Competitor,
            )
            .await;
            let history = create_submission_history(
                &sql,
                NewSubmissionHistory {
                    submitter: &user.id,
                    code: "",
                    question_index: 1,
                    language: "java",
                    compile_result: None,
                },
            )
            .await
            .unwrap();

            for i in 0..5 {
                create_submission_test_history(
                    &sql,
                    &history.id,
                    i,
                    NewSubmissionTestHistory {
                        result: TestResultState::Pass,
                        stdout: "".into(),
                        stderr: "".into(),
                        exit_status: 0,
                        time_taken: Duration::from_millis(1).into(),
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
        let (f, sql) = mock_db().await;

        let user = dummy_user(&sql, "dummy_user", "foobar", Role::Competitor).await;
        for _ in 0..5 {
            create_submission_history(
                &sql.db,
                NewSubmissionHistory {
                    submitter: &user.id,
                    code: "",
                    question_index: 1,
                    language: "java",
                    compile_result: None,
                },
            )
            .await
            .unwrap();
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        let n = count_previous_submissions(&sql.db, &user.id, 1)
            .await
            .unwrap();
        assert_eq!(n, 5);

        drop(f)
    }

    #[tokio::test]
    async fn user_score() {
        let (f, sql) = mock_db().await;

        let user = dummy_user(&sql, "dummy_user", "foobar", Role::Competitor).await;
        for i in 0..5 {
            create_submission_history(
                &sql,
                NewSubmissionHistory {
                    submitter: &user.id,
                    code: "",
                    question_index: i,
                    language: "java",
                    compile_result: None,
                },
            )
            .await
            .unwrap();
        }

        let n = get_user_score(&sql.db, &user.id).await.unwrap();
        assert_eq!(n, 42. * 5.);

        drop(f)
    }

    #[tokio::test]
    async fn latest_submissions() {
        let (f, sql) = mock_db().await;

        let user = dummy_user(&sql, "dummy_user", "foobar", Role::Competitor).await;
        for i in 0..5 {
            create_submission_history(
                &sql,
                NewSubmissionHistory {
                    submitter: &user.id,
                    code: "not-latest",
                    question_index: i,
                    language: "java",
                    compile_result: None,
                },
            )
            .await
            .unwrap();
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        for i in 0..5 {
            create_submission_history(
                &sql,
                NewSubmissionHistory {
                    submitter: &user.id,
                    code: "latest",
                    question_index: i,
                    language: "java",
                    compile_result: None,
                },
            )
            .await
            .unwrap();
        }

        let submissions = get_latest_submissions(&sql, &user.id).await.unwrap();

        for s in submissions {
            assert_eq!(s.code, "latest");
        }

        drop(f)
    }
}
