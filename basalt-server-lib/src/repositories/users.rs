use anyhow::Context;
use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use argon2::{PasswordHash, PasswordVerifier};
use rand::rngs::OsRng;
use redact::Secret;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::{Executor, Sqlite};
use utoipa::ToSchema;

use crate::storage::SqliteLayer;

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[repr(i32)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Competitor = 0,
    Host = 1,
}

impl From<i32> for Role {
    fn from(value: i32) -> Self {
        match value {
            1 => Role::Host,
            _ => Role::Competitor,
        }
    }
}

impl From<i64> for Role {
    fn from(value: i64) -> Self {
        match value {
            1 => Role::Host,
            _ => Role::Competitor,
        }
    }
}

impl From<Role> for i32 {
    fn from(value: Role) -> Self {
        match value {
            Role::Competitor => 0,
            Role::Host => 1,
        }
    }
}

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
pub struct Username(pub String);

impl Username {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, FromRow, Serialize, Deserialize, ToSchema)]
pub struct User {
    pub username: Username,
    #[serde(skip)]
    pub password_hash: Secret<String>,
    pub role: Role,
}

#[derive(Debug, thiserror::Error)]
pub enum GetUserError {
    #[error("A database error occurred: {0}")]
    QueryError(String),
    #[error("Could not find user of {property} {value}")]
    UserNotFound {
        property: &'static str,
        value: String,
    },
}

#[allow(dead_code)]
pub async fn get_user_by_username(
    sql: &SqliteLayer,
    username: String,
) -> Result<User, GetUserError> {
    sqlx::query_as!(User, "SELECT * from users WHERE username = $1", username)
        .fetch_optional(&sql.db)
        .await
        .map_err(|e| GetUserError::QueryError(e.to_string()))?
        .ok_or(GetUserError::UserNotFound {
            property: "username",
            value: username,
        })
}

#[derive(Debug, FromRow, Deserialize)]
pub struct UserLogin {
    pub username: String,
    pub password: Secret<String>,
}

pub async fn login_user(sql: &SqliteLayer, login: &UserLogin) -> Result<User, GetUserError> {
    let user = sqlx::query_as!(
        User,
        "SELECT * from users WHERE username = $1",
        login.username,
    )
    .fetch_optional(&sql.db)
    .await
    .map_err(|e| GetUserError::QueryError(e.to_string()))?
    .ok_or_else(|| GetUserError::UserNotFound {
        property: "username",
        value: login.username.to_string(),
    })?;

    // user.password
    let password_hash = PasswordHash::new(user.password_hash.expose_secret()).unwrap();

    if Argon2::default()
        .verify_password(login.password.expose_secret().as_bytes(), &password_hash)
        .is_ok()
    {
        Ok(user)
    } else {
        Err(GetUserError::UserNotFound {
            property: "username",
            value: login.username.to_string(),
        })
    }
}

/// Creates a user and inserts into database.
///
/// Uses Argon2 to hash the password
pub async fn create_user(
    db: impl Executor<'_, Database = Sqlite>,
    username: impl AsRef<str>,
    password: impl AsRef<str>,
    role: Role,
) -> anyhow::Result<User> {
    let salt = SaltString::generate(&mut OsRng);
    let username: &str = username.as_ref();
    let password: &str = password.as_ref();
    let password_hash = Argon2::default()
        .hash_password(password.as_ref(), &salt)
        .expect("Failed to hash password")
        .to_string();
    let role_int: i32 = role.into();
    sqlx::query_as!(User,
            "INSERT INTO users (username, password_hash, role) VALUES (?, ?, ?) RETURNING username, password_hash, role",
            username,
            password_hash,
            role_int
        ).fetch_one(db).await.context("Failed to create user")
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum QuestionState {
    Pass,
    Fail,
    InProgress,
    NotAttempted,
}

#[cfg(test)]
mod tests {
    use crate::testing::mock_db;

    use super::*;
    #[tokio::test]
    async fn get_nonexistent_user() {
        let (f, sql_layer) = mock_db().await;
        let db = sql_layer.read().await;
        let response = get_user_by_username(&db, "superuser".into()).await;
        assert!(response.is_err());
        drop(f)
    }

    #[tokio::test]
    async fn get_existing_user() {
        let (f, sql_layer) = mock_db().await;
        let sql = sql_layer.write().await;
        let dummy_user = crate::testing::users_repositories::dummy_user(
            &sql.db,
            "awesome_user".to_string(),
            "awesome-password".to_string(),
            Role::Competitor,
        )
        .await;
        let user = get_user_by_username(&sql, "awesome_user".into())
            .await
            .expect("Failed to find user");
        assert_eq!(user.username, dummy_user.username);
        drop(f)
    }
    #[tokio::test]
    async fn get_correct_user() {
        let (f, sql_layer) = mock_db().await;
        let sql = sql_layer.write().await;
        let dummy_user = crate::testing::users_repositories::dummy_user(
            &sql.db,
            "awesome_user".to_string(),
            "awesome-password".to_string(),
            Role::Competitor,
        )
        .await;
        crate::testing::users_repositories::dummy_user(
            &sql.db,
            "awesome_user2".to_string(),
            "awesome-password".to_string(),
            Role::Competitor,
        )
        .await;
        let user = get_user_by_username(&sql, "awesome_user".into())
            .await
            .expect("Failed to find user");
        assert_eq!(user.username, dummy_user.username);
        drop(f)
    }
}
