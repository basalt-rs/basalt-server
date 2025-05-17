use std::fmt::Display;

use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use argon2::{PasswordHash, PasswordVerifier};
use rand::rngs::OsRng;
use redact::Secret;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::SqliteExecutor;
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

impl From<&str> for Username {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl Display for Username {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Username {
    pub fn as_str(&self) -> &str {
        &self.0
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
pub struct UserId(pub String);

impl UserId {
    // Default feels wrong here as each call to this function generates a different value.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        use rand::{distributions::Alphanumeric, Rng};
        let id = rand::thread_rng()
            .sample_iter(Alphanumeric)
            .take(20)
            .map(char::from)
            .collect::<String>();
        Self(id)
    }
}

impl Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, FromRow, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: UserId,
    pub username: Username,
    pub display_name: Option<String>,
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

pub async fn get_user_by_username(sql: &SqliteLayer, name: Username) -> Result<User, GetUserError> {
    sqlx::query_as!(User, "SELECT * from users WHERE username = $1", name)
        .fetch_optional(&sql.db)
        .await
        .map_err(|e| GetUserError::QueryError(e.to_string()))?
        .ok_or(GetUserError::UserNotFound {
            property: "username",
            value: name.0,
        })
}

pub async fn get_user_by_id(db: impl SqliteExecutor<'_>, id: UserId) -> Result<User, GetUserError> {
    sqlx::query_as!(User, "SELECT * from users WHERE id = $1", id)
        .fetch_optional(db)
        .await
        .map_err(|e| GetUserError::QueryError(e.to_string()))?
        .ok_or(GetUserError::UserNotFound {
            property: "id",
            value: id.0,
        })
}

// I so desperately want this to return a stream, but sqlx makes that a total pain...
pub async fn get_users_with_role(
    db: impl SqliteExecutor<'_>,
    role: Role,
) -> Result<Vec<User>, sqlx::Error> {
    let role = i32::from(role);
    sqlx::query_as!(User, "SELECT * from users WHERE role = $1", role)
        .fetch_all(db)
        .await
}

#[derive(Debug, FromRow, Deserialize)]
pub struct UserLogin {
    pub username: Username,
    pub password: Secret<String>,
}

pub async fn login_user(
    db: impl SqliteExecutor<'_>,
    login: &UserLogin,
) -> Result<User, GetUserError> {
    let user = sqlx::query_as!(
        User,
        "SELECT * from users WHERE username = $1",
        login.username,
    )
    .fetch_optional(db)
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

#[derive(Debug)]
pub enum CreateUserError {
    Confict,
    Other(sqlx::Error),
}

impl Display for CreateUserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateUserError::Confict => write!(f, "Conflict"),
            CreateUserError::Other(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for CreateUserError {}

/// Creates a user and inserts into database.
///
/// Uses Argon2 to hash the password
pub async fn create_user(
    db: impl SqliteExecutor<'_>,
    username: impl AsRef<str>,
    display_name: Option<&str>,
    password: impl AsRef<str>,
    role: Role,
) -> Result<User, CreateUserError> {
    let salt = SaltString::generate(&mut OsRng);
    let id = UserId::new();
    let username: &str = username.as_ref();
    let password: &str = password.as_ref();
    let password_hash = Argon2::default()
        .hash_password(password.as_ref(), &salt)
        .expect("Failed to hash password")
        .to_string();
    let role_int: i32 = role.into();
    sqlx::query_as!(User,
        "INSERT INTO users (id, username, display_name, password_hash, role) VALUES (?, ?, ?, ?, ?) RETURNING id, username, display_name, password_hash, role",
        id,
        username,
        display_name,
        password_hash,
        role_int
    )
    .fetch_one(db)
    .await
    .map_err(|e| {
        match e   {
            sqlx::Error::Database(ref dbe) => {
                if dbe.is_unique_violation() {
                    CreateUserError::Confict
                } else {
                    CreateUserError::Other(e)
                }
            },
            _ => CreateUserError::Other(e),
        }
    })
}

pub async fn update_user(
    db: impl SqliteExecutor<'_>,
    update: User,
) -> Result<User, CreateUserError> {
    let role_int: i32 = update.role.into();
    let password_hash = update.password_hash.expose_secret();
    sqlx::query_as!(
        User,
        r#"
        UPDATE users SET
            username = ?,
            display_name = ?,
            password_hash = ?,
            role = ?
        WHERE
            id = ?
        RETURNING
            id, username, display_name, password_hash, role
        "#,
        update.username,
        update.display_name,
        password_hash,
        role_int,
        update.id,
    )
    .fetch_one(db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref dbe) => {
            if dbe.is_unique_violation() {
                CreateUserError::Confict
            } else {
                CreateUserError::Other(e)
            }
        }
        _ => CreateUserError::Other(e),
    })
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
        let (f, sql) = mock_db().await;
        let response = get_user_by_id(&sql.db, UserId::new()).await;
        assert!(response.is_err());
        drop(f)
    }

    #[tokio::test]
    async fn get_existing_user_by_username() {
        let (f, sql) = mock_db().await;
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
    async fn get_existing_user_by_id() {
        let (f, sql) = mock_db().await;
        let dummy_user = create_user(
            &sql.db,
            "awesome_user".to_string(),
            Some("Awesome User"),
            "awesome-password".to_string(),
            Role::Competitor,
        )
        .await
        .unwrap();
        let user = get_user_by_id(&sql.db, dummy_user.id)
            .await
            .expect("Failed to find user");
        assert_eq!(user.username, dummy_user.username);
        drop(f)
    }

    #[tokio::test]
    async fn get_correct_user() {
        let (f, sql) = mock_db().await;
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
