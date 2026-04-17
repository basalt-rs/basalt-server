use sqlx::SqliteExecutor;

use crate::{
    repositories::{
        self,
        users::{Role, User},
    },
    storage::SqliteLayer,
};

pub mod submissions_repositories;
pub mod users_repositories;

pub const SAMPLE_1: &str = include_str!("../../../samples/single.toml");

pub async fn mock_db() -> SqliteLayer {
    SqliteLayer::in_memory()
        .await
        .expect("Failed to create SqliteDB")
}

pub fn setup_test_logger() {
    // ignore error since the logger would already be setup
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
}

/// Mock the application state
///
/// Assigns `Arc<AppState>` to the variable name passed in
///
/// ```
/// mock_state!(let state);
/// mock_state!(let state; Config { .. });
/// ```
#[macro_export]
macro_rules! mock_state {
    (let $state: ident) => {
        mock_state!($state, Config::default());
    };
    (let $state: ident; $config: expr) => {
        let db = $crate::testing::mock_db().await;
        let mut state = AppState::new(db, $config, None);
        state.init().await.unwrap();
        let $state = Arc::new(state);
    };
}

/// Create a mock user
///
/// ```
/// user!("some name", Competitor);
/// user!("some name", Host);
/// user!("some name", Host, { display_name: Some("hello".into()) });
/// ```
#[macro_export]
macro_rules! user {
    ($name: literal, $role: ident) => {
        user!($name, $role, {})
    };
    ($name: literal, $role: ident, {$($key: ident: $value: expr),*$(,)?}) => {
        User {
            $($key: $value,)*
            ..User {
                id: $crate::repositories::users::UserId::new(),
                username: $name.into(),
                display_name: None,
                password_hash: redact::Secret::from(""),
                role: Role::$role,
            }
        }
    };
}

/// Create a user and insert it into the database
///
/// ```
/// db_user(&state.db, "some name", Role::Competitor).await;
/// db_user(&state.db, "some name", Role::Host).await;
/// ```
pub async fn db_user(db: impl SqliteExecutor<'_>, name: impl AsRef<str>, role: Role) -> User {
    repositories::users::create_user(db, name, None, "", role)
        .await
        .unwrap()
}
