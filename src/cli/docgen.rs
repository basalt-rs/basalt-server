use std::{env::temp_dir, io::Write, sync::Arc};

use anyhow::Context;
use tokio::sync::RwLock;
use utoipa::{OpenApi, ToSchema};

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

    // delete file if exists
    if path.exists() {
        std::fs::remove_file(&path).expect("Failed to remove old spec");
    }

    // write content to docs file
    let mut file = std::fs::File::create_new(path).expect("Failed to open new spec");
    file.write_all(content.as_bytes())
        .expect("Failed to write file");

    Ok(())
}
