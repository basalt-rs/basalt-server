use rhai::{Engine, EvalAltResult, Scope, AST};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::mpsc;
use tracing::{error, info, trace, warn};

use super::events::ServerEvent;
use crate::server::AppState;

pub struct RhaiHookHandler {
    rx: mpsc::UnboundedReceiver<(ServerEvent, Arc<AppState>)>,
    asts: Vec<AST>,
    engine: Engine,
}

impl RhaiHookHandler {
    pub fn create() -> (Self, mpsc::UnboundedSender<(ServerEvent, Arc<AppState>)>) {
        // create message queue
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<(ServerEvent, Arc<AppState>)>();

        let mut engine = Engine::new();
        engine.register_type::<ServerEvent>();

        (
            Self {
                rx,
                asts: Vec::new(),
                engine,
            },
            tx,
        )
    }

    /// Begin handling events sent over the channel
    ///
    /// Each event is handled in a separate thread. Panics
    /// are recovered from gracefully.
    pub async fn start(&mut self) {
        loop {
            if let Some((event, state)) = self.rx.recv().await {
                trace!("rhai handler received event");
                let state = state.clone();
                // on first run, go ahead and compile all scripts
                if self.asts.is_empty() {
                    // TODO(Jack): Support announcement fn registration (among others)
                    // self.engine.register_fn("announce", func);
                    for h in &state.config.integrations.event_handlers {
                        if let Some(ext) = h.extension() {
                            if ext != "rhai" {
                                info!("Skipping non-rhai script {:?}", &h);
                                continue;
                            }
                        }
                        if let Ok(ast) = self.engine.compile_file(h.clone()) {
                            self.asts.push(ast);
                        } else {
                            warn!("Failed to compile rhai script: {:?}", &h);
                        }
                    }
                }

                for ast in self.asts.iter() {
                    let mut scope = Scope::new();
                    let result = self.engine.call_fn::<i64>(
                        &mut scope,
                        ast,
                        event.get_fn_name(),
                        (event.clone(),),
                    );

                    match result {
                        Ok(_) => {}
                        Err(err) => match *err {
                            EvalAltResult::ErrorFunctionNotFound(_, _) => {}
                            e => {
                                error!("Failed to evaluate handler: {}", e);
                            }
                        },
                    }
                }
            };
        }
    }
}
