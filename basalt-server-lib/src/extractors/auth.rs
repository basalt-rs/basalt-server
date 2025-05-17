use std::sync::Arc;

use crate::{
    repositories::{
        self,
        session::SessionId,
        users::{Role, User},
    },
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

async fn extract(
    parts: &mut Parts,
    state: &Arc<AppState>,
) -> Result<Option<UserWithSession>, AuthError> {
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

    state.team_manager.check_in(&user.id);

    Ok(Some(UserWithSession(user, session_id.to_string().into())))
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Deserialize, Serialize)]
pub struct UserWithSession(pub User, pub SessionId);

impl FromRequestParts<Arc<AppState>> for UserWithSession {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        extract(parts, state).await?.ok_or(AuthError::Forbidden)
    }
}

impl From<UserWithSession> for User {
    fn from(value: UserWithSession) -> Self {
        value.0
    }
}

impl FromRequestParts<Arc<AppState>> for User {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        extract(parts, state)
            .await?
            .map(|UserWithSession(user, _)| user)
            .ok_or(AuthError::Forbidden)
    }
}

#[derive(Debug, derive_more::From, derive_more::Deref)]
#[repr(transparent)]
pub struct OptionalUser(pub Option<User>);

impl FromRequestParts<Arc<AppState>> for OptionalUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        extract(parts, state)
            .await
            .map(|x| x.map(Into::into).into())
    }
}

#[derive(Debug, derive_more::From, derive_more::Deref)]
#[repr(transparent)]
pub struct HostUser(pub User);

impl FromRequestParts<Arc<AppState>> for HostUser {
    /// If the extractor fails it'll use this "rejection" type. A rejection is a kind of error that
    /// can be converted into a response.
    type Rejection = AuthError;

    /// Perform the extraction.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let auth_user = User::from_request_parts(parts, state).await?;
        if auth_user.role == Role::Host {
            Ok(auth_user.into())
        } else {
            Err(AuthError::Forbidden)
        }
    }
}
