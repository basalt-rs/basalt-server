use anyhow::Context;
use chrono::{DateTime, Utc};
use paste::paste;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error};

use crate::repositories::users::Username;

use crate::server::AppState;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ServerEvent {
    OnComplete {
        name: Username,
        time: DateTime<Utc>,
    },
    OnPause {
        paused_by: Username,
        time: DateTime<Utc>,
    },
    OnUnpause {
        unpaused_by: Username,
        time: DateTime<Utc>,
    },
    OnTestEvaluation {
        name: Username,
        question_idx: u32,
        question_text: String,
        passed: u16,
        failed: u16,
        points: f32,
        time: DateTime<Utc>,
    },
    OnSubmissionEvaluation {
        name: Username,
        question_idx: u32,
        question_text: String,
        passed: u16,
        failed: u16,
        points: f32,
        time: DateTime<Utc>,
    },
    OnTeamKick {
        team_kicked: Username,
        kicked_by: Username,
        time: DateTime<Utc>,
    },
    OnTeamBan {
        team_banned: Username,
        banned_by: Username,
        time: DateTime<Utc>,
    },
    OnAnnouncement {
        announcer: Username,
        announcement: String,
        time: DateTime<Utc>,
    },
    OnCheckIn {
        name: Username,
        time: DateTime<Utc>,
    },
}

impl ServerEvent {
    pub async fn handle(self, state: Arc<AppState>) -> anyhow::Result<()> {
        macro_rules! match_path {
            ($($ident: ident),+$(,)?) => {
                paste! {
                    match self {
                        $(ServerEvent::$ident { .. } => state
                            .config
                            .events
                            .[< $ident:snake >]
                            .0.iter().map(|e| e.file.clone()).collect::<Vec<PathBuf>>(),)+
                    }
                }
            }
        }
        let paths = match_path!(
            OnComplete,
            OnPause,
            OnUnpause,
            OnTestEvaluation,
            OnSubmissionEvaluation,
            OnTeamKick,
            OnTeamBan,
            OnAnnouncement,
            OnCheckIn,
        );
        let event = self.clone();
        paths
            .into_iter()
            .map(|p| tokio::task::block_in_place(|| super::deno::evaluate(event.clone(), p)))
            .collect::<anyhow::Result<Vec<()>>>()?;

        Ok(())
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
