use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, trace};

use super::events::ServerEvent;
use crate::server::AppState;
use evaluator::create_evaluation_context;

pub struct EventHookHandler {
    rx: mpsc::UnboundedReceiver<(ServerEvent, Arc<AppState>)>,
}

impl EventHookHandler {
    pub fn create() -> (Self, mpsc::UnboundedSender<(ServerEvent, Arc<AppState>)>) {
        // create message queue
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<(ServerEvent, Arc<AppState>)>();

        (Self { rx }, tx)
    }

    /// Begin handling events sent over the channel
    ///
    /// Each event is handled in a separate thread. Panics
    /// are recovered from gracefully.
    pub async fn start(&mut self) {
        loop {
            if let Some((event, state)) = self.rx.recv().await {
                trace!("received event");
                let state = state.clone();
                tokio::spawn(async move {
                    let (eval, rx) = create_evaluation_context();
                    eval.start(event, state);
                    match rx.await {
                        Ok(Ok(_)) => {
                            trace!("Successfully handled event");
                        }
                        Ok(Err(e)) => {
                            error!("Error handling event: {:?}", e);
                        }
                        Err(e) => {
                            error!("Error receiving value from eval sender: {:?}", e);
                        }
                    };
                });
            };
        }
    }
}

mod evaluator {
    use crate::{
        repositories::{self, users::Username},
        server::{hooks::events::ServerEvent, AppState},
        services::ws::{self, Broadcast, WebSocketSend},
        utils,
    };

    use anyhow::Context;
    use deno_core::OpState;
    use rustyscript::{json_args, Module, Runtime, RuntimeOptions};
    use std::{path::PathBuf, sync::Arc, time::Duration};
    use tokio::sync::oneshot;
    use tracing::debug;

    #[deno_core::op2(async)]
    async fn op_announcement(op_state: &OpState, #[string] msg: String) -> bool {
        let state = op_state.borrow::<Arc<AppState>>().clone();
        let sql = state.db.read().await;

        let new = repositories::announcements::create_announcement(
            &sql.db,
            &Username("SYSTEM".to_owned()),
            &msg,
        )
        .await;
        drop(sql);
        let result = match new {
            Ok(new) => {
                state.websocket.broadcast(ws::WebSocketSend::Broadcast {
                    broadcast: ws::Broadcast::NewAnnouncement(new.clone()),
                });
                if let Err(err) = (ServerEvent::OnAnnouncement {
                    announcer: Username("SYSTEM".into()),
                    announcement: msg,
                    time: utils::utc_now(),
                }
                .dispatch(state.clone()))
                {
                    tracing::error!("Error dispatching announcement event: {:?}", err);
                }
                true
            }
            Err(err) => {
                tracing::error!("Error getting announcements: {:?}", err);
                false
            }
        };
        drop(state);
        result
    }

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

    pub fn create_evaluation_context() -> (JSEvaluator, oneshot::Receiver<anyhow::Result<Vec<()>>>)
    {
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
}
