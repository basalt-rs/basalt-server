use std::{net::SocketAddr, sync::Arc};

use anyhow::{bail, Context};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    response::Response,
};
use rand::Rng;
use tokio::sync::mpsc;
use tracing::{debug, error, trace, warn};

use super::{ConnectedClient, ConnectionKind, WebSocketRecv};
use crate::server::AppState;

#[axum::debug_handler]
#[utoipa::path(get, path = "/", responses((status = OK, description = "connected to websocket")))]
pub async fn handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Response {
    // TODO: This should be associated with a user once we have authentication setup.
    let who = ConnectionKind::Leaderboard {
        id: rand::thread_rng()
            .sample_iter(rand::distributions::Alphanumeric)
            .take(20)
            .map(char::from)
            .collect(),
        addr,
    };

    trace!(?who, "Client connect");
    ws.on_upgrade(move |ws| async move {
        // Using defer here so that if the thread panics, we still remove the connection.
        scopeguard::defer! {
            state.active_connections.remove(&who);
        }
        if let Err(e) = handle_socket(ws, who.clone(), Arc::clone(&state)).await {
            error!(?who, ?e, "Error handling websocket connection");
        }
    })
}

#[tracing::instrument(skip(ws, state))]
async fn handle_socket(
    mut ws: WebSocket,
    who: ConnectionKind,
    state: Arc<AppState>,
) -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    state
        .active_connections
        .insert(who.clone(), ConnectedClient { send: tx });

    if ws.send(Message::Ping("ping".into())).await.is_ok() {
        trace!("Send ping");
    } else {
        bail!("Could not send ping");
    }

    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                None => {
                    // Connection is closed.
                    trace!("Connection closed");
                    return Ok(());
                },
                Some(msg) => {
                    trace!(?msg, "Sending message on websocket");
                    ws.send(Message::text(serde_json::to_string(&msg)?)).await?;
                }
            },
            msg = ws.recv() => match msg {
                None => {
                    return Ok(());
                },
                Some(Err(error)) => {
                    debug!(?error, "Error while waiting for websocket message");
                    return Ok(());
                },
                Some(Ok(msg)) => {
                    trace!(?msg, "recv msg");
                    handle_message(msg, &mut ws, &who, Arc::clone(&state)).await?;
                }
            }
        }
    }
}

async fn handle_message(
    msg: Message,
    ws: &mut WebSocket,
    who: &ConnectionKind,
    state: Arc<AppState>,
) -> anyhow::Result<()> {
    match msg {
        Message::Text(bytes) => match serde_json::from_str::<WebSocketRecv>(bytes.as_str()) {
            Ok(msg) => {
                trace!(?msg, "Receiving websocket message");
                msg.handle(who, state)
                    .await
                    .context("handling websocket message")?;
            }
            Err(error) => {
                debug!(?error, "Ignoring invalid websocket message");
            }
        },
        Message::Binary(_) => {
            warn!("Ignoring unexpected binary message");
        }
        Message::Ping(bytes) => {
            ws.send(Message::Pong(bytes)).await?;
        }
        Message::Pong(_) => {}
        Message::Close(_) => {
            trace!("Close message received");
        }
    }
    Ok(())
}
