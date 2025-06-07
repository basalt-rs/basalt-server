use anyhow::Context;
use std::sync::Arc;
use tokio::{sync::mpsc, task::JoinSet};
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
                let webhooks = state.config.integrations.webhooks.clone();
                let hook_event = event.clone();
                webhooks
                    .into_iter()
                    .map(|webhook_url| {
                        let client = reqwest::Client::new();
                        let event = hook_event.clone();
                        async move {
                            match client.post(webhook_url.clone()).json(&event).send().await {
                                Ok(r) => trace!(
                                    "Published event to {} with status {}",
                                    webhook_url,
                                    r.status()
                                ),
                                Err(e) => {
                                    error!("Error publishing event to {}, {:?}", webhook_url, e)
                                }
                            }
                            ()
                        }
                    })
                    .collect::<JoinSet<()>>()
                    .join_all()
                    .await;
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
