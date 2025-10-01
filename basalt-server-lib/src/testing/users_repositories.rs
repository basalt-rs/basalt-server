use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use sqlx::SqliteExecutor;

use crate::repositories::users::{GetUserError, Role, User, UserId};

pub async fn dummy_user(
    db: impl SqliteExecutor<'_>,
    name: impl AsRef<str>,
    password: impl AsRef<[u8]>,
    role: Role,
) -> User {
    let salt = SaltString::generate(&mut OsRng);
    let name = name.as_ref();
    let password_hash = Argon2::default()
        .hash_password(password.as_ref(), &salt)
        .expect("Failed to hash password")
        .to_string();
    let id = UserId::new();
    let role_int: i32 = role.into();
    sqlx::query_as!(User,
        "INSERT INTO users (id, username, display_name, password_hash, role) VALUES (?, ?, ?, ?, ?) RETURNING id, username, display_name, password_hash, role",
        id,
        name,
        None::<String>,
        password_hash,
        role_int
    ).fetch_one(db).await.expect("Failed to create user")
}

pub async fn get_user_by_username(
    db: impl SqliteExecutor<'_>,
    name: impl AsRef<str>,
) -> Result<User, GetUserError> {
    let name = name.as_ref();
    sqlx::query_as!(User, "SELECT * from users WHERE username = $1", name)
        .fetch_optional(db)
        .await
        .map_err(|e| GetUserError::QueryError(e.to_string()))?
        .ok_or(GetUserError::UserNotFound {
            property: "username",
            value: name.to_string(),
        })
}
