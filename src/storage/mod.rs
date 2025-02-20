use anyhow::Context;
use bedrock::Config;
use std::path::Path;
use std::str::FromStr;
use tokio::io::AsyncWriteExt;
use tracing::debug;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
    Pool, Sqlite,
};

use crate::repositories::users::{create_user, Role};

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

        if !path.exists() {
            let mut file = tokio::fs::File::create(&path)
                .await
                .context("Failed to create datafile")?;
            file.write_all(INITIAL_DB_CONTENT)
                .await
                .context("Failed to write datafile")?;
        }

        debug!(?path, "Connecting to sqlite database");
        let db = sqlx::sqlite::SqlitePool::connect(path.as_path().to_str().unwrap())
            .await
            .context("Failed to connect to SQLiteDB")?;
        Ok(Self { db })
    }
    /// Converts a `Pathbuf` to a `SqliteLayer`
    pub async fn from_path(value: impl AsRef<Path>) -> anyhow::Result<Self> {
        let mut file = tokio::fs::File::create(value.as_ref())
            .await
            .context("Failed to create datafile")?;
        file.write_all(INITIAL_DB_CONTENT)
            .await
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

    pub async fn ingest(&self, cfg: &Config) -> anyhow::Result<()> {
        let mut tx = self.db.begin().await.unwrap();
        for user in &cfg.accounts.competitors {
            create_user(&mut *tx, &user.name, &user.password, Role::Competitor)
                .await
                .context("Failed to create user")?;
        }

        tx.commit()
            .await
            .context("Failed to commit user ingestion transaction")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::mock_db;
    use bedrock::Config;

    #[tokio::test]
    async fn ingestion_works() {
        let mut file = tokio::fs::File::open("tests/single.toml").await.unwrap();
        let cfg = Config::read_async(&mut file, Some("single.toml"))
            .await
            .unwrap();
        let (f, sql_layer) = mock_db().await;
        let db = sql_layer.write().await;
        db.ingest(&cfg).await.expect("Failed to ingest config");

        drop(f)
    }
}
