use std::sync::Arc;
use tokio::{sync::mpsc, task::JoinSet};
use tracing::{error, trace};

use super::events::ServerEvent;
use crate::server::hooks::evaluator::create_evaluation_context;
use crate::server::AppState;

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

pub struct EventWebhookHandler {
    rx: mpsc::UnboundedReceiver<(ServerEvent, Arc<AppState>)>,
}

impl EventWebhookHandler {
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
        // Create a single client instance to reuse across events
        let client = reqwest::Client::new();

        loop {
            if let Some((event, state)) = self.rx.recv().await {
                let webhooks = &state.config.integrations.webhooks;
                let mut join_set = JoinSet::new();

                for webhook_url in webhooks {
                    let client = client.clone();
                    let event = event.clone();
                    let url = webhook_url.clone();
                    let url_str = url.to_string();

                    join_set.spawn(async move {
                        match client.post(url).json(&event).send().await {
                            Ok(r) => {
                                trace!("Published event to {} with status {}", url_str, r.status())
                            }
                            Err(e) => {
                                error!("Error publishing event to {}, {:?}", url_str, e)
                            }
                        }
                    });
                }

                join_set.join_all().await;
            };
        }
    }
}
