use std::time::Duration;

use sqlx::SqliteExecutor;

use crate::repositories::{
    submissions::{create_submission_history, SubmissionHistory, SubmissionId},
    users::User,
};
pub async fn dummy_submission(
    db: impl SqliteExecutor<'_> + Copy,
    submitter: &User,
    score: f64,
    question_index: usize,
) -> SubmissionHistory {
    create_submission_history(
        db,
        crate::repositories::submissions::NewSubmissionHistory {
            id: SubmissionId::new(),
            submitter: &submitter.id,
            code: "",
            question_index,
            compile_result: None,
            language: "java",
            test_only: false,
        },
    )
    .await
    .unwrap()
    .finish(db, score, true, Duration::from_secs(1))
    .await
    .unwrap()
}
