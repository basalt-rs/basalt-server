use super::events::ServerEvent;
use anyhow::Context;
use std::{path::PathBuf, rc::Rc};

pub async fn evaluate(event: ServerEvent, path: PathBuf) -> anyhow::Result<()> {
    let main_module = deno_core::resolve_path(
        path.to_str().context("Failed to stringify path")?,
        &std::env::current_dir().context("Failed to get info about current directory")?,
    )
    .context("Failed to read main module")?;
    let mut runtime = deno_core::JsRuntime::new(deno_core::RuntimeOptions {
        module_loader: Some(Rc::new(deno_core::FsModuleLoader)),
        ..Default::default()
    });
    let event_json = serde_json::to_string(&event).context("Failed to serialize event data")?;
    runtime
        .execute_script("[init]", format!("globalThis.basaltEvent = {}", event_json))
        .context("Failed to initialize state")?;
    let mod_id = runtime
        .load_main_es_module(&main_module)
        .await
        .context("Failed to load main module")?;
    let result = runtime.mod_evaluate(mod_id);
    runtime
        .run_event_loop(Default::default())
        .await
        .context("Something went running event loop")?;
    result
        .await
        .context("Something went wrong evaluating main module")?;
    Ok(())
}

pub fn run(event: ServerEvent, path: PathBuf) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("Failed to build Javascript runtime thread")?;
    runtime
        .block_on(evaluate(event, path))
        .context("Failed to execute event-handler code")
}
