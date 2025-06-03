use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error};

use crate::repositories::users::Username;
use crate::server::AppState;
use crate::services::ws::TestResults;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind")]
pub enum ServerEvent {
    // Unused
    #[serde(rename_all = "camelCase")]
    OnComplete { name: Username, time: DateTime<Utc> },
    #[serde(rename_all = "camelCase")]
    OnPause {
        paused_by: Username,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    OnUnpause {
        unpaused_by: Username,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    OnTestEvaluation {
        name: Username,
        question_idx: u32,
        question_text: String,
        test_results: TestResults,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    OnSubmissionEvaluation {
        name: Username,
        question_idx: u32,
        question_text: String,
        test_results: TestResults,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    // Unused
    OnTeamKick {
        team_kicked: Username,
        kicked_by: Username,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    // Unused
    OnTeamBan {
        team_banned: Username,
        banned_by: Username,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    OnAnnouncement {
        announcer: Username,
        announcement: String,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    OnCheckIn { name: Username, time: DateTime<Utc> },
}

impl ServerEvent {
    pub async fn handle(self, state: Arc<AppState>) -> anyhow::Result<()> {
        state
            .config
            .integrations
            .events
            .iter()
            .map(|p| tokio::task::block_in_place(|| super::deno::evaluate(self.clone(), p)))
            .collect::<anyhow::Result<Vec<()>>>()?;

        Ok(())
    }

    pub fn get_fn_name(&self) -> &'static str {
        match self {
            ServerEvent::OnComplete { .. } => "onComplete",
            ServerEvent::OnPause { .. } => "onPause",
            ServerEvent::OnUnpause { .. } => "onUnpause",
            ServerEvent::OnTestEvaluation { .. } => "onTestEvaluation",
            ServerEvent::OnSubmissionEvaluation { .. } => "onSubmissionEvaluation",
            ServerEvent::OnTeamKick { .. } => "onTeamKick",
            ServerEvent::OnTeamBan { .. } => "onTeamBan",
            ServerEvent::OnAnnouncement { .. } => "onAnnouncement",
            ServerEvent::OnCheckIn { .. } => "onCheckIn",
        }
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

    /// Begin handling events sent over the channel
    ///
    /// Each event is handled in a separate thread. Panics
    /// are recovered from gracefully.
    pub async fn start(&mut self, state: Arc<AppState>) {
        loop {
            if let Some(event) = self.rx.recv().await {
                debug!("received event");
                tokio::spawn({
                    let state = state.clone();
                    async move {
                        if let Err(err) = event.handle(state.clone()).await {
                            error!("error handling event: {:?}", err);
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
