use std::time::{Duration, SystemTime};

use redact::Secret;
use serde::{Deserialize, Serialize};
use sqlx::{prelude::FromRow, SqliteExecutor};
use utoipa::ToSchema;

use crate::{
    repositories::users::{Role, UserId},
    storage::SqliteLayer,
};

use super::users::{User, Username};

#[derive(
    Debug,
    Clone,
    Hash,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    ToSchema,
    derive_more::From,
    derive_more::Into,
    sqlx::Type,
)]
#[sqlx(transparent)]
pub struct SessionId(pub String);

impl SessionId {
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

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Session {
    pub user_id: UserId,
    pub session_id: SessionId,
}

#[derive(Debug, thiserror::Error)]
pub enum CreateSessionError {
    #[error("A database error occurred: {0}")]
    QueryError(String),
}

pub async fn create_session(
    db: impl SqliteExecutor<'_>,
    user: &User,
) -> Result<SessionId, CreateSessionError> {
    let session_id = SessionId::new();

    let expire: u32 = (SystemTime::now() + Duration::from_secs(60 * 60 * 24 * 30))
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("System time is before unix epoch")
        .as_secs()
        .try_into()
        .expect("This code will be gone by the year 2106...");

    sqlx::query_as!(
        Session,
        "INSERT INTO sessions (session_id, user_id, expires_at) VALUES ($1, $2, $3)",
        session_id,
        user.id,
        expire,
    )
    .execute(db)
    .await
    .map_err(|e| CreateSessionError::QueryError(e.to_string()))?;

    Ok(session_id)
}

#[derive(Debug, thiserror::Error)]
pub enum GetSessionError {
    #[error("A database error occurred: {0}")]
    QueryError(String),
    #[error("Could not get user with session {session_id}.")]
    SessionNotFound { session_id: String },
}

pub async fn get_user_from_session(
    sql: &SqliteLayer,
    session_id: &str,
) -> Result<User, GetSessionError> {
    #[derive(sqlx::FromRow)]
    struct SessionUser {
        expires_at: i64,
        id: UserId,
        username: Username,
        display_name: Option<String>,
        password_hash: Secret<String>,
        role: Role,
    }

    let session = sqlx::query_as!(SessionUser, "SELECT users.*, expires_at FROM users JOIN sessions ON users.id = sessions.user_id WHERE session_id = $1", session_id)
        .fetch_optional(&sql.db)
        .await
        .map_err(|e| GetSessionError::QueryError(e.to_string()))?
        .ok_or_else(|| GetSessionError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;

    if SystemTime::UNIX_EPOCH + Duration::from_secs(session.expires_at as u64) < SystemTime::now() {
        sqlx::query!("DELETE FROM sessions WHERE session_id = $1", session_id)
            .execute(&sql.db)
            .await
            .map_err(|e| GetSessionError::QueryError(e.to_string()))?;

        return Err(GetSessionError::SessionNotFound {
            session_id: session_id.to_string(),
        });
    }

    Ok(User {
        id: session.id,
        username: session.username,
        display_name: session.display_name,
        password_hash: session.password_hash,
        role: session.role,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum CloseSessionError {
    #[error("A database error occurred: {0}")]
    QueryError(#[from] sqlx::Error),
}

pub async fn close_session(
    db: impl SqliteExecutor<'_>,
    session_id: &SessionId,
) -> Result<(), CloseSessionError> {
    sqlx::query!("delete from sessions where session_id = $1", session_id)
        .execute(db)
        .await?;

    Ok(())
}
