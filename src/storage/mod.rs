use std::io::Write;
use std::path::Path;

use sqlx::{Pool, Sqlite};
use thiserror::Error;

const INITIAL_DB_CONTENT: &[u8] = include_bytes!("../../initial_data.db");

pub struct SqliteLayer {
    pub db: Pool<Sqlite>,
}

#[derive(Debug, Error)]
pub enum SqliteLayerCreationError {
    #[error("Failed to determine system's data folder")]
    DataFolderDetermination,
    #[error("Failed to create datafile: {0}")]
    DatafileCreation(String),
    #[error("Failed to read datafile")]
    DatafileRead(String),
    #[error("Failed to connect to sqlite: {0}")]
    Connection(String),
}

impl SqliteLayer {
    /// Create a new sqlite db and apply migrations.
    ///
    /// An attempt will be made to create a new file in your data folder if
    /// one does not already exist to contain this new database.
    ///
    /// ```rust
    /// let sqlite_layer = SqliteLayer::new("competition-title".into()).unwrap();
    /// ```
    pub async fn new(title: String) -> Result<Self, SqliteLayerCreationError> {
        let mut path = directories::BaseDirs::new()
            .ok_or(SqliteLayerCreationError::DataFolderDetermination)?
            .data_local_dir()
            .join("basalt-server")
            .join(title);
        if !path.exists() {
            std::fs::create_dir_all(&path).unwrap()
        }
        path = path.join("data").with_extension("db");
        let mut file = std::fs::File::create(&path)
            .map_err(|e| SqliteLayerCreationError::DatafileCreation(e.to_string()))?;
        file.write_all(INITIAL_DB_CONTENT)
            .map_err(|e| SqliteLayerCreationError::DatafileCreation(e.to_string()))?;
        let db = sqlx::sqlite::SqlitePool::connect(dbg!(path.as_path().to_str().unwrap()))
            .await
            .map_err(|e| SqliteLayerCreationError::Connection(e.to_string()))?;
        Ok(Self { db })
    }
    /// Converts a `Pathbuf` to a `SqliteLayer`
    pub async fn from_pathbuf(value: &Path) -> Result<Self, SqliteLayerCreationError> {
        dbg!(value);
        let mut file = std::fs::File::create(value)
            .map_err(|e| SqliteLayerCreationError::DatafileCreation(e.to_string()))?;
        file.write_all(INITIAL_DB_CONTENT)
            .map_err(|e| SqliteLayerCreationError::DatafileCreation(dbg!(e.to_string())))?;
        let db = sqlx::sqlite::SqlitePool::connect(dbg!(value.to_str().unwrap()))
            .await
            .map_err(|e| SqliteLayerCreationError::Connection(e.to_string()))?;
        Ok(Self { db })
    }
}
