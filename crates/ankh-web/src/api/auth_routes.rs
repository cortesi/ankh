//! Auth handlers for `/api/v1`.

use ankh_types::{
    AuthMeResponse, ForgotPasswordRequest, LoginRequest, SignupRequest, ValidateResetTokenRequest,
    VerificationRequest,
};
use axum::{Extension, Json, http::HeaderMap, response::IntoResponse};
use serde::Deserialize;

use crate::{
    AnkhWebState, AuthSession, RequireActiveUser,
    api::ApiResult,
    auth::{
        SET_COOKIE_HEADER, current_session_id, session_clear_cookie_header,
        session_set_cookie_header,
    },
    services::auth,
};

/// Password reset completion payload.
#[derive(Deserialize)]
pub struct ResetPasswordRequest {
    /// One-time password reset token.
    pub token: String,
    /// Replacement plaintext password.
    pub new_password: String,
}

/// Create a session-backed account and return the current user.
pub async fn signup(
    Extension(state): Extension<AnkhWebState>,
    Json(request): Json<SignupRequest>,
) -> ApiResult<impl IntoResponse> {
    let auth = auth::signup(
        &state,
        request.username,
        request.email,
        request.password,
        request.invite_token,
        request.org_invite_token,
    )
    .await?;

    Ok((
        [(
            SET_COOKIE_HEADER,
            session_set_cookie_header(auth.session_id.as_str(), &state.config().cookie),
        )],
        Json(auth.user),
    ))
}

/// Authenticate a user and set the session cookie.
pub async fn login(
    Extension(state): Extension<AnkhWebState>,
    Json(request): Json<LoginRequest>,
) -> ApiResult<impl IntoResponse> {
    let auth = auth::login(&state, request.email, request.password).await?;
    Ok((
        [(
            SET_COOKIE_HEADER,
            session_set_cookie_header(auth.session_id.as_str(), &state.config().cookie),
        )],
        Json(auth.user),
    ))
}

/// Clear the current session cookie.
pub async fn logout(
    Extension(state): Extension<AnkhWebState>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    auth::logout(&state, current_session_id(&headers, &state.config().cookie)).await?;
    Ok((
        [(
            SET_COOKIE_HEADER,
            session_clear_cookie_header(&state.config().cookie),
        )],
        Json(()),
    ))
}

/// Return the current authenticated user, if any.
pub async fn me(
    Extension(state): Extension<AnkhWebState>,
    AuthSession(session): AuthSession,
) -> ApiResult<Json<AuthMeResponse>> {
    let user = auth::get_current_user(&state, session).await?;
    Ok(Json(AuthMeResponse { user }))
}

/// Report whether waitlist mode is enabled.
pub async fn waitlist_status(Extension(state): Extension<AnkhWebState>) -> ApiResult<Json<bool>> {
    Ok(Json(auth::waitlist_status(&state).await?))
}

/// Consume an email verification token.
pub async fn verify_email(
    Extension(state): Extension<AnkhWebState>,
    Json(request): Json<VerificationRequest>,
) -> ApiResult<Json<()>> {
    auth::verify_email(&state, request.token).await?;
    Ok(Json(()))
}

/// Resend the verification email for the current session.
pub async fn resend_verification(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
) -> ApiResult<Json<()>> {
    auth::resend_verification(&state, &session).await?;
    Ok(Json(()))
}

/// Request a password reset email.
pub async fn forgot_password(
    Extension(state): Extension<AnkhWebState>,
    Json(request): Json<ForgotPasswordRequest>,
) -> ApiResult<Json<()>> {
    auth::request_password_reset(&state, request.email).await?;
    Ok(Json(()))
}

/// Check whether a password reset token is still valid.
pub async fn validate_reset_token(
    Extension(state): Extension<AnkhWebState>,
    Json(request): Json<ValidateResetTokenRequest>,
) -> ApiResult<Json<bool>> {
    Ok(Json(
        auth::validate_reset_token(&state, request.token).await?,
    ))
}

/// Reset the password for the supplied token and clear any active session.
pub async fn reset_password(
    Extension(state): Extension<AnkhWebState>,
    Json(request): Json<ResetPasswordRequest>,
) -> ApiResult<impl IntoResponse> {
    auth::reset_password(&state, request.token, request.new_password).await?;
    Ok((
        [(
            SET_COOKIE_HEADER,
            session_clear_cookie_header(&state.config().cookie),
        )],
        Json(()),
    ))
}
