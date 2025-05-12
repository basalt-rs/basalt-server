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
use rand::Rng;
use tokio::sync::mpsc;
use tracing::{debug, error, trace, warn};

use super::WebSocketRecv;
use crate::{
    extractors::auth::{AuthError, AuthUser},
    repositories,
    server::{websocket::ConnectedClient, AppState},
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
    let db = state.db.read().await;
    trace!("getting user from session");
    let user = if let Some(header) = headers.get("Sec-WebSocket-Protocol") {
        let session_id = header.to_str().unwrap();
        let user = repositories::session::get_user_from_session(&db, session_id)
            .await
            .map_err(|_| {
                trace!("token expired");
                AuthError::ExpiredToken
            })?;
        Some(AuthUser {
            user,
            session_id: session_id.to_string(),
        })
    } else {
        None
    };
    drop(db);
    let who = match user {
        Some(user) => ConnectionKind::User { user },
        None => ConnectionKind::Leaderboard {
            id: rand::thread_rng()
                .sample_iter(rand::distributions::Alphanumeric)
                .take(20)
                .map(char::from)
                .collect(),
            addr,
        },
    };

    trace!(?who, "Client connect");
    Ok(ws.on_upgrade(move |ws| async move {
        // Using defer here so that if the thread panics, we still remove the connection.
        scopeguard::defer! {
            state.websocket.active_connections.remove(&who);
        }
        if let Err(e) = handle_socket(ws, who.clone(), Arc::clone(&state)).await {
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
    let (tx, mut rx) = mpsc::unbounded_channel();
    state
        .websocket
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
