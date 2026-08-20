//! Device-session handlers for `/api/v1`.

use ankh_db::Error as DbError;
use ankh_types::{DeviceAuthorizationRequest, DeviceTokenRequest};
use axum::{
    Extension, Json,
    body::Body,
    extract::{OriginalUri, Path, Query},
    http::{
        HeaderMap, StatusCode, Uri,
        header::{CACHE_CONTROL, LOCATION},
    },
    response::{IntoResponse, Response},
};

use crate::{
    AnkhWebState, RequireActiveUser,
    api::{ApiError, ApiResult},
    auth::current_session_id,
    errors,
    services::device_sessions,
};

/// List active device sessions for the current user.
pub async fn list_device_sessions(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
) -> ApiResult<Json<Vec<ankh_types::DeviceSessionInfo>>> {
    Ok(Json(
        device_sessions::list_device_sessions(&state, &session).await?,
    ))
}

/// Mint a browser device session for the current user.
pub async fn create_device_session(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
) -> ApiResult<Json<ankh_types::CreateDeviceSessionResponse>> {
    Ok(Json(
        device_sessions::create_browser_device_session(&state, &session).await?,
    ))
}

/// Revoke a device session by ID.
pub async fn revoke_device_session(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
    Path(id): Path<String>,
) -> ApiResult<Json<()>> {
    device_sessions::revoke_device_session(&state, &session, id).await?;
    Ok(Json(()))
}

/// Browser entrypoint for device PKCE authorization.
pub async fn authorize(
    Extension(state): Extension<AnkhWebState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Query(request): Query<DeviceAuthorizationRequest>,
) -> Response {
    let Some(session_id) = current_session_id(&headers, &state.config().cookie) else {
        return login_redirect_response(state.config().device_auth.login_path.as_str(), &uri);
    };

    let mut db = match state.db_pool().get().await {
        Ok(db) => db,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let session = match db
        .touch_session_if_stale(
            session_id.as_str(),
            ankh_constants::SESSION_TOUCH_STALE_AFTER,
        )
        .await
    {
        Ok(session) => session,
        Err(DbError::SessionMissing(_)) => {
            return login_redirect_response(state.config().device_auth.login_path.as_str(), &uri);
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    match db.is_user_waitlisted(session.email.as_str()).await {
        Ok(false) => {}
        Ok(true) => return ApiError::forbidden(errors::WAITLISTED).into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
    drop(db);

    match device_sessions::authorize_device(&state, &session, request).await {
        Ok(response) => redirect_response(response.callback_url.as_str()),
        Err(error) => error.into_response(),
    }
}

/// Exchange a device authorization code for a bearer token.
pub async fn token(
    Extension(state): Extension<AnkhWebState>,
    headers: HeaderMap,
    Json(request): Json<DeviceTokenRequest>,
) -> ApiResult<Json<ankh_types::DeviceTokenResponse>> {
    Ok(Json(
        device_sessions::exchange_device_token(&state, &headers, request).await?,
    ))
}

/// Build an HTTP redirect response with cache disabled.
fn redirect_response(location: &str) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::FOUND;
    response
        .headers_mut()
        .insert(LOCATION, location.parse().expect("valid redirect location"));
    response
        .headers_mut()
        .insert(CACHE_CONTROL, "no-store".parse().expect("cache header"));
    response
}

/// Build the login redirect target that preserves the original authorize request.
fn login_redirect_response(login_path: &str, uri: &Uri) -> Response {
    let redirect = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("redirect", uri.to_string().as_str())
        .finish();
    redirect_response(format!("{login_path}?{redirect}").as_str())
}
