use std::net::SocketAddr;

use dashmap::DashMap;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::{repositories::users::User, services::ws::WebSocketSend};

#[derive(Clone, Eq, PartialEq, Hash, derive_more::Debug)]
pub enum ConnectionKind {
    User {
        #[debug("{:?}", user.username)]
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
        self.active_connections.retain(|key, conn| {
            match conn.send.send(broadcast.clone()) {
                Ok(()) => true,
                Err(_) => {
                    tracing::warn!(?key, "Socket discovered to be closed when sending broadcast. Removing from active connections...");
                    false
                }
            }
        });
    }

    pub fn get_sender(&self, who: &'_ ConnectionKind) -> Option<UnboundedSender<WebSocketSend>> {
        self.active_connections.get(who).map(|x| x.send.clone())
    }
}
