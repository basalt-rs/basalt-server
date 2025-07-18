use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::repositories::users::UserId;

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
    pub id: UserId,
    /// Contains full information about team
    #[serde(flatten)]
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

#[derive(Clone, Debug, PartialEq, Deserialize, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamWithScore {
    pub score: f64,
    pub id: UserId,
    pub name: String,
    pub display_name: Option<String>,
    #[serde(flatten)]
    pub team_info: TeamFull,
}

#[derive(Default)]
pub struct TeamManagement {
    teams: DashMap<UserId, TeamInfo>,
}

impl TeamManagement {
    pub fn insert(&self, id: UserId) {
        self.teams.insert(id, TeamInfo::default());
    }

    pub fn insert_many(&self, ids: impl IntoIterator<Item = UserId>) {
        ids.into_iter().for_each(|id| self.insert(id));
    }

    pub fn check_in(&self, id: &UserId) -> bool {
        let mut effective = false;
        if let Some(mut t) = self.teams.get_mut(id) {
            effective = !t.checked_in;
            t.check();
        }
        effective
    }

    pub fn disconnect(&self, id: &UserId) {
        if let Some(mut t) = self.teams.get_mut(id) {
            t.disconnect();
        }
    }

    pub fn list(&self) -> impl Iterator<Item = TeamFull> {
        self.teams
            .clone()
            .into_iter()
            .map(|(k, v)| TeamFull { id: k, info: v })
    }

    pub fn get_team(&self, id: &UserId) -> Option<TeamFull> {
        self.teams.get(id).map(|t| TeamFull {
            id: id.clone(),
            info: *t,
        })
    }

    pub fn get_all(&self) -> DashMap<UserId, TeamInfo> {
        self.teams.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn check_works() {
        let team1 = UserId::new();
        let teams = DashMap::new();
        teams.insert(
            team1.clone(),
            TeamInfo {
                last_seen: None,
                checked_in: false,
                disconnected: false,
            },
        );

        let manager = TeamManagement { teams };
        let team = manager.get_team(&team1).unwrap();
        assert!(!team.info.checked_in);
        assert!(!team.info.disconnected);
        assert!(team.info.last_seen.is_none());

        let result = manager.check_in(&team1);
        assert!(result);

        let team = manager.get_team(&team1).unwrap();
        let team_name = team.id.clone();
        assert_eq!(team1, team_name);
        assert!(team.info.checked_in);
        assert!(!team.info.disconnected);
        assert!(team.info.last_seen.is_some());

        let result = manager.check_in(&team1);
        assert!(!result);
    }

    #[test]
    fn disconnect_works() {
        let team1 = UserId::new();
        let teams = DashMap::new();
        teams.insert(
            team1.clone(),
            TeamInfo {
                last_seen: None,
                checked_in: false,
                disconnected: false,
            },
        );

        let manager = TeamManagement { teams };
        let team = manager.get_team(&team1).unwrap();
        assert!(!team.info.checked_in);
        assert!(!team.info.disconnected);
        assert!(team.info.last_seen.is_none());

        manager.disconnect(&team1);

        let team = manager.get_team(&team1).unwrap();
        let team_name = team.id.clone();
        assert_eq!(team1, team_name);
        assert!(!team.info.checked_in);
        assert!(team.info.disconnected);
        assert!(team.info.last_seen.is_none());
    }

    #[test]
    fn get_team_works() {
        let team1 = UserId::new();
        let team2 = UserId::new();
        let teams = DashMap::new();
        teams.insert(
            team1.clone(),
            TeamInfo {
                last_seen: None,
                checked_in: false,
                disconnected: false,
            },
        );
        teams.insert(
            team2.clone(),
            TeamInfo {
                last_seen: None,
                checked_in: true,
                disconnected: true,
            },
        );

        let manager = TeamManagement { teams };
        let team1 = manager.get_team(&team1).unwrap();
        let team2 = manager.get_team(&team2).unwrap();
        let team3 = manager.get_team(&UserId::new());
        assert!(team3.is_none());
        assert!(!team1.info.checked_in);
        assert!(!team1.info.disconnected);
        assert!(team1.info.last_seen.is_none());
        assert!(team2.info.checked_in);
        assert!(team2.info.disconnected);
        assert!(team2.info.last_seen.is_none());
        assert_ne!(team1.id, team2.id);
    }
}
