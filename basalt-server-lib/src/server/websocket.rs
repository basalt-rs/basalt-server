use std::net::SocketAddr;

use dashmap::DashMap;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::{repositories::users::User, services::ws::WebSocketSend};

#[derive(Clone, Eq, PartialEq, Hash, derive_more::Debug)]
pub enum ConnectionKind {
    User {
        #[debug("{:?}", user.username.0)]
        user: User,
    },
    Leaderboard {
        id: String,
        #[debug(skip)]
        addr: SocketAddr,
    },
}

impl ConnectionKind {
    pub fn is_user(&self) -> bool {
        match self {
            ConnectionKind::User { .. } => true,
            ConnectionKind::Leaderboard { .. } => false,
        }
    }

    pub fn user(&self) -> Option<&User> {
        match self {
            ConnectionKind::User { user } => Some(user),
            ConnectionKind::Leaderboard { .. } => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectedClient {
    pub send: mpsc::UnboundedSender<WebSocketSend>,
}

#[derive(Debug, Default)]
pub struct WebSocketManager {
    pub active_connections: DashMap<ConnectionKind, ConnectedClient>,
}

impl WebSocketManager {
    pub fn broadcast(&self, broadcast: WebSocketSend) {
        let mut to_remove = Vec::new();
        for conn in &self.active_connections {
            if conn.send.send(broadcast.clone()).is_err() {
                // This _shouldn't_ happen, but it _could_
                tracing::warn!(key = ?conn.key(), "Socket discovered to be closed when sending broadcast. Removing from active connections...");
                to_remove.push(conn.key().clone());
            }
        }
        to_remove.iter().for_each(|x| {
            self.active_connections.remove(x);
        });
    }

    pub fn get_sender(&self, who: &'_ ConnectionKind) -> Option<UnboundedSender<WebSocketSend>> {
        self.active_connections.get(who).map(|x| x.send.clone())
    }
}
