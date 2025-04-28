use crate::server::AppState;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{AppendHeaders, IntoResponse},
};
use std::sync::Arc;
use tokio::sync::OnceCell;
use tracing::{debug, error};
use utoipa_axum::{router::OpenApiRouter, routes};

static PDF: OnceCell<Box<[u8]>> = OnceCell::const_new();

#[axum::debug_handler]
#[utoipa::path(get, tag = "competition", path = "/packet", responses((status = OK, body = Vec<u8>, content_type = "application/pdf")))]
pub async fn download_packet(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, StatusCode> {
    PDF.get_or_try_init(|| async {
        debug!("Rendering packet PDF");
        state.config.render_pdf(None).map(Vec::into_boxed_slice)
    })
    .await
    .map(|x| {
        (
            AppendHeaders([
                (header::CONTENT_TYPE, "application/pdf"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"competition.pdf\"",
                ),
            ]),
            x.as_ref(),
        )
    })
    .map_err(|err| {
        error!("Error while rendering packet PDF: {:?}", err);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(download_packet))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
