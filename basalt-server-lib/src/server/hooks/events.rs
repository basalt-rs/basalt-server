use std::sync::Arc;

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Serialize;
use tracing::info;

use crate::repositories::users::UserId;
use crate::server::AppState;
use crate::services::ws::TestResults;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind")]
pub enum ServerEvent {
    // Unused
    #[serde(rename_all = "camelCase")]
    OnComplete { id: UserId, time: DateTime<Utc> },
    #[serde(rename_all = "camelCase")]
    OnPause {
        paused_by: UserId,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    OnUnpause {
        unpaused_by: UserId,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    OnTestEvaluation {
        id: UserId,
        question_idx: u32,
        question_text: String,
        test_results: TestResults,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    OnSubmissionEvaluation {
        id: UserId,
        question_idx: u32,
        question_text: String,
        test_results: TestResults,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    // Unused
    OnTeamKick {
        team_kicked: UserId,
        kicked_by: UserId,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    // Unused
    OnTeamBan {
        team_banned: UserId,
        banned_by: UserId,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    OnAnnouncement {
        announcer: UserId,
        announcement: String,
        time: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    OnCheckIn { id: UserId, time: DateTime<Utc> },
}

impl ServerEvent {
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

    /// Dispatch an event to all subscribers asynchronously
    pub fn dispatch(&self, state: Arc<AppState>) -> anyhow::Result<()> {
        info!("Event dispatched: {:?}", self);
        match state
            .dispatchers
            .iter()
            .map(|tx| {
                tx.send((self.clone(), state.clone()))
                    .context("Failed to emit event")
            })
            .collect::<anyhow::Result<Vec<()>>>()
        {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}
