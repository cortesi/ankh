-- Canonical PostgreSQL schema for Ankh identity data.
CREATE TABLE IF NOT EXISTS ankh_schema_version (
    version INTEGER PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS ankh_settings (
    id INTEGER PRIMARY KEY,
    waitlist_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Namespaces provide a shared name registry for users and organizations.
-- A name can belong to either a user or an org, but not both.
CREATE TABLE IF NOT EXISTS namespaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('user', 'org')),
    tier TEXT NOT NULL DEFAULT 'free' CHECK (tier IN ('free', 'pro')),
    limits_override JSONB,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended')),
    gen BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (name = lower(name)),
    CHECK (char_length(name) BETWEEN 3 AND 39),
    CHECK (name ~ '^[a-z0-9][a-z0-9-]*[a-z0-9]$' OR char_length(name) = 3),
    CHECK (position('--' in name) = 0)
);

CREATE TABLE IF NOT EXISTS users (
    id UUID UNIQUE NOT NULL DEFAULT gen_random_uuid(),
    email TEXT PRIMARY KEY,
    namespace_id UUID NOT NULL UNIQUE REFERENCES namespaces(id) ON DELETE RESTRICT,
    password_hash TEXT NOT NULL,
    waitlist_status TEXT NOT NULL DEFAULT 'active'
        CHECK (waitlist_status IN ('active', 'waitlisted')),
    email_verified_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS sessions (
    id UUID UNIQUE NOT NULL DEFAULT gen_random_uuid(),
    token_hash TEXT PRIMARY KEY,
    email TEXT NOT NULL REFERENCES users(email) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    touched_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS device_auth_grants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash TEXT NOT NULL UNIQUE,
    code_challenge TEXT NOT NULL,
    state TEXT NOT NULL,
    redirect_port INTEGER NOT NULL,
    device_name TEXT NOT NULL,
    platform TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    CHECK (redirect_port > 0 AND redirect_port <= 65535)
);

CREATE TABLE IF NOT EXISTS device_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    device_name TEXT NOT NULL,
    platform TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS tokens (
    token_hash TEXT PRIMARY KEY,
    email TEXT NOT NULL REFERENCES users(email) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS tokens_email_kind_idx ON tokens(email, kind);

CREATE TABLE IF NOT EXISTS invites (
    token_hash TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS invites_email_idx ON invites(email);
CREATE INDEX IF NOT EXISTS sessions_email_idx ON sessions(email);
CREATE INDEX IF NOT EXISTS sessions_expires_at_idx ON sessions(expires_at);
CREATE INDEX IF NOT EXISTS device_auth_grants_user_id_idx ON device_auth_grants(user_id);
CREATE INDEX IF NOT EXISTS device_auth_grants_expires_at_idx ON device_auth_grants(expires_at);
CREATE INDEX IF NOT EXISTS device_sessions_user_id_idx ON device_sessions(user_id);
CREATE INDEX IF NOT EXISTS device_sessions_expires_at_idx ON device_sessions(expires_at);

-- Sysadmin authentication tables are separate from user auth.
CREATE TABLE IF NOT EXISTS sysadmins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_login_at TIMESTAMPTZ,
    disabled_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS sysadmin_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sysadmin_id UUID NOT NULL REFERENCES sysadmins(id) ON DELETE CASCADE,
    token_hash TEXT UNIQUE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS sysadmin_tokens_sysadmin_id_idx ON sysadmin_tokens(sysadmin_id);
CREATE INDEX IF NOT EXISTS sysadmin_tokens_expires_at_idx ON sysadmin_tokens(expires_at);

-- Organizations are shared workspaces with membership and roles.
CREATE TABLE IF NOT EXISTS organizations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    namespace_id UUID NOT NULL UNIQUE REFERENCES namespaces(id) ON DELETE RESTRICT,
    display_name TEXT,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (display_name IS NULL OR char_length(display_name) <= 100)
);

-- Organization membership with roles.
CREATE TABLE IF NOT EXISTS org_members (
    org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member')),
    added_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (org_id, user_id)
);

-- Enforce exactly one owner per organization.
CREATE UNIQUE INDEX IF NOT EXISTS org_single_owner ON org_members (org_id) WHERE role = 'owner';
CREATE INDEX IF NOT EXISTS org_members_user_id_idx ON org_members(user_id);

-- Organization invitations.
CREATE TABLE IF NOT EXISTS org_invites (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash TEXT NOT NULL UNIQUE,
    org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    email TEXT NOT NULL,
    invited_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    accepted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    CHECK (email = lower(email))
);

-- Only one pending (not accepted, not revoked) invite per org and email.
CREATE UNIQUE INDEX IF NOT EXISTS org_invites_pending_email
    ON org_invites (org_id, email)
    WHERE accepted_at IS NULL AND revoked_at IS NULL;

CREATE INDEX IF NOT EXISTS org_invites_org_id_idx ON org_invites(org_id);
CREATE INDEX IF NOT EXISTS org_invites_email_idx ON org_invites(email);
CREATE INDEX IF NOT EXISTS org_invites_expires_at_idx ON org_invites(expires_at);
