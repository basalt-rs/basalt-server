use redact::{expose_secret, Secret};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::storage::SqliteLayer;

#[derive(Debug)]
#[repr(i32)]
pub enum Role {
    Competitor = 0,
    Admin = 1,
}

impl From<i32> for Role {
    fn from(value: i32) -> Self {
        match value {
            1 => Role::Admin,
            _ => Role::Competitor,
        }
    }
}

impl From<Role> for i32 {
    fn from(value: Role) -> Self {
        match value {
            Role::Competitor => 0,
            Role::Admin => 1,
        }
    }
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct User {
    pub username: String,
    #[serde(serialize_with = "expose_secret")]
    pub password_hash: Secret<String>,
    pub role: i64,
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
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
        let mut db = sql_layer.write().await;
        let dummy_user = crate::testing::users_repositories::dummy_user(
            &mut db,
            "awesome_user".to_string(),
            "awesome-password".to_string(),
            Role::Competitor,
        )
        .await;
        let user = get_user_by_username(&db, "awesome_user".into())
            .await
            .expect("Failed to find user");
        assert_eq!(user.username, dummy_user.username);
        drop(f)
    }
    #[tokio::test]
    async fn get_correct_user() {
        let (f, sql_layer) = mock_db().await;
        let mut db = sql_layer.write().await;
        let dummy_user = crate::testing::users_repositories::dummy_user(
            &mut db,
            "awesome_user".to_string(),
            "awesome-password".to_string(),
            Role::Competitor,
        )
        .await;
        crate::testing::users_repositories::dummy_user(
            &mut db,
            "awesome_user2".to_string(),
            "awesome-password".to_string(),
            Role::Competitor,
        )
        .await;
        let user = get_user_by_username(&db, "awesome_user".into())
            .await
            .expect("Failed to find user");
        assert_eq!(user.username, dummy_user.username);
        drop(f)
    }
}
