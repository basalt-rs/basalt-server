use anyhow::Context;
use bedrock::Config;
use derive_more::Deref;
use futures::{future::BoxFuture, stream::BoxStream};
use std::{path::Path, str::FromStr};
use tokio::fs::File;
use tracing::debug;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
    Database, Executor, Pool, Sqlite,
};

use crate::repositories::users::{create_user, Role};

#[derive(Debug, Deref)]
pub struct SqliteLayer {
    db: Pool<Sqlite>,
}

impl SqliteLayer {
    /// Create a new sqlite db and apply migrations.
    ///
    /// An attempt will be made to create a new file in your data folder if
    /// one does not already exist to contain this new database.
    ///
    /// ```no_run
    /// # use basalt_server_lib::storage::SqliteLayer;
    /// # async {
    /// let sqlite_layer = SqliteLayer::new("competition-title").await.unwrap();
    /// # };
    /// ```
    pub async fn new(title: impl AsRef<str>) -> anyhow::Result<(bool, Self)> {
        let path = directories::ProjectDirs::from("rs", "basalt", "basalt-server")
            .context("Failed to resolve project directory")?
            .data_local_dir()
            .join(title.as_ref())
            .join("data.db");

        Self::from_path(&path).await
    }

    /// Create a new instance of the database at a specific path
    async fn from_path(path: impl AsRef<Path>) -> anyhow::Result<(bool, Self)> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("creating database file")?;
        }

        let init = !path.exists();
        if init {
            File::create_new(path)
                .await
                .context("creating database file")?;
        }

        debug!(?path, "Connecting to sqlite database");
        let db = sqlx::sqlite::SqlitePool::connect(path.to_str().unwrap())
            .await
            .context("Failed to connect to SQLiteDB")?;

        // aways intialise, in case new tables need to be created
        let this = Self { db };
        this.init_db().await?;

        Ok((init, this))
    }

    /// Create a new [`SqliteLayer`] using an in-memory database, primarily for testing
    pub async fn in_memory() -> anyhow::Result<Self> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("from_str is given a valid URI")
            .journal_mode(SqliteJournalMode::Wal)
            .read_only(false);

        let db = sqlx::sqlite::SqlitePool::connect_with(opts)
            .await
            .context("Failed to connect to SQLite DB")?;

        let this = Self { db };

        this.init_db().await?;

        Ok(this)
    }

    /// Run the migration.sql to initialise the database tables
    async fn init_db(&self) -> anyhow::Result<()> {
        sqlx::raw_sql(include_str!("../../migration.sql"))
            .execute(&self.db)
            .await
            .context("Intialising Database")?;
        Ok(())
    }

    pub async fn ingest(&self, cfg: &Config) -> anyhow::Result<()> {
        let mut tx = self.db.begin().await.unwrap();
        for user in &cfg.accounts.competitors {
            create_user(
                &mut *tx,
                &user.name,
                user.display_name.as_deref(),
                &user.password,
                Role::Competitor,
            )
            .await
            .context("Failed to create user")?;
        }

        for host in &cfg.accounts.hosts {
            create_user(
                &mut *tx,
                &host.name,
                host.display_name.as_deref(),
                &host.password,
                Role::Host,
            )
            .await
            .context("Failed to create host user")?;
        }

        tx.commit()
            .await
            .context("Failed to commit user ingestion transaction")?;

        Ok(())
    }
}

// just proxy methods to self.db
impl<'a> Executor<'a> for &SqliteLayer {
    type Database = Sqlite;

    fn fetch_many<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxStream<
        'e,
        Result<
            sqlx::Either<
                <Self::Database as Database>::QueryResult,
                <Self::Database as Database>::Row,
            >,
            sqlx::Error,
        >,
    >
    where
        'a: 'e,
        E: 'q + sqlx::Execute<'q, Self::Database>,
    {
        self.db.fetch_many(query)
    }

    fn fetch_optional<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxFuture<'e, Result<Option<<Self::Database as Database>::Row>, sqlx::Error>>
    where
        'a: 'e,
        E: 'q + sqlx::Execute<'q, Self::Database>,
    {
        self.db.fetch_optional(query)
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        sql: &'q str,
        parameters: &'e [<Self::Database as Database>::TypeInfo],
    ) -> BoxFuture<'e, Result<<Self::Database as Database>::Statement<'q>, sqlx::Error>>
    where
        'a: 'e,
    {
        self.db.prepare_with(sql, parameters)
    }

    fn describe<'e, 'q: 'e>(
        self,
        sql: &'q str,
    ) -> BoxFuture<'e, Result<sqlx::Describe<Self::Database>, sqlx::Error>>
    where
        'a: 'e,
    {
        self.db.describe(sql)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        repositories::users::{create_user, Role},
        storage::SqliteLayer,
        testing::{mock_db, users_repositories::get_user_by_username},
    };
    use bedrock::Config;

    #[tokio::test]
    async fn ingestion_works() {
        let cfg = Config::from_str(
            include_str!("../../../samples/single.toml"),
            Some("single.toml"),
        )
        .unwrap();

        let db = mock_db().await;

        db.ingest(&cfg).await.expect("Failed to ingest config");
    }

    #[tokio::test]
    async fn persistent_database() {
        // NOTE: using a tempdir so the file itself isn't created
        let tempdir = async_tempfile::TempDir::new().await.unwrap();

        let file = tempdir.join("database.db");
        assert!(!tokio::fs::try_exists(&file).await.unwrap());

        let (init, layer) = SqliteLayer::from_path(&file).await.unwrap();
        assert!(init);
        create_user(&layer, "foo", None, "password", Role::Host)
            .await
            .unwrap();

        layer.close().await;
        drop(layer);

        let (init, layer) = SqliteLayer::from_path(&file).await.unwrap();
        assert!(!init);

        let user = get_user_by_username(&layer, "foo").await.unwrap();
        assert_eq!(user.username, "foo");
        assert_eq!(user.display_name, None);
        assert_eq!(user.role, Role::Host);

        layer.close().await;
        drop(layer);
    }
}
