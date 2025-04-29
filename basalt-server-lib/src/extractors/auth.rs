use std::sync::Arc;

use crate::{
    repositories::{self, users::User},
    server::AppState,
};
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, Response, StatusCode},
    response::IntoResponse,
    RequestPartsExt,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use serde::{Deserialize, Serialize};
use tracing::trace;

#[derive(Debug)]
pub enum AuthError {
    ExpiredToken,
    InvalidToken,
    Forbidden,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response<axum::body::Body> {
        let (status, message) = match self {
            AuthError::ExpiredToken => (StatusCode::UNAUTHORIZED, "Expired Token"),
            AuthError::InvalidToken => (StatusCode::BAD_REQUEST, "Invalid token"),
            AuthError::Forbidden => (StatusCode::FORBIDDEN, "Forbidden"),
        };

        (status, message).into_response()
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Deserialize, Serialize)]
pub struct AuthUser {
    pub session_id: String,
    pub user: User,
}

async fn extract(parts: &mut Parts, state: &Arc<AppState>) -> Result<Option<AuthUser>, AuthError> {
    // Extract the token from the authorization header
    let Ok(TypedHeader(Authorization(bearer))) =
        parts.extract::<TypedHeader<Authorization<Bearer>>>().await
    else {
        return Ok(None);
    };

    let session_id = bearer.token();

    // confirm user is in db and the session is active
    let db = state.db.read().await;
    trace!("getting user from session");
    let user = repositories::session::get_user_from_session(&db, session_id)
        .await
        .map_err(|_| {
            trace!("token expired");
            AuthError::ExpiredToken
        })?;
    trace!(?user.username, "resolved user");

    state.team_manager.check_in(&user.username);

    drop(db);
    Ok(Some(AuthUser {
        user,
        session_id: session_id.into(),
    }))
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    /// If the extractor fails it'll use this "rejection" type. A rejection is a kind of error that
    /// can be converted into a response.
    type Rejection = AuthError;

    /// Perform the extraction.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        extract(parts, state).await?.ok_or(AuthError::Forbidden)
    }
}

#[derive(Debug, derive_more::From, derive_more::Deref)]
#[repr(transparent)]
pub struct OptionalAuthUser(pub Option<AuthUser>);

impl FromRequestParts<Arc<AppState>> for OptionalAuthUser {
    /// If the extractor fails it'll use this "rejection" type. A rejection is a kind of error that
    /// can be converted into a response.
    type Rejection = AuthError;

    /// Perform the extraction.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        extract(parts, state).await.map(Into::into)
    }
}
