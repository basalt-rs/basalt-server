use anyhow::Context;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, trace};

use super::events::ServerEvent;
use crate::server::{hooks::evaluator::create_evaluation_context, AppState};

pub struct EventHookHandler {
    rx: mpsc::UnboundedReceiver<ServerEvent>,
}

impl EventHookHandler {
    pub fn create() -> (Self, EventDispatcherService) {
        // create message queue
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ServerEvent>();

        (Self { rx }, EventDispatcherService::new(tx))
    }

    /// Begin handling events sent over the channel
    ///
    /// Each event is handled in a separate thread. Panics
    /// are recovered from gracefully.
    pub async fn start(&mut self, state: Arc<AppState>) {
        loop {
            if let Some(event) = self.rx.recv().await {
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

pub struct EventDispatcherService {
    tx: mpsc::UnboundedSender<ServerEvent>,
}

impl EventDispatcherService {
    pub fn new(tx: mpsc::UnboundedSender<ServerEvent>) -> Self {
        Self { tx }
    }

    pub fn dispatch(&self, event: ServerEvent) -> anyhow::Result<()> {
        self.tx.send(event).context("Failed to transmit event")?;
        Ok(())
    }
}
