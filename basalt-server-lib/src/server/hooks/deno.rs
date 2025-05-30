use super::events::ServerEvent;
use anyhow::Context;
use rustyscript::{json_args, Module, Runtime, RuntimeOptions};
use std::{path::PathBuf, time::Duration};

pub async fn evaluate(event: ServerEvent, path: PathBuf) -> anyhow::Result<()> {
    let main_module = Module::load(path).context("Failed to load provided module")?;
    let current_handle = tokio::runtime::Handle::current();
    let mut runtime = Runtime::with_tokio_runtime_handle(
        RuntimeOptions {
            timeout: Duration::from_secs(20),
            default_entrypoint: Some("handle".to_string()),
            ..Default::default()
        },
        current_handle,
    )
    .context("Failed to initialize runtime")?;
    let module_handle = runtime
        .load_module(&main_module)
        .context("Failed to load module into runtime")?;
    runtime
        .call_entrypoint_async::<()>(&module_handle, json_args!(event))
        .await
        .context("Failed to execute event handler")?;
    Ok(())
}
