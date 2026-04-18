const SPEC_PATH: &str = "../openapi.yaml";

#[cfg(feature = "doc-gen")]
async fn gen_docs(path: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::{io::BufRead, process::Stdio};
    use tokio::process::Command;

    /// Returns a `tokio::process::Command` configured to run `redocly`.
    /// Uses the `redocly` binary directly if found on PATH, otherwise
    /// falls back to `npx redocly`.
    pub fn redocly_command() -> Command {
        if which::which("redocly").is_ok() {
            Command::new("redocly")
        } else {
            let mut cmd = Command::new("npx");
            cmd.arg("redocly").arg("-y");
            cmd
        }
    }

    let out_dir = path.with_file_name("doc").join("index.html");

    // npx -y @redocly/cli build-docs <SPEC_PATH> -o <out_dir>
    let result = redocly_command()
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
    Ok(())
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    println!("cargo::rerun-if-changed={}", SPEC_PATH);
    println!("cargo::rerun-if-changed=../basalt-server-lib/src/services");

    #[cfg(feature = "doc-gen")]
    {
        use anyhow::Context;
        use basalt_server_lib::{server::AppState, storage::SqliteLayer};
        use std::{path::Path, sync::Arc};
        use utoipa::OpenApi;

        let tempfile = async_tempfile::TempFile::new()
            .await
            .context("Failed to create tempfile")?;

        let sqlite_layer = SqliteLayer::from_path(tempfile.file_path())
            .await
            .context("Failed to create sqlite layer")?;

        let dummy_state = Arc::new(AppState::new(
            sqlite_layer,
            bedrock::Config::default(),
            None,
        ));
        let router = basalt_server_lib::server::doc_router(dummy_state);

        #[derive(OpenApi)]
        #[openapi()]
        struct ApiDoc;
        let content = ApiDoc::openapi()
            .merge_from(router.into_openapi())
            .to_yaml()
            .context("Failed to serialize to YAML")?;

        let path = Path::new(SPEC_PATH);
        let write = if path.exists() {
            let existing = tokio::fs::read_to_string(path)
                .await
                .with_context(|| format!("reading existing {} file", SPEC_PATH))?;
            existing != content
        } else {
            true
        };

        if write {
            tokio::fs::write(path, content)
                .await
                .with_context(|| format!("Writing to {}", SPEC_PATH))?;

            gen_docs(path).await?;
        }
    }
    Ok(())
}
