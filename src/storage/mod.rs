use anyhow::Context;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
    Pool, Sqlite,
};

const INITIAL_DB_CONTENT: &[u8] = include_bytes!(env!("INITIAL_DATA_PATH"));

pub struct SqliteLayer {
    pub db: Pool<Sqlite>,
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
    pub async fn new(title: impl AsRef<str>) -> anyhow::Result<Self> {
        let mut path = directories::ProjectDirs::from("rs", "basalt", "basalt-server")
            .context("Failed to resolve project directory")?
            .data_local_dir()
            .join(title.as_ref());
        tokio::fs::create_dir_all(&path)
            .await
            .expect("failed to create database files");
        path = path.join("data").with_extension("db");
        let mut file = std::fs::File::create(&path).context("Failed to create datafile")?;
        file.write_all(INITIAL_DB_CONTENT)
            .context("Failed to write datafile")?;
        let db = sqlx::sqlite::SqlitePool::connect(dbg!(path.as_path().to_str().unwrap()))
            .await
            .context("Failed to connect to SQLiteDB")?;
        Ok(Self { db })
    }
    /// Converts a `Pathbuf` to a `SqliteLayer`
    pub async fn from_path(value: impl AsRef<Path>) -> anyhow::Result<Self> {
        let mut file = std::fs::File::create(value.as_ref()).context("Failed to create datafile")?;
        file.write_all(INITIAL_DB_CONTENT)
            .context("Failed to write default database to datafile")?;
        drop(file);
        let uri = format!("sqlite://{}", value.as_ref().to_str().unwrap());
        let opts = SqliteConnectOptions::from_str(&uri)
            .context("Invalid options")?
            .journal_mode(SqliteJournalMode::Wal)
            .read_only(false);
        let db = sqlx::sqlite::SqlitePool::connect_with(opts)
            .await
            .context("Failed to connect to SQLite DB")?;
        Ok(Self { db })
    }
}
