use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::repositories::users::Username;
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
