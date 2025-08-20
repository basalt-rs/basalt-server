use crate::server::AppState;

use super::events::ServerEvent;
use anyhow::Context;
use rustyscript::{json_args, Module, Runtime, RuntimeOptions};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::oneshot;
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

pub fn create_evaluation_context() -> (JSEvaluator, oneshot::Receiver<anyhow::Result<Vec<()>>>) {
    let (tx, rx) = oneshot::channel();
    let evaluator = JSEvaluator::create(tx);
    (evaluator, rx)
}

pub struct JSEvaluator {
    // result transmitter
    tx: oneshot::Sender<anyhow::Result<Vec<()>>>,
}

impl JSEvaluator {
    pub fn create(tx: oneshot::Sender<anyhow::Result<Vec<()>>>) -> Self {
        Self { tx }
    }

    pub fn start(self, event: ServerEvent, state: Arc<AppState>) {
        std::thread::spawn(move || {
            let results = state
                .config
                .integrations
                .event_handlers
                .iter()
                .map(|p| {
                    let event = event.clone();
                    evaluate(event, p)
                })
                .collect::<anyhow::Result<Vec<()>>>();
            if let Err(e) = self.tx.send(results) {
                tracing::error!("Failed to send evaluation results: {:?}", e);
            }
        });
    }
}
