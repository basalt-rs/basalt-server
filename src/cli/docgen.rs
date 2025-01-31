use std::sync::Arc;

use anyhow::Context;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use utoipa::OpenApi;

use crate::{server::AppState, storage::SqliteLayer};

const SPEC_PATH: &str = "./openapi.yaml";

#[derive(OpenApi)]
#[openapi()]
struct ApiDoc;

pub async fn handle() -> anyhow::Result<()> {
    let tempfile = async_tempfile::TempFile::new()
        .await
        .context("Failed to create tempfile")?;
    let sqlite_layer = SqliteLayer::from_pathbuf(tempfile.file_path())
        .await
        .context("Failed to create sqlite layer")?;
    let dummy_state = Arc::new(AppState {
        db: RwLock::new(sqlite_layer),
    });
    let router = crate::server::doc_router(dummy_state);
    let content = ApiDoc::openapi()
        .merge_from(router.into_openapi())
        .to_yaml()
        .context("Failed to serialize to YAML")?;

    let path = std::path::Path::new(SPEC_PATH);

    // write content to docs file
    let mut file = tokio::fs::File::create(path)
        .await
        .expect("Failed to open new spec");
    file.write_all(content.as_bytes())
        .await
        .expect("Failed to write file");

    Ok(())
}
