use bedrock::Config;
use chrono::Utc;
use dashmap::DashMap;
use serde::Serialize;

#[derive(Default, Copy, Clone, Serialize, utoipa::ToSchema)]
pub struct TeamInfo {
    /// When the team last contacted the server
    pub last_seen: Option<chrono::DateTime<Utc>>,
    /// Whether or not the team has checked into the competition by logging in
    pub checked_in: bool,
}

impl TeamInfo {
    fn check(&mut self) {
        self.checked_in = true;
        self.last_seen = Some(Utc::now());
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

    pub fn check_in(self, name: String) {
        self.teams.entry(name).and_modify(|t| t.check());
    }

    pub fn list(&self) -> Vec<TeamInfo> {
        self.teams
            .clone()
            .into_read_only()
            .values()
            .copied()
            .collect::<Vec<TeamInfo>>()
    }
}
