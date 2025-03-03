use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};

use crate::{
    repositories::users::{Role, User},
    storage::SqliteLayer,
};

pub async fn dummy_user(
    sql: &mut SqliteLayer,
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
    let role_int: i32 = role.into();
    sqlx::query_as!(User,
        "INSERT INTO users (username, password_hash, role) VALUES (?, ?, ?) RETURNING username, password_hash, role",
        name,
        password_hash,
        role_int
    ).fetch_one(&sql.db).await.expect("Failed to create user")
}
