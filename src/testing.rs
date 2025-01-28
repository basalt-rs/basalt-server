use std::sync::Arc;

use tokio::sync::RwLock;

use crate::storage::SqliteLayer;

pub async fn mock_db() -> (async_tempfile::TempFile, Arc<RwLock<SqliteLayer>>) {
    let db_tempfile = async_tempfile::TempFile::new()
        .await
        .expect("Failed to create temporary file for datafile");

    let sqlite_layer = SqliteLayer::from_pathbuf(db_tempfile.file_path())
        .await
        .expect("Failed to create SqliteDB");

    let db = Arc::new(RwLock::new(sqlite_layer));
    (db_tempfile, db)
}
