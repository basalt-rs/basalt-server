use rand::{distributions::Alphanumeric, Rng};
use redact::{expose_secret, Secret};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::storage::SqliteLayer;

use super::users::User;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Session {
    pub username: String,
    #[serde(serialize_with = "expose_secret")]
    pub password_hash: Secret<String>,
    pub role: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum CreateSessionError {
    #[error("A database error occurred: {0}")]
    QueryError(String),
}

pub async fn create_session(sql: &SqliteLayer, user: &User) -> Result<String, CreateSessionError> {
    // TODO: use ulid or something
    let session_id = rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(20)
        .map(char::from)
        .collect::<String>();
    sqlx::query_as!(
        Session,
        "insert into sessions (session_id, username) values ($1, $2)",
        session_id,
        user.username,
    )
    .execute(&sql.db)
    .await
    .map_err(|e| CreateSessionError::QueryError(e.to_string()))?;

    Ok(session_id)
}

#[derive(Debug, thiserror::Error)]
pub enum CloseSessionError {
    #[error("A database error occurred: {0}")]
    QueryError(#[from] sqlx::Error),
}

pub async fn close_session(sql: &SqliteLayer, session_id: &str) -> Result<(), CloseSessionError> {
    sqlx::query_as!(
        Session,
        "delete from sessions where session_id = $1",
        session_id,
    )
    .execute(&sql.db)
    .await?;

    Ok(())
}
