use std::path::PathBuf;

use sqlx::{Pool, Sqlite};
use thiserror::Error;

pub struct SqliteLayer {
    pub db: Pool<Sqlite>,
}

#[derive(Debug, Error)]
pub enum SqliteLayerCreationError {
    #[error("Failed to determine system's data folder")]
    FailedToDetermineDataFolder,
    #[error("Failed to create datafile")]
    FailedToCreateDatafile(String),
    #[error("Failed to connect to sqlite: {0}")]
    FailedToConnectSqlite(String),
}

impl SqliteLayer {
    /// Create a new sqlite db and apply migrations.
    ///
    /// An attempt will be made to create a new file in your data folder if
    /// one does not already exist to contain this new database.
    ///
    /// ```rust
    /// pub fn main() {
    ///     let sqlite_layer = SqliteLayer::new("competition-title".into()).unwrap();
    /// }
    /// ```
    pub async fn new(title: String) -> Result<Self, SqliteLayerCreationError> {
        let mut path = directories::BaseDirs::new()
            .ok_or(SqliteLayerCreationError::FailedToDetermineDataFolder)?
            .data_local_dir()
            .join("basalt-server")
            .join(title);
        if !path.exists() {
            std::fs::create_dir_all(&path).unwrap()
        }
        path = path.join("data").with_extension("db");
        std::fs::File::create(&path)
            .map_err(|e| SqliteLayerCreationError::FailedToCreateDatafile(e.to_string()))?;
        dbg!(&path);
        let db = sqlx::sqlite::SqlitePool::connect(dbg!(path.as_path().to_str().unwrap()))
            .await
            .map_err(|e| SqliteLayerCreationError::FailedToConnectSqlite(e.to_string()))?;
        Ok(Self { db })
    }
    /// Converts a `Pathbuf` to a `SqliteLayer`
    pub async fn from_pathbuf(value: &PathBuf) -> Result<Self, SqliteLayerCreationError> {
        let db = sqlx::sqlite::SqlitePool::connect(dbg!(value.as_path().to_str().unwrap()))
            .await
            .map_err(|e| SqliteLayerCreationError::FailedToConnectSqlite(e.to_string()))?;
        Ok(Self { db })
    }
}
