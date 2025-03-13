use std::{io::BufRead, path::Path, process::Stdio, sync::Arc};

use anyhow::Context;
use tokio::{fs, process::Command};
use utoipa::OpenApi;

use basalt_server_lib::{server::AppState, storage::SqliteLayer};

const SPEC_PATH: &str = "../openapi.yaml";

#[derive(OpenApi)]
#[openapi()]
struct ApiDoc;

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    println!("cargo::rerun-if-changed={}", SPEC_PATH);
    let tempfile = async_tempfile::TempFile::new()
        .await
        .context("Failed to create tempfile")?;

    let sqlite_layer = SqliteLayer::from_path(tempfile.file_path())
        .await
        .context("Failed to create sqlite layer")?;

    let dummy_state = Arc::new(AppState::new(sqlite_layer, bedrock::Config::default()));
    let router = basalt_server_lib::server::doc_router(dummy_state);

    let content = ApiDoc::openapi()
        .merge_from(router.into_openapi())
        .to_yaml()
        .context("Failed to serialize to YAML")?;

    let path = Path::new(SPEC_PATH);
    let write = if path.exists() {
        let existing = fs::read_to_string(path)
            .await
            .with_context(|| format!("reading existing {} file", SPEC_PATH))?;
        existing != content
    } else {
        true
    };

    if write {
        fs::write(path, content)
            .await
            .with_context(|| format!("Writing to {}", SPEC_PATH))?;

        let out_dir = path.with_file_name("doc").join("index.html");

        // npx -y @redocly/cli build-docs <SPEC_PATH> -o <out_dir>
        let result = Command::new("npx")
            .arg("-y")
            .arg("@redocly/cli")
            .arg("build-docs")
            .arg(SPEC_PATH)
            .arg("-o")
            .arg(out_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawning redocly command")?
            .wait_with_output()
            .await
            .context("waiting for redocly command")?;

        if !result.status.success() {
            println!("cargo::warning=Redocly exited with nonzero exit code.");
            // Give a lot of error information so we can easily fix
            for line in result.stdout.lines() {
                println!("cargo::warning=[STDOUT] {}", line.unwrap());
            }
            for line in result.stderr.lines() {
                println!("cargo::warning=[STDERR] {}", line.unwrap());
            }
        }
    }

    Ok(())
}
