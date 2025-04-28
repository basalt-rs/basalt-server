use bedrock::Config;
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::repositories::users::Username;

#[derive(Debug, PartialEq, Eq, Default, Copy, Clone, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamInfo {
    /// When the team last contacted the server
    pub last_seen: Option<chrono::DateTime<Utc>>,
    /// Whether or not the team has checked into the competition by logging in
    pub checked_in: bool,
    /// Just a flag stating whether or not the team has deliberately disconnected
    pub disconnected: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamFull {
    /// Username of team/player
    pub team: Username,
    /// Contains full information about team
    pub info: TeamInfo,
}

impl TeamInfo {
    fn check(&mut self) {
        self.checked_in = true;
        self.last_seen = Some(Utc::now());
        self.disconnected = false;
    }
    fn disconnect(&mut self) {
        self.disconnected = true;
    }
}

pub struct TeamManagement {
    teams: DashMap<String, TeamInfo>,
}

impl TeamManagement {
    pub fn from_config(cfg: &Config) -> Self {
        let teams: DashMap<String, TeamInfo> = DashMap::new();
        for t in &cfg.accounts.competitors {
            teams.insert(t.name.clone(), TeamInfo::default());
        }
        TeamManagement { teams }
    }

    pub fn check_in(&self, name: String) {
        self.teams.entry(name).and_modify(|t| t.check());
    }

    pub fn disconnect(&self, name: String) {
        self.teams.entry(name).and_modify(|t| t.disconnect());
    }

    pub fn list(&self) -> Vec<TeamFull> {
        self.teams
            .clone()
            .into_read_only()
            .iter()
            .map(|(k, v)| TeamFull {
                team: k.clone().into(),
                info: *v,
            })
            .collect::<Vec<TeamFull>>()
    }

    pub fn get_team(&self, team: String) -> Option<TeamFull> {
        self.teams.get(&team).map(|t| *t).map(|t| TeamFull {
            team: team.into(),
            info: t,
        })
    }

    pub fn get_all(&self) -> DashMap<String, TeamInfo> {
        self.teams.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TEST_TEAM_1: &str = "team1";
    #[test]
    fn check_works() {
        let teams = DashMap::new();
        teams.insert(
            TEST_TEAM_1.into(),
            TeamInfo {
                last_seen: None,
                checked_in: false,
                disconnected: false,
            },
        );

        let manager = TeamManagement { teams };
        let team = manager.get_team(TEST_TEAM_1.into()).unwrap();
        assert!(!team.info.checked_in);
        assert_eq!(team.info.disconnected, false);
        assert!(team.info.last_seen.is_none());

        manager.check_in(TEST_TEAM_1.into());

        let team = manager.get_team(TEST_TEAM_1.into()).unwrap();
        let team_name: String = team.team.clone().into();
        assert_eq!(TEST_TEAM_1.to_owned(), team_name);
        assert!(team.info.checked_in);
        assert_eq!(team.info.disconnected, false);
        assert!(team.info.last_seen.is_some());
    }

    #[test]
    fn disconnect_works() {
        let teams = DashMap::new();
        teams.insert(
            TEST_TEAM_1.into(),
            TeamInfo {
                last_seen: None,
                checked_in: false,
                disconnected: false,
            },
        );

        let manager = TeamManagement { teams };
        let team = manager.get_team(TEST_TEAM_1.into()).unwrap();
        assert!(!team.info.checked_in);
        assert!(!team.info.disconnected);
        assert!(team.info.last_seen.is_none());

        manager.disconnect(TEST_TEAM_1.into());

        let team = manager.get_team(TEST_TEAM_1.into()).unwrap();
        let team_name: String = team.team.clone().into();
        assert_eq!(TEST_TEAM_1.to_owned(), team_name);
        assert!(!team.info.checked_in);
        assert!(team.info.disconnected);
        assert!(team.info.last_seen.is_none());
    }
}
