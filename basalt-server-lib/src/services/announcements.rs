use crate::{
    extractors::auth::HostUser,
    repositories::{
        self,
        announcements::{Announcement, AnnouncementId},
    },
    server::{hooks::events::ServerEvent, AppState},
    utils,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/", tag = "announcements",
    responses(
        (status = OK, body = Vec<Announcement>, content_type = "application/json")
    )
)]
pub async fn get_all(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Announcement>>, StatusCode> {
    let sql = state.db.read().await;
    match crate::repositories::announcements::get_announcements(&sql.db).await {
        Ok(a) => Ok(Json(a)),
        Err(err) => {
            tracing::error!("Error getting announcements: {:?}", err);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct NewAnnouncement {
    message: String,
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/", tag = "announcements",
    request_body = NewAnnouncement,
    responses(
        (status=201, body=Announcement, content_type="application/json"),
        (status=401, description="User may not create announcements"),
    )
)]
pub async fn new(
    State(state): State<Arc<AppState>>,
    HostUser(user): HostUser,
    Json(NewAnnouncement { message }): Json<NewAnnouncement>,
) -> Result<Json<Announcement>, StatusCode> {
    let sql = state.db.read().await;

    let new = repositories::announcements::create_announcement(&sql.db, &user.id, &message).await;
    drop(sql);
    match new {
        Ok(new) => {
            state
                .websocket
                .broadcast(super::ws::WebSocketSend::Broadcast {
                    broadcast: super::ws::Broadcast::NewAnnouncement(new.clone()),
                });
            if let Err(err) = (ServerEvent::OnAnnouncement {
                announcer: user.id.clone(),
                announcement: message,
                time: utils::utc_now(),
            }
            .dispatch(state.clone()))
            {
                tracing::error!("Error dispatching announcement event: {:?}", err);
            }
            Ok(Json(new))
        }
        Err(err) => {
            tracing::error!("Error getting announcements: {:?}", err);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}", tag = "announcements",
    responses(
        (status=OK, body=Announcement, content_type="application/json"),
        (status=404, description="Announcement with provided id does not exists"),
        (status=401, description="User may not delete announcements"),
    )
)]
pub async fn delete(
    State(state): State<Arc<AppState>>,
    HostUser(_u): HostUser,
    Path((id,)): Path<(AnnouncementId,)>,
) -> Result<Json<Announcement>, StatusCode> {
    let sql = state.db.read().await;

    let del = repositories::announcements::delete_announcement(&sql.db, &id).await;
    drop(sql);
    match del {
        Ok(Some(del)) => {
            state
                .websocket
                .broadcast(super::ws::WebSocketSend::Broadcast {
                    broadcast: super::ws::Broadcast::DeleteAnnouncement { id },
                });
            Ok(Json(del))
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(err) => {
            tracing::error!("Error getting announcements: {:?}", err);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(get_all, new, delete))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
