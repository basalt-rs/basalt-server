use std::{net::SocketAddr, sync::Arc};

use anyhow::{bail, Context};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    http::HeaderMap,
    response::Response,
};
use tracing::{debug, error, trace, warn};

use super::WebSocketRecv;
use crate::{
    extractors::auth::AuthError,
    repositories,
    server::{websocket::LeaderboardId, AppState},
    services::ws::ConnectionKind,
};

#[axum::debug_handler]
#[utoipa::path(get, path="/", tag="ws", responses((status = OK, description = "connected to websocket")))]
pub async fn connect_websocket(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, AuthError> {
    trace!("Attempting to connect to WS");
    let protocol = headers
        .get("Sec-WebSocket-Protocol")
        .map(|s| s.to_str().unwrap().to_string());
    let user = if let Some(session_id) = &protocol {
        let user = repositories::session::get_user_from_session(&state.db, session_id)
            .await
            .map_err(|_| {
                trace!("token expired");
                AuthError::ExpiredToken
            })?;
        trace!(?user, "User authed");
        Some(user)
    } else {
        trace!("user not authed");
        None
    };

    let who = match user {
        Some(user) => ConnectionKind::User { user: user.id },
        None => ConnectionKind::Leaderboard {
            id: LeaderboardId::new(),
            addr,
        },
    };

    trace!(?who, "WS client connect");
    let ws = if let Some(protocol) = protocol {
        ws.protocols([protocol])
    } else {
        ws
    };
    Ok(ws.on_upgrade(move |ws| async move {
        // Using defer here so that if the thread panics, we still remove the connection.
        scopeguard::defer! {
            state.websocket.remove_connection(&who);
        }
        if let Err(e) = handle_socket(ws, who, Arc::clone(&state)).await {
            error!(?who, ?e, "Error handling websocket connection");
        }
    }))
}

#[tracing::instrument(skip(ws, state))]
async fn handle_socket(
    mut ws: WebSocket,
    who: ConnectionKind,
    state: Arc<AppState>,
) -> anyhow::Result<()> {
    let mut rx = state.websocket.add_connection(who);

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
                    // WS is closed
                    return Ok(());
                },
                Some(Err(error)) => {
                    error!(?error, "Error while waiting for websocket message");
                    return Ok(());
                },
                Some(Ok(msg)) => {
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
