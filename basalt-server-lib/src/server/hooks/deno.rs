use super::events::ServerEvent;
use anyhow::Context;
use rustyscript::{json_args, Module, Runtime, RuntimeOptions};
use std::{path::PathBuf, time::Duration};
use tracing::debug;

pub fn evaluate(event: ServerEvent, path: &PathBuf) -> anyhow::Result<()> {
    let main_module = Module::load(path).context("Failed to load provided module")?;
    let entrypoint = event.get_fn_name();
    let mut runtime = Runtime::new(RuntimeOptions {
        timeout: Duration::from_secs(20),
        default_entrypoint: Some(entrypoint.into()),
        ..Default::default()
    })
    .context("Failed to initialize runtime")?;
    let module_handle = runtime
        .load_module(&main_module)
        .context("Failed to load module into runtime")?;
    match runtime.call_entrypoint_immediate::<()>(&module_handle, json_args!(event)) {
        Ok(()) => Ok(()),
        Err(rustyscript::Error::MissingEntrypoint(_)) => {
            debug!("A handler not provided for this function: {}", entrypoint);
            Ok(())
        }
        e => e,
    }
    .context("Failed to execute event handler")?;
    runtime
        .block_on_event_loop(Default::default(), Default::default())
        .context("Failed to block on event loop")?;
    Ok(())
}
