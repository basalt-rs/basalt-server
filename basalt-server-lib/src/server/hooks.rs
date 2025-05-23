use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::error;

use axum::Router;

use tower::{Layer, ServiceBuilder};
use tracing::debug;

use crate::repositories::users::Username;

use super::AppState;

pub enum ServerEvent {
    CheckIn { name: Username, time: DateTime<Utc> },
}

impl ServerEvent {
    pub async fn handle(self, _: Arc<AppState>) -> anyhow::Result<()> {
        todo!()
    }
}

pub struct EventHookHandler {
    rx: mpsc::UnboundedReceiver<ServerEvent>,
}

impl EventHookHandler {
    pub fn create() -> (Self, EventDispatcherService) {
        // create message queue
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ServerEvent>();

        (Self { rx }, EventDispatcherService::new(tx))
    }

    pub async fn start(&mut self, state: Arc<AppState>) {
        loop {
            if let Some(event) = self.rx.recv().await {
                debug!("received event");
                tokio::spawn({
                    let state = state.clone();
                    async move {
                        if let Err(err) = event.handle(state.clone()).await {
                            error!("error handling event: {}", err);
                        };
                    }
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
