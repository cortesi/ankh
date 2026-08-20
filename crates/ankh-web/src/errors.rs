//! UI-facing error messages for shared auth, org, and device flows.

/// Error shown for malformed email addresses.
pub const INVALID_EMAIL: &str = "Invalid email address";
/// Error shown when a password is too short.
pub const PASSWORD_TOO_SHORT: &str = "Password too short";
/// Error shown for invalid usernames.
pub const INVALID_USERNAME: &str = "Invalid username";
/// Error shown when a username uses a reserved name.
pub const RESERVED_USERNAME: &str = "This username is reserved";
/// Error shown when credentials do not match.
pub const INVALID_CREDENTIALS: &str = "Invalid email or password";
/// Error shown when signup attempts an existing account.
pub const ACCOUNT_EXISTS: &str = "Account already exists";
/// Error shown when a username is already taken.
pub const USERNAME_TAKEN: &str = "Username already taken";
/// Error shown for invalid or expired tokens.
pub const INVALID_TOKEN: &str = "Invalid or expired token";
/// Error shown when an invite token is invalid or mismatched.
pub const INVALID_INVITE: &str = "Invalid or expired invite";
/// Error shown when an org invite token is invalid or mismatched.
pub const INVALID_ORG_INVITE: &str = "Invalid or expired organization invite";
/// Error shown when resend requests are too frequent.
pub const RESEND_TOO_SOON: &str = "Please wait before requesting another email";
/// Error shown when auth is required.
pub const UNAUTHORIZED: &str = "Please log in";
/// Error shown when a waitlisted user attempts to use the product.
pub const WAITLISTED: &str = "You're on the waitlist. We'll email you when your account is ready.";
/// Error shown when rate limits are exceeded.
pub const RATE_LIMITED: &str = "Too many requests. Please try again later.";
/// Error shown when an org name is already taken.
pub const ORG_NAME_TAKEN: &str = "Organization name already taken";
/// Error shown for invalid org ID format.
pub const INVALID_ORG_ID: &str = "Invalid organization ID";
/// Error shown when an org is not found.
pub const ORG_NOT_FOUND: &str = "Organization not found";
/// Error shown when user is not a member of the org.
pub const NOT_ORG_MEMBER: &str = "Not a member of this organization";
/// Error shown when owner tries to leave without transferring ownership.
pub const OWNER_CANNOT_LEAVE: &str = "Transfer ownership before leaving";
/// Error shown when user lacks permission for an action.
pub const PERMISSION_DENIED: &str = "Permission denied";
/// Error shown when an invite already exists for this email.
pub const INVITE_ALREADY_PENDING: &str = "An invite is already pending for this email";
/// Error shown when user is already a member.
pub const ALREADY_ORG_MEMBER: &str = "User is already a member";
/// Error shown for invalid invite ID format.
pub const INVALID_INVITE_ID: &str = "Invalid invite ID";
/// Error shown for invalid user ID format.
pub const INVALID_USER_ID: &str = "Invalid user ID";
/// Error shown when invite email does not match the logged-in user.
pub const INVITE_EMAIL_MISMATCH: &str = "This invite was sent to a different email address";
/// Error shown when trying to manage members of a personal org.
pub const PERSONAL_ORG_NO_MEMBERS: &str = "Personal organizations cannot have additional members";
/// Error shown for invalid device session ID format.
pub const INVALID_DEVICE_SESSION_ID: &str = "Invalid device session ID";
