use crate::storage::SqliteLayer;

pub mod submissions_repositories;
pub mod users_repositories;

pub const SAMPLE_1: &str = include_str!("../../../samples/single.toml");

pub async fn mock_db() -> (async_tempfile::TempFile, SqliteLayer) {
    let db_tempfile = async_tempfile::TempFile::new()
        .await
        .expect("Failed to create temporary file for datafile");

    let sqlite_layer = SqliteLayer::from_path(db_tempfile.file_path())
        .await
        .expect("Failed to create SqliteDB");

    (db_tempfile, sqlite_layer)
}
