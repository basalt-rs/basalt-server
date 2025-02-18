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
use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::{Deserialize, Serialize};
use tokio::sync::{OnceCell, SetError};

#[derive(Clone, derive_more::Debug)]
struct Keys {
    #[debug(skip)]
    encoding: EncodingKey,
    #[debug(skip)]
    decoding: DecodingKey,
}

impl Keys {
    fn new(secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        }
    }
}

static KEYS: OnceCell<Keys> = OnceCell::const_new();

pub(crate) fn init_keys(name: impl AsRef<[u8]>) -> Result<(), SetError<()>> {
    KEYS.set({
        if let Ok(secret) = std::env::var("JWT_SECRET") {
            Keys::new(secret.as_bytes())
        } else {
            tracing::warn!("JWT_SECRET not set! Using name of competition.");
            Keys::new(name.as_ref())
        }
    })
    .map_err(|e| match e {
        // We want to erase the type so we're not returning the Keys
        SetError::AlreadyInitializedError(_) => SetError::AlreadyInitializedError(()),
        SetError::InitializingError(_) => SetError::InitializingError(()),
    })
}

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
pub struct JWTUser {
    pub user: User,
    pub session_id: String,
    pub exp: u64,
}

async fn extract(parts: &mut Parts, state: &Arc<AppState>) -> Result<Option<JWTUser>, AuthError> {
    // Extract the token from the authorization header
    let Ok(TypedHeader(Authorization(bearer))) =
        parts.extract::<TypedHeader<Authorization<Bearer>>>().await
    else {
        return Ok(None);
    };

    let user = jsonwebtoken::decode::<JWTUser>(
        bearer.token(),
        &KEYS.get().expect("set at startup").decoding,
        &jsonwebtoken::Validation::default(),
    )
    .map_err(|_| AuthError::InvalidToken)?;

    // confirm user is in db and the session is active
    let db = state.db.read().await;
    let _ = repositories::users::get_user_from_session(&db, &user.claims.session_id)
        .await
        .map_err(|_| AuthError::ExpiredToken)?;

    Ok(Some(user.claims))
}

impl FromRequestParts<Arc<AppState>> for JWTUser {
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
pub struct OptionalJWTUser(pub Option<JWTUser>);

impl FromRequestParts<Arc<AppState>> for OptionalJWTUser {
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

pub fn create_jwt(user: &JWTUser) -> String {
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        user,
        &KEYS.get().expect("set at startup").encoding,
    )
    .unwrap()
}
