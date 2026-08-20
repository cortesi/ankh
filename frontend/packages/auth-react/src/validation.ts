const RESERVED_NAMESPACE_NAMES = new Set([
  "api",
  "admin",
  "healthz",
  "assets",
  "static",
  "dev",
  "login",
  "logout",
  "signup",
  "signin",
  "signout",
  "register",
  "verify-email",
  "forgot-password",
  "reset-password",
  "waitlist",
  "dashboard",
  "settings",
  "account",
  "profile",
  "console",
  "org",
  "orgs",
  "organization",
  "organizations",
  "team",
  "teams",
  "user",
  "users",
  "member",
  "members",
  "new",
  "create",
  "edit",
  "delete",
  "remove",
  "www",
  "mail",
  "ftp",
  "smtp",
  "imap",
  "pop",
  "support",
  "help",
  "info",
  "contact",
  "billing",
  "sales",
  "root",
  "system",
  "null",
  "undefined",
  "anonymous",
]);

export const PASSWORD_MIN_LENGTH = 8;

export function normalizeName(value: string) {
  return value.trim().toLowerCase();
}

export function validateEmail(value: string) {
  const normalized = value.trim().toLowerCase();
  if (!normalized) {
    return "Email is required";
  }
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(normalized)) {
    return "Invalid email address";
  }
  return null;
}

export function validateLoginIdentifier(value: string) {
  if (!value.trim()) {
    return "Email or username is required";
  }
  if (value.includes("@")) {
    return validateEmail(value);
  }
  return null;
}

export function validatePassword(value: string) {
  if (value.length < PASSWORD_MIN_LENGTH) {
    return "Password too short";
  }
  return null;
}

export function validateUsername(value: string) {
  const normalized = normalizeName(value);
  const formatError = validateNameFormat(normalized);
  if (formatError) {
    return formatError;
  }
  if (RESERVED_NAMESPACE_NAMES.has(normalized)) {
    return "This username is reserved";
  }
  return null;
}

export function validateOrgName(value: string) {
  const normalized = normalizeName(value);
  const formatError = validateNameFormat(normalized);
  if (formatError) {
    return formatError;
  }
  if (RESERVED_NAMESPACE_NAMES.has(normalized)) {
    return "This name is reserved";
  }
  return null;
}

function validateNameFormat(value: string) {
  if (value.length < 3) {
    return "must be at least 3 characters";
  }
  if (value.length > 39) {
    return "must be at most 39 characters";
  }
  if (!/^[a-z0-9-]+$/.test(value)) {
    return "must contain only lowercase letters, numbers, and hyphens";
  }
  if (!/^[a-z0-9]/.test(value) || !/[a-z0-9]$/.test(value)) {
    return "must start and end with a letter or number";
  }
  if (value.includes("--")) {
    return "cannot contain consecutive hyphens";
  }
  return null;
}
