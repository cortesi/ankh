//! Authentication services.

use std::collections::HashMap;

use ankh_constants::{
    DEFAULT_SESSION_TTL, EMAIL_VERIFICATION_TTL, MIN_PASSWORD_LEN, PASSWORD_RESET_TTL,
    VERIFICATION_RESEND_COOLDOWN,
};
use ankh_db::{Error as DbError, OrgInvite, Session, TokenKind};
use ankh_mail::template;
use ankh_types::UserInfo;

use crate::{
    api::{ApiError, ApiResult},
    auth::{
        bad_request, enforce_login_rate_limit, enforce_password_reset_rate_limit,
        enforce_signup_rate_limit, now_epoch_secs, too_many_requests,
    },
    errors,
    hooks::DeviceSessionsRevoked,
    state::AnkhWebState,
};

/// Authentication result that also carries the session token to set as a cookie.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthSuccess {
    /// Authenticated user info.
    pub user: UserInfo,
    /// Newly issued session ID.
    pub session_id: String,
}

/// Normalize an email for case-insensitive comparisons.
#[must_use]
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Create a new account, start a session, and emit a verification email.
pub async fn signup(
    state: &AnkhWebState,
    username: String,
    email: String,
    password: String,
    invite_token: Option<String>,
    org_invite_token: Option<String>,
) -> ApiResult<AuthSuccess> {
    validate_username(username.as_str()).map_err(bad_request)?;
    let username = ankh_names::normalize_name(username.as_str());
    let email = normalize_email(email.as_str());
    validate_email(email.as_str()).map_err(bad_request)?;
    validate_password(password.as_str()).map_err(bad_request)?;
    enforce_signup_rate_limit(email.as_str())?;

    let invite_token = invite_token
        .and_then(|token| (!token.trim().is_empty()).then_some(token.trim().to_string()));
    let org_invite_token = org_invite_token
        .and_then(|token| (!token.trim().is_empty()).then_some(token.trim().to_string()));

    let (session_id, verification_token, waitlisted) = {
        let mut db = state
            .db_pool()
            .get()
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let settings = db
            .get_app_settings()
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let mut waitlist_enabled = settings.waitlist_enabled;

        let org_invite: Option<OrgInvite> = if let Some(ref token) = org_invite_token {
            let invite = match db.get_org_invite(token).await {
                Ok(invite) => invite,
                Err(DbError::OrgInviteNotFound(_))
                | Err(DbError::OrgInviteExpired(_))
                | Err(DbError::OrgInviteRevoked)
                | Err(DbError::OrgInviteAlreadyAccepted) => {
                    return Err(bad_request(errors::INVALID_ORG_INVITE));
                }
                Err(err) => return Err(ApiError::internal(err.to_string())),
            };
            if invite.email != email {
                return Err(bad_request(errors::INVALID_ORG_INVITE));
            }
            waitlist_enabled = false;
            Some(invite)
        } else {
            None
        };

        if let Some(token) = invite_token.as_deref() {
            let invited_email = match db.peek_invite(token).await {
                Ok(email) => email,
                Err(DbError::InviteNotFound(_)) | Err(DbError::InviteExpired(_)) => {
                    return Err(bad_request(errors::INVALID_INVITE));
                }
                Err(err) => return Err(ApiError::internal(err.to_string())),
            };
            if invited_email != email {
                return Err(bad_request(errors::INVALID_INVITE));
            }

            match db.is_user_waitlisted(email.as_str()).await {
                Ok(_) => return Err(ApiError::conflict(errors::ACCOUNT_EXISTS)),
                Err(DbError::UserMissing(_)) => {}
                Err(err) => return Err(ApiError::internal(err.to_string())),
            }

            waitlist_enabled = false;
        }

        match db
            .add_user(username.as_str(), email.as_str(), password.as_str())
            .await
        {
            Ok(()) => {}
            Err(DbError::UserExists(_)) => {
                return Err(ApiError::conflict(errors::ACCOUNT_EXISTS));
            }
            Err(DbError::NamespaceExists(_)) => {
                return Err(ApiError::conflict(errors::USERNAME_TAKEN));
            }
            Err(DbError::InvalidNamespaceName(msg)) => return Err(ApiError::bad_request(msg)),
            Err(err) => return Err(ApiError::internal(err.to_string())),
        }

        if waitlist_enabled {
            db.set_user_waitlisted(email.as_str(), true)
                .await
                .map_err(|err| ApiError::internal(err.to_string()))?;
        }

        if let Some(token) = invite_token.as_deref() {
            match db.consume_invite(token).await {
                Ok(_) => {}
                Err(DbError::InviteNotFound(_)) | Err(DbError::InviteExpired(_)) => {
                    return Err(bad_request(errors::INVALID_INVITE));
                }
                Err(err) => return Err(ApiError::internal(err.to_string())),
            }
        }

        if org_invite.is_some() {
            let user = db
                .get_user_by_name(username.as_str())
                .await
                .map_err(|err| ApiError::internal(err.to_string()))?;
            if let Some(ref token) = org_invite_token {
                match db.accept_org_invite(token, user.id).await {
                    Ok(()) => {}
                    Err(DbError::OrgInviteNotFound(_))
                    | Err(DbError::OrgInviteExpired(_))
                    | Err(DbError::OrgInviteRevoked)
                    | Err(DbError::OrgInviteAlreadyAccepted) => {
                        return Err(bad_request(errors::INVALID_ORG_INVITE));
                    }
                    Err(err) => return Err(ApiError::internal(err.to_string())),
                }
            }
        }

        let token = db
            .create_token(
                email.as_str(),
                TokenKind::EmailVerification,
                EMAIL_VERIFICATION_TTL,
            )
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;

        let session_id = db
            .signin(email.as_str(), password.as_str(), DEFAULT_SESSION_TTL)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        (session_id, token, waitlist_enabled)
    };

    // Waitlisted signups confirm nothing yet: skip the verification email so a
    // waitlist launch needs no real mailer and mints no dead tokens. The account
    // is verified later, after release.
    if !waitlisted {
        let verification_url = state
            .mail()
            .action_url("/verify-email", verification_token.as_str());
        let vars = HashMap::from([("action_url".to_string(), verification_url)]);
        let email_to_send =
            state
                .mail()
                .render_email(template::EMAIL_VERIFICATION, email.as_str(), &vars)?;
        state.mail().send(&email_to_send).await?;
    }

    Ok(AuthSuccess {
        user: UserInfo {
            username,
            email,
            email_verified: false,
            waitlisted,
        },
        session_id,
    })
}

/// Authenticate a user and return session-backed user info.
pub async fn login(
    state: &AnkhWebState,
    email: String,
    password: String,
) -> ApiResult<AuthSuccess> {
    // Normalize the identifier once so that rate limiting, sign-in, and the
    // user lookup all key off the same value. An email is lowercased and
    // validated; a username is lowercased to match the stored namespace name.
    let identifier = email.trim().to_string();
    let input_is_email = identifier.contains('@');
    let identifier = if input_is_email {
        let normalized = normalize_email(identifier.as_str());
        validate_email(normalized.as_str()).map_err(bad_request)?;
        normalized
    } else {
        identifier.to_lowercase()
    };
    enforce_login_rate_limit(identifier.as_str())?;

    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let session_id = db
        .signin(identifier.as_str(), password.as_str(), DEFAULT_SESSION_TTL)
        .await
        .map_err(|err| match err {
            DbError::InvalidCredentials => ApiError::unauthorized(errors::INVALID_CREDENTIALS),
            _ => ApiError::internal(err.to_string()),
        })?;
    let user = if input_is_email {
        db.get_user_by_email(identifier.as_str())
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?
    } else {
        db.get_user_by_name(identifier.as_str())
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?
    };
    let email_verified = db
        .is_email_verified(user.email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let waitlisted = db
        .is_user_waitlisted(user.email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(AuthSuccess {
        session_id,
        user: UserInfo {
            username: user.username,
            email: user.email,
            email_verified,
            waitlisted,
        },
    })
}

/// Clear the current session and revoke the server-side state.
pub async fn logout(state: &AnkhWebState, session_id: Option<String>) -> ApiResult<()> {
    let Some(session_id) = session_id else {
        return Ok(());
    };

    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    match db.delete_session(session_id.as_str()).await {
        Ok(()) | Err(DbError::SessionMissing(_)) => Ok(()),
        Err(err) => Err(ApiError::internal(err.to_string())),
    }
}

/// Return the current session user, if one is present.
pub async fn get_current_user(
    state: &AnkhWebState,
    session: Option<Session>,
) -> ApiResult<Option<UserInfo>> {
    let Some(session) = session else {
        return Ok(None);
    };

    let email = session.email.clone();
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let user = db
        .get_user_by_email(email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let email_verified = db
        .is_email_verified(email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let waitlisted = db
        .is_user_waitlisted(email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(Some(UserInfo {
        username: user.username,
        email: session.email,
        email_verified,
        waitlisted,
    }))
}

/// Return whether waitlist mode is enabled.
pub async fn waitlist_status(state: &AnkhWebState) -> ApiResult<bool> {
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    db.get_app_settings()
        .await
        .map(|settings| settings.waitlist_enabled)
        .map_err(|err| ApiError::internal(err.to_string()))
}

/// Verify an email token and mark the account as verified.
pub async fn verify_email(state: &AnkhWebState, token: String) -> ApiResult<()> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(bad_request(errors::INVALID_TOKEN));
    }

    let mut db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let email = match db
        .consume_token(token.as_str(), TokenKind::EmailVerification)
        .await
    {
        Ok(email) => email,
        Err(DbError::TokenNotFound(_)) | Err(DbError::TokenExpired(_)) => {
            return Err(bad_request(errors::INVALID_TOKEN));
        }
        Err(err) => return Err(ApiError::internal(err.to_string())),
    };

    db.mark_email_verified(email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    db.delete_tokens_for_email(email.as_str(), TokenKind::EmailVerification)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    Ok(())
}

/// Resend the verification email if cooldown and verification allow it.
pub async fn resend_verification(state: &AnkhWebState, session: &Session) -> ApiResult<()> {
    let email = session.email.clone();
    let token = {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let email_verified = db
            .is_email_verified(email.as_str())
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        if email_verified {
            return Ok(());
        }

        if let Some(created_at) = db
            .latest_token_created_at(email.as_str(), TokenKind::EmailVerification)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?
        {
            let created_at_secs: u64 = created_at.timestamp().try_into().unwrap_or(0);
            if now_epoch_secs().saturating_sub(created_at_secs)
                < VERIFICATION_RESEND_COOLDOWN.as_secs()
            {
                return Err(too_many_requests(errors::RESEND_TOO_SOON));
            }
        }

        db.delete_tokens_for_email(email.as_str(), TokenKind::EmailVerification)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;

        db.create_token(
            email.as_str(),
            TokenKind::EmailVerification,
            EMAIL_VERIFICATION_TTL,
        )
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
    };

    let verification_url = state.mail().action_url("/verify-email", token.as_str());
    let vars = HashMap::from([("action_url".to_string(), verification_url)]);
    let email_to_send =
        state
            .mail()
            .render_email(template::EMAIL_VERIFICATION, email.as_str(), &vars)?;
    state.mail().send(&email_to_send).await?;
    Ok(())
}

/// Request a password reset email for the supplied address.
pub async fn request_password_reset(state: &AnkhWebState, email: String) -> ApiResult<()> {
    let email = normalize_email(email.as_str());
    validate_email(email.as_str()).map_err(bad_request)?;
    enforce_password_reset_rate_limit(email.as_str())?;

    let token = {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        db.delete_tokens_for_email(email.as_str(), TokenKind::PasswordReset)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;

        match db
            .create_token(email.as_str(), TokenKind::PasswordReset, PASSWORD_RESET_TTL)
            .await
        {
            Ok(token) => Some(token),
            Err(DbError::UserMissing(_)) => None,
            Err(err) => return Err(ApiError::internal(err.to_string())),
        }
    };

    let Some(token) = token else {
        return Ok(());
    };

    let reset_url = state.mail().action_url("/reset-password", token.as_str());
    let vars = HashMap::from([("action_url".to_string(), reset_url)]);
    let email_to_send =
        state
            .mail()
            .render_email(template::PASSWORD_RESET, email.as_str(), &vars)?;
    state.mail().send(&email_to_send).await?;
    Ok(())
}

/// Validate a password reset token without consuming it.
pub async fn validate_reset_token(state: &AnkhWebState, token: String) -> ApiResult<bool> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Ok(false);
    }

    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    match db
        .peek_token(token.as_str(), TokenKind::PasswordReset)
        .await
    {
        Ok(_) => Ok(true),
        Err(DbError::TokenNotFound(_)) | Err(DbError::TokenExpired(_)) => Ok(false),
        Err(err) => Err(ApiError::internal(err.to_string())),
    }
}

/// Reset a password using a valid reset token.
pub async fn reset_password(
    state: &AnkhWebState,
    token: String,
    new_password: String,
) -> ApiResult<()> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(bad_request(errors::INVALID_TOKEN));
    }
    validate_password(new_password.as_str()).map_err(bad_request)?;

    let revoked = {
        let mut db = state
            .db_pool()
            .get()
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let email = match db
            .consume_token(token.as_str(), TokenKind::PasswordReset)
            .await
        {
            Ok(email) => email,
            Err(DbError::TokenNotFound(_)) | Err(DbError::TokenExpired(_)) => {
                return Err(bad_request(errors::INVALID_TOKEN));
            }
            Err(err) => return Err(ApiError::internal(err.to_string())),
        };

        db.set_password(email.as_str(), new_password.as_str())
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let user = db
            .get_user_by_email(email.as_str())
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let active_device_sessions = db
            .list_device_sessions_for_user(user.id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        db.revoke_all_device_sessions(user.id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        db.delete_sessions_for_email(email.as_str())
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        db.delete_tokens_for_email(email.as_str(), TokenKind::PasswordReset)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;

        DeviceSessionsRevoked {
            user_id: user.id,
            session_ids: active_device_sessions
                .into_iter()
                .map(|session| session.id)
                .collect(),
        }
    };
    dispatch_device_sessions_revoked(state, revoked).await;
    Ok(())
}

/// Validate email format for server-side checks.
pub fn validate_email(email: &str) -> Result<(), &'static str> {
    let email = normalize_email(email);
    let (local, domain) = email.split_once('@').ok_or(errors::INVALID_EMAIL)?;
    if local.is_empty() || domain.is_empty() || !domain.contains('.') {
        return Err(errors::INVALID_EMAIL);
    }
    Ok(())
}

/// Validate password policy for server-side checks.
pub fn validate_password(password: &str) -> Result<(), &'static str> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(errors::PASSWORD_TOO_SHORT);
    }
    Ok(())
}

/// Validate username format and reserved names.
pub fn validate_username(username: &str) -> Result<(), &'static str> {
    ankh_names::validate_name_format(username).map_err(|_| errors::INVALID_USERNAME)?;
    if ankh_names::NamePolicy::shared()
        .validate_namespace_name(username)
        .is_err()
    {
        return Err(errors::RESERVED_USERNAME);
    }
    Ok(())
}

/// Dispatch device-session revocation hooks best-effort.
async fn dispatch_device_sessions_revoked(state: &AnkhWebState, payload: DeviceSessionsRevoked) {
    if payload.session_ids.is_empty() {
        return;
    }
    if let Err(error) = state.hooks().on_device_sessions_revoked(payload).await {
        state.record_hook_failure("on_device_sessions_revoked", error);
    }
}
