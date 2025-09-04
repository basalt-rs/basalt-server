use std::{net::SocketAddr, time::Duration};

use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot};

use crate::{define_id_type, repositories::users::UserId, services::ws::WebSocketSend};

define_id_type!(LeaderboardId);

#[derive(Clone, Copy, Eq, PartialEq, Hash, derive_more::Debug)]
pub enum ConnectionKind {
    User {
        user: UserId,
    },
    Leaderboard {
        id: LeaderboardId,
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

    pub fn user(&self) -> Option<&UserId> {
        match self {
            ConnectionKind::User { user } => Some(user),
            ConnectionKind::Leaderboard { .. } => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectedClient {
    send: mpsc::UnboundedSender<WebSocketSend>,
}

impl ConnectedClient {
    pub fn send(
        &self,
        message: WebSocketSend,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<WebSocketSend>> {
        self.send.send(message)
    }
}

#[derive(Debug, Default)]
pub struct WebSocketManager {
    active_connections: DashMap<ConnectionKind, ConnectedClient>,
    waiting_connections: DashMap<UserId, Vec<oneshot::Sender<ConnectedClient>>>,
}

impl WebSocketManager {
    pub fn broadcast(&self, broadcast: WebSocketSend) {
        self.active_connections.retain(|key, conn| {
            match conn.send(broadcast.clone()) {
                Ok(()) => true,
                Err(_) => {
                    tracing::warn!(?key, "Socket discovered to be closed when sending broadcast. Removing from active connections...");
                    false
                }
            }
        });
    }

    pub fn remove_connection(&self, who: &'_ ConnectionKind) {
        self.active_connections.remove(who);
    }

    pub fn add_connection(&self, who: ConnectionKind) -> mpsc::UnboundedReceiver<WebSocketSend> {
        let (tx, rx) = mpsc::unbounded_channel();
        let connected = ConnectedClient { send: tx };
        // If this is a user, alert anybody waiting
        if let ConnectionKind::User { ref user } = who {
            if let Some((_, senders)) = self.waiting_connections.remove(user) {
                for sender in senders {
                    let _ = sender.send(connected.clone());
                }
            }
        }
        self.active_connections.insert(who, connected);
        rx
    }

    /// Wait to for a websocket connection to occur, with a timeout.  If the websocket does not
    /// occur before `timeout` has elapsed, this function returns `None`.
    pub async fn wait_for_connection(
        &self,
        user: UserId,
        timeout: Duration,
    ) -> Option<ConnectedClient> {
        if let Some(conn) = self.active_connections.get(&ConnectionKind::User { user }) {
            return Some(conn.clone());
        }
        let (tx, rx) = oneshot::channel();
        self.waiting_connections.entry(user).or_default().push(tx);
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(v)) => Some(v.clone()),
            Ok(Err(_)) => None,
            Err(_) => None,
        }
    }

    pub fn get_sender(&self, who: &'_ ConnectionKind) -> Option<ConnectedClient> {
        self.active_connections.get(who).as_deref().cloned()
    }
}
