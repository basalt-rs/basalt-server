use sqlx::SqliteExecutor;

use crate::repositories::{
    submissions::{create_submission_history, SubmissionHistory},
    users::User,
};
pub async fn dummy_submission(
    db: impl SqliteExecutor<'_>,
    submitter: &User,
    score: f64,
) -> SubmissionHistory {
    create_submission_history(
        db,
        crate::repositories::submissions::NewSubmissionHistory {
            submitter: &submitter.username,
            compile_fail: false,
            code: "",
            question_index: 0,
            score,
            success: true,
            language: "java",
        },
    )
    .await
    .unwrap()
}
